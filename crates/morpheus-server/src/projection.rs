use morpheus_protocol::{
    ValidatedMarketplaceEvent, ValidationCode, ValidationError, ValidationResult,
};
use morpheus_store::{
    CatalogOfferProjectionRecord, EventStore, MarketplaceEventRecord, OrderProjectionRecord,
    ProjectionErrorRecord,
};
use serde_json::{Value, json};

pub async fn persist_and_project<S>(
    store: &S,
    event: &ValidatedMarketplaceEvent,
    protocol_version: &str,
    created_at: &str,
) -> ValidationResult<()>
where
    S: EventStore,
{
    store
        .record_marketplace_event(MarketplaceEventRecord {
            marketplace_event_id: event.marketplace_event_id.clone(),
            matrix_event_id: event.matrix_event_id.clone(),
            protocol_version: protocol_version.into(),
            issuer_instance: event.issuer.instance_id.clone(),
            actor_id: event.issuer.actor_id.clone(),
            event_type: event.event_type.clone(),
            body: event.body.clone(),
            created_at: created_at.into(),
        })
        .await?;

    if let Err(err) = project_event(store, event).await {
        store
            .record_projection_error(ProjectionErrorRecord {
                matrix_event_id: Some(event.matrix_event_id.clone()),
                code: err.code,
                message: err.message.clone(),
                details: err.details.clone(),
            })
            .await?;
        return Err(err);
    }

    Ok(())
}

async fn project_event<S>(store: &S, event: &ValidatedMarketplaceEvent) -> ValidationResult<()>
where
    S: EventStore,
{
    match event.event_type.as_str() {
        "io.marketplace.actor.seller.announced" => {
            store
                .upsert_catalog_seller(
                    required_str(&event.body, "seller_id")?,
                    event.issuer.instance_id.as_str(),
                    required_str(&event.body, "status")?,
                    event.body.clone(),
                )
                .await
        }
        "io.marketplace.actor.seller.suspended" => {
            store
                .upsert_catalog_seller(
                    required_str(&event.body, "seller_id")?,
                    event.issuer.instance_id.as_str(),
                    "suspended",
                    event.body.clone(),
                )
                .await
        }
        "io.marketplace.product.upserted" => {
            store
                .upsert_catalog_product(
                    required_str(&event.body, "product_id")?,
                    required_str(&event.body, "seller_id")?,
                    required_i64(&event.body, "revision")?,
                    event.body.clone(),
                )
                .await
        }
        "io.marketplace.product.withdrawn" => {
            store
                .tombstone_catalog_object(
                    required_str(&event.body, "product_id")?,
                    "product",
                    event.body.clone(),
                )
                .await
        }
        "io.marketplace.offer.upserted" => {
            let inventory_kind = event
                .body
                .get("availability")
                .and_then(Value::as_object)
                .and_then(|availability| availability.get("mode"))
                .and_then(Value::as_str)
                .ok_or_else(|| missing("availability.mode"))?;
            store
                .upsert_catalog_offer(CatalogOfferProjectionRecord {
                    offer_id: required_str(&event.body, "offer_id")?.into(),
                    product_id: required_str(&event.body, "product_id")?.into(),
                    seller_id: required_str(&event.body, "seller_id")?.into(),
                    revision: required_i64(&event.body, "revision")?,
                    price: event
                        .body
                        .get("price")
                        .cloned()
                        .ok_or_else(|| missing("price"))?,
                    inventory_kind: inventory_kind.into(),
                    body: event.body.clone(),
                })
                .await
        }
        "io.marketplace.offer.withdrawn" => {
            store
                .tombstone_catalog_object(
                    required_str(&event.body, "offer_id")?,
                    "offer",
                    event.body.clone(),
                )
                .await
        }
        "io.marketplace.inventory.updated" => {
            store
                .tombstone_catalog_object(
                    required_str(&event.body, "offer_id")?,
                    "inventory",
                    event.body.clone(),
                )
                .await
        }
        event_type if event_type.starts_with("io.marketplace.order.") => {
            project_order_event(store, event, order_status(event_type)?).await
        }
        event_type if event_type.starts_with("io.marketplace.payment.") => {
            project_order_event(store, event, None).await?;
            store
                .upsert_payment(
                    required_str(&event.body, "payment_id")?,
                    required_str(&event.body, "order_id")?,
                    payment_status(event_type)?,
                    event.body.clone(),
                )
                .await
        }
        event_type if event_type.starts_with("io.marketplace.entitlement.") => {
            project_order_event(store, event, None).await?;
            store
                .upsert_entitlement(
                    required_str(&event.body, "entitlement_id")?,
                    required_str(&event.body, "order_id")?,
                    entitlement_status(event_type)?,
                    event.body.clone(),
                )
                .await
        }
        event_type if event_type.starts_with("io.marketplace.dispute.") => {
            project_order_event(store, event, None).await?;
            if event_type == "io.marketplace.dispute.ruling.issued" {
                let dispute_id = required_str(&event.body, "dispute_id")?;
                store
                    .upsert_arbitration_ruling(
                        event.marketplace_event_id.as_str(),
                        dispute_id,
                        required_str(&event.body, "ruling")?,
                        event.body.clone(),
                    )
                    .await
            } else {
                store
                    .upsert_dispute(
                        required_str(&event.body, "dispute_id")?,
                        required_str(&event.body, "order_id")?,
                        dispute_status(event_type)?,
                        event.body.clone(),
                    )
                    .await
            }
        }
        _ => Ok(()),
    }
}

async fn project_order_event<S>(
    store: &S,
    event: &ValidatedMarketplaceEvent,
    status: Option<&str>,
) -> ValidationResult<()>
where
    S: EventStore,
{
    let order_id = required_str(&event.body, "order_id")?;
    store
        .record_order_event(
            order_id,
            event.marketplace_event_id.as_str(),
            event.event_type.as_str(),
            event.body.clone(),
        )
        .await?;

    if event.event_type == "io.marketplace.order.created" {
        store
            .upsert_order(OrderProjectionRecord {
                order_id: order_id.into(),
                room_id: required_str(&event.body, "room_id")?.into(),
                customer_id: required_str(&event.body, "customer_id")?.into(),
                seller_id: required_str(&event.body, "seller_id")?.into(),
                offer_id: required_str(&event.body, "offer_id")?.into(),
                status: "created".into(),
                body: event.body.clone(),
            })
            .await?;
    } else if let Some(status) = status
        && let Some(mut order) = store.order(order_id).await?
    {
        order.status = status.into();
        store
            .upsert_order(OrderProjectionRecord {
                order_id: order.order_id,
                room_id: order.room_id,
                customer_id: order.customer_id,
                seller_id: order.seller_id,
                offer_id: order.offer_id,
                status: order.status,
                body: order.body,
            })
            .await?;
    }

    Ok(())
}

fn order_status(event_type: &str) -> ValidationResult<Option<&'static str>> {
    Ok(match event_type {
        "io.marketplace.order.created" => Some("created"),
        "io.marketplace.order.accepted" => Some("accepted"),
        "io.marketplace.order.rejected" => Some("rejected"),
        "io.marketplace.order.cancelled" => Some("cancelled"),
        "io.marketplace.order.completed" => Some("completed"),
        _ => None,
    })
}

fn payment_status(event_type: &str) -> ValidationResult<&'static str> {
    match event_type {
        "io.marketplace.payment.intent.created" => Ok("intent_created"),
        "io.marketplace.payment.authorized" => Ok("authorized"),
        "io.marketplace.payment.captured" => Ok("captured"),
        "io.marketplace.payment.failed" => Ok("failed"),
        "io.marketplace.payment.cancelled" => Ok("cancelled"),
        "io.marketplace.payment.refund.requested" => Ok("refund_requested"),
        "io.marketplace.payment.refunded" => Ok("refunded"),
        "io.marketplace.payment.chargeback.opened" => Ok("chargeback_opened"),
        _ => Err(unknown_projection(event_type)),
    }
}

fn entitlement_status(event_type: &str) -> ValidationResult<&'static str> {
    match event_type {
        "io.marketplace.entitlement.granted" => Ok("granted"),
        "io.marketplace.entitlement.activated" => Ok("activated"),
        "io.marketplace.entitlement.completed" => Ok("completed"),
        "io.marketplace.entitlement.revoked" => Ok("revoked"),
        "io.marketplace.entitlement.expired" => Ok("expired"),
        _ => Err(unknown_projection(event_type)),
    }
}

fn dispute_status(event_type: &str) -> ValidationResult<&'static str> {
    match event_type {
        "io.marketplace.dispute.opened" => Ok("opened"),
        "io.marketplace.dispute.evidence.submitted" => Ok("evidence_submitted"),
        "io.marketplace.dispute.closed" => Ok("closed"),
        _ => Err(unknown_projection(event_type)),
    }
}

fn required_str<'a>(body: &'a Value, field: &str) -> ValidationResult<&'a str> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing(field))
}

fn required_i64(body: &Value, field: &str) -> ValidationResult<i64> {
    body.get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| missing(field))
}

fn missing(field: &str) -> ValidationError {
    ValidationError::with_details(
        ValidationCode::MissingRequiredField,
        format!("Missing projection field {field}"),
        json!({ "field": field }),
    )
}

fn unknown_projection(event_type: &str) -> ValidationError {
    ValidationError::with_details(
        ValidationCode::UnknownEventType,
        format!("No projection mapping for {event_type}"),
        json!({ "eventType": event_type }),
    )
}
