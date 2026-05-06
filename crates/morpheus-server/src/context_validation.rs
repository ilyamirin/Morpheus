use morpheus_core::{
    AllowlistPolicy, CatalogIndex, CustomerBinding, Money, OfferRecord, OrderCreatedBody,
    OrderFlowEvent, ProductRecord, SellerRecord, validate_order_created, validate_order_sequence,
};
use morpheus_protocol::{
    ValidatedMarketplaceEvent, ValidationCode, ValidationError, ValidationResult,
    is_order_event_type, parse_object_instance,
};
use morpheus_store::EventStore;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub async fn validate_event_context<S>(
    store: &S,
    event: &ValidatedMarketplaceEvent,
) -> ValidationResult<()>
where
    S: EventStore,
{
    if !is_order_event_type(&event.event_type) {
        return Ok(());
    }

    let mut flow_events = store
        .marketplace_events_by_room(&event.room_id)
        .await?
        .into_iter()
        .filter(|record| is_order_event_type(&record.event_type))
        .map(|record| OrderFlowEvent {
            event_type: record.event_type,
            body: record.body,
        })
        .collect::<Vec<_>>();

    flow_events.push(OrderFlowEvent {
        event_type: event.event_type.clone(),
        body: event.body.clone(),
    });
    validate_order_sequence(&flow_events)?;

    if event.event_type == "io.marketplace.order.created" {
        validate_order_created_context(store, event, &flow_events).await?;
    }

    Ok(())
}

async fn validate_order_created_context<S>(
    store: &S,
    event: &ValidatedMarketplaceEvent,
    flow_events: &[OrderFlowEvent],
) -> ValidationResult<()>
where
    S: EventStore,
{
    let order: OrderCreatedBody = serde_json::from_value(event.body.clone()).map_err(|err| {
        ValidationError::with_details(
            ValidationCode::MissingRequiredField,
            "order.created body does not match core schema",
            json!({ "error": err.to_string() }),
        )
    })?;
    let seller_instance = parse_object_instance(&order.offer_id)?;
    let catalog = catalog_index_for_instance(store, seller_instance).await?;
    let allowlist = AllowlistPolicy::new([
        (seller_instance.to_string(), vec!["orders".into()]),
        (order.arbiter_instance.clone(), vec!["arbitration".into()]),
    ]);
    let customer = customer_binding_for_order(flow_events, &order.customer_id)?;
    validate_order_created(&order, &catalog, &allowlist, &customer)
}

async fn catalog_index_for_instance<S>(
    store: &S,
    instance_id: &str,
) -> ValidationResult<CatalogIndex>
where
    S: EventStore,
{
    let mut catalog = CatalogIndex::new(instance_id);
    let mut seller_ids = BTreeSet::new();
    let mut product_ids = BTreeSet::new();

    for seller in store.catalog_sellers().await? {
        if parse_object_instance(&seller.seller_id)? == instance_id {
            seller_ids.insert(seller.seller_id.clone());
            catalog.upsert_seller(SellerRecord {
                seller_id: seller.seller_id,
                status: seller.status,
            })?;
        }
    }

    for product in store.catalog_products().await? {
        if parse_object_instance(&product.product_id)? == instance_id
            && seller_ids.contains(&product.seller_id)
        {
            product_ids.insert(product.product_id.clone());
            catalog.upsert_product(ProductRecord {
                product_id: product.product_id,
                seller_id: product.seller_id,
                revision: u64::try_from(product.revision).map_err(|_| invalid_revision())?,
                terms_hash: string_field_opt(&product.body, "terms_hash").map(str::to_string),
            })?;
        }
    }

    for offer in store.catalog_offers().await? {
        if parse_object_instance(&offer.offer_id)? == instance_id
            && seller_ids.contains(&offer.seller_id)
            && product_ids.contains(&offer.product_id)
        {
            catalog.upsert_offer(OfferRecord {
                offer_id: offer.offer_id,
                product_id: offer.product_id,
                seller_id: offer.seller_id,
                revision: u64::try_from(offer.revision).map_err(|_| invalid_revision())?,
                price: money_from_value(&offer.price)?,
                entitlement_type: nested_string(&offer.body, "entitlement", "type")?.to_string(),
                payment_capture_policy: nested_string_opt(
                    &offer.body,
                    "payment_terms",
                    "capture_policy",
                )
                .map(str::to_string),
                offer_terms_hash: string_field_opt(&offer.body, "offer_terms_hash")
                    .map(str::to_string),
                seller_terms_hash: string_field_opt(&offer.body, "seller_terms_hash")
                    .map(str::to_string),
            })?;
        }
    }

    for tombstone in store.catalog_tombstones().await? {
        if parse_object_instance(&tombstone.object_id)? == instance_id {
            catalog.remove_object(&tombstone.object_id);
        }
    }

    Ok(catalog)
}

fn customer_binding_for_order(
    flow_events: &[OrderFlowEvent],
    customer_id: &str,
) -> ValidationResult<CustomerBinding> {
    flow_events
        .iter()
        .filter(|event| event.event_type == "io.marketplace.actor.customer.bound")
        .rev()
        .find(|event| string_field(&event.body, "customer_id").ok() == Some(customer_id))
        .map(|event| {
            Ok(CustomerBinding {
                customer_id: string_field(&event.body, "customer_id")?.to_string(),
                status: string_field(&event.body, "status")?.to_string(),
                accepted_payment_adapters: string_array(&event.body, "accepted_payment_adapters")?,
                accepted_arbitration_policies: string_array(
                    &event.body,
                    "accepted_arbitration_policies",
                )?,
            })
        })
        .transpose()?
        .ok_or_else(|| {
            ValidationError::new(
                ValidationCode::CatalogReferenceMismatch,
                "order.created requires a preceding customer.bound event",
            )
        })
}

fn money_from_value(value: &Value) -> ValidationResult<Money> {
    Ok(Money {
        amount: string_field(value, "amount")?.to_string(),
        currency: string_field(value, "currency")?.to_string(),
    })
}

fn nested_string<'a>(body: &'a Value, object: &str, field: &str) -> ValidationResult<&'a str> {
    body.get(object)
        .ok_or_else(|| missing(object))
        .and_then(|value| string_field(value, field))
}

fn nested_string_opt<'a>(body: &'a Value, object: &str, field: &str) -> Option<&'a str> {
    body.get(object)
        .and_then(|value| string_field(value, field).ok())
}

fn string_field<'a>(body: &'a Value, field: &str) -> ValidationResult<&'a str> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing(field))
}

fn string_field_opt<'a>(body: &'a Value, field: &str) -> Option<&'a str> {
    body.get(field).and_then(Value::as_str)
}

fn string_array(body: &Value, field: &str) -> ValidationResult<Vec<String>> {
    body.get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| missing(field))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .ok_or_else(|| missing(field))
        })
        .collect()
}

fn missing(field: &str) -> ValidationError {
    ValidationError::with_details(
        ValidationCode::MissingRequiredField,
        format!("Missing contextual field {field}"),
        json!({ "field": field }),
    )
}

fn invalid_revision() -> ValidationError {
    ValidationError::new(
        ValidationCode::RevisionRollback,
        "Projection revision cannot be converted to unsigned protocol revision",
    )
}
