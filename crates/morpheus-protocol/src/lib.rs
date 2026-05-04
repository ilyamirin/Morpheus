use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const PROTOCOL_NAME: &str = "io.marketplace";
pub const PROTOCOL_VERSION: &str = "0.1";

pub const CATALOG_EVENT_TYPES: &[&str] = &[
    "io.marketplace.instance.profile",
    "io.marketplace.catalog.profile",
    "io.marketplace.catalog.snapshot.published",
    "io.marketplace.actor.seller.announced",
    "io.marketplace.actor.seller.suspended",
    "io.marketplace.product.upserted",
    "io.marketplace.product.withdrawn",
    "io.marketplace.offer.upserted",
    "io.marketplace.offer.withdrawn",
    "io.marketplace.inventory.updated",
];

pub const ORDER_EVENT_TYPES: &[&str] = &[
    "io.marketplace.actor.customer.bound",
    "io.marketplace.order.created",
    "io.marketplace.order.accepted",
    "io.marketplace.order.cancelled",
    "io.marketplace.order.rejected",
    "io.marketplace.order.completed",
    "io.marketplace.payment.intent.created",
    "io.marketplace.payment.authorized",
    "io.marketplace.payment.captured",
    "io.marketplace.payment.failed",
    "io.marketplace.payment.cancelled",
    "io.marketplace.payment.refund.requested",
    "io.marketplace.payment.refunded",
    "io.marketplace.payment.chargeback.opened",
    "io.marketplace.entitlement.granted",
    "io.marketplace.entitlement.activated",
    "io.marketplace.entitlement.completed",
    "io.marketplace.entitlement.revoked",
    "io.marketplace.entitlement.expired",
    "io.marketplace.dispute.opened",
    "io.marketplace.dispute.evidence.submitted",
    "io.marketplace.dispute.ruling.issued",
    "io.marketplace.dispute.closed",
];

pub const ENTITLEMENT_TYPES: &[&str] = &[
    "download_access",
    "license_key",
    "account_access",
    "service_delivery",
    "booking_slot",
    "subscription_access",
    "external_entitlement",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issuer {
    pub instance_id: String,
    #[serde(default)]
    pub actor_id: Option<String>,
    pub matrix_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceEnvelope {
    pub protocol: String,
    pub protocol_version: String,
    pub event_id: String,
    pub created_at: String,
    pub issuer: Issuer,
    #[serde(default)]
    pub critical: Vec<String>,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixMarketplaceEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub room_id: String,
    pub event_id: String,
    pub sender: String,
    pub origin_server_ts: i64,
    pub content: MarketplaceEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedMarketplaceEvent {
    pub event_type: String,
    pub room_id: String,
    pub matrix_event_id: String,
    pub marketplace_event_id: String,
    pub sender: String,
    pub issuer: Issuer,
    pub body: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValidationCode {
    MissingRequiredField,
    UnsupportedProtocolVersion,
    UnknownEventType,
    UnknownCritical,
    UnauthorizedSender,
    RoomProfileViolation,
    CatalogReferenceMismatch,
    RevisionRollback,
    InvalidStateTransition,
    PaymentTermsMismatch,
    ActorNotActive,
    PolicyViolation,
    DuplicateEvent,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{code:?}: {message}")]
pub struct ValidationError {
    pub code: ValidationCode,
    pub message: String,
    pub details: Value,
}

impl ValidationError {
    pub fn new(code: ValidationCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: json!({}),
        }
    }

    pub fn with_details(code: ValidationCode, message: impl Into<String>, details: Value) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }
}

pub type ValidationResult<T> = Result<T, ValidationError>;

pub fn validate_event_envelope(raw_event: &Value) -> ValidationResult<ValidatedMarketplaceEvent> {
    let event: MatrixMarketplaceEvent =
        serde_json::from_value(raw_event.clone()).map_err(|err| {
            ValidationError::with_details(
                ValidationCode::MissingRequiredField,
                "Invalid Matrix marketplace event",
                json!({ "error": err.to_string() }),
            )
        })?;

    if !is_known_event_type(&event.event_type) {
        return Err(ValidationError::new(
            ValidationCode::UnknownEventType,
            format!("Unknown marketplace event type {}", event.event_type),
        ));
    }
    if event.content.protocol != PROTOCOL_NAME {
        return Err(ValidationError::new(
            ValidationCode::MissingRequiredField,
            "Marketplace protocol must be io.marketplace",
        ));
    }
    if event.content.protocol_version != PROTOCOL_VERSION {
        return Err(ValidationError::new(
            ValidationCode::UnsupportedProtocolVersion,
            "Unsupported marketplace protocol version",
        ));
    }
    if !event.content.critical.is_empty() {
        return Err(ValidationError::with_details(
            ValidationCode::UnknownCritical,
            "Unknown critical fields or extensions are rejected in v0.1",
            json!({ "critical": event.content.critical }),
        ));
    }
    if event.sender != event.content.issuer.matrix_user_id {
        return Err(ValidationError::with_details(
            ValidationCode::UnauthorizedSender,
            "Matrix sender must match content.issuer.matrix_user_id",
            json!({ "sender": event.sender, "issuer": event.content.issuer.matrix_user_id }),
        ));
    }
    if !is_matrix_user_id(&event.sender)
        || !event.room_id.starts_with('!')
        || !event.event_id.starts_with('$')
    {
        return Err(ValidationError::new(
            ValidationCode::MissingRequiredField,
            "Matrix sender, room_id, or event_id has invalid shape",
        ));
    }
    let _created_at = DateTime::parse_from_rfc3339(&event.content.created_at)
        .map_err(|_| {
            ValidationError::new(
                ValidationCode::MissingRequiredField,
                "created_at must be ISO-8601",
            )
        })?
        .with_timezone(&Utc);
    if !event.content.created_at.ends_with('Z') {
        return Err(ValidationError::new(
            ValidationCode::MissingRequiredField,
            "created_at must be UTC and end with Z",
        ));
    }
    if requires_actor_issuer(&event.event_type) && event.content.issuer.actor_id.is_none() {
        return Err(ValidationError::new(
            ValidationCode::MissingRequiredField,
            format!("{} requires issuer.actor_id", event.event_type),
        ));
    }
    if event.event_type == "io.marketplace.order.created" {
        let body_room_id = required_str(&event.content.body, "room_id")?;
        if body_room_id != event.room_id {
            return Err(ValidationError::with_details(
                ValidationCode::CatalogReferenceMismatch,
                "Order room mismatch between Matrix room_id and content.body.room_id",
                json!({ "event_room_id": event.room_id, "body_room_id": body_room_id }),
            ));
        }
    }
    validate_body_shape(&event.event_type, &event.content.body)?;

    Ok(ValidatedMarketplaceEvent {
        event_type: event.event_type,
        room_id: event.room_id,
        matrix_event_id: event.event_id,
        marketplace_event_id: event.content.event_id,
        sender: event.sender,
        issuer: event.content.issuer,
        body: event.content.body,
    })
}

pub fn is_known_event_type(event_type: &str) -> bool {
    CATALOG_EVENT_TYPES.contains(&event_type) || ORDER_EVENT_TYPES.contains(&event_type)
}

pub fn is_catalog_event_type(event_type: &str) -> bool {
    CATALOG_EVENT_TYPES.contains(&event_type)
}

pub fn is_order_event_type(event_type: &str) -> bool {
    ORDER_EVENT_TYPES.contains(&event_type)
}

pub fn parse_object_instance(id: &str) -> ValidationResult<&str> {
    let mut parts = id.split(':');
    let _kind = parts.next();
    let instance = parts.next().ok_or_else(|| {
        ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            format!("Object id {id} does not include an instance id"),
        )
    })?;
    if instance.is_empty() {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Object id instance id is empty",
        ));
    }
    Ok(instance)
}

pub fn canonical_json_sha256(value: &Value) -> ValidationResult<String> {
    let encoded = serde_json::to_vec(value).map_err(|err| {
        ValidationError::with_details(
            ValidationCode::CatalogReferenceMismatch,
            "Failed to encode canonical JSON",
            json!({ "error": err.to_string() }),
        )
    })?;
    let digest = Sha256::digest(encoded);
    Ok(format!("sha256:{}", hex::encode(digest)))
}

fn validate_body_shape(event_type: &str, body: &Value) -> ValidationResult<()> {
    match event_type {
        "io.marketplace.order.created" => {
            for field in [
                "order_id",
                "room_id",
                "customer_id",
                "seller_id",
                "offer_id",
                "payment_adapter",
                "entitlement_type",
                "arbiter_instance",
                "arbiter_actor",
                "arbitration_policy_id",
                "expires_at",
            ] {
                required_str(body, field)?;
            }
            required_u64(body, "offer_revision")?;
            required_u64(body, "quantity")?;
            required_object(body, "price")?;
        }
        "io.marketplace.actor.customer.bound" => {
            required_str(body, "customer_id")?;
            required_str(body, "status")?;
        }
        event if event.starts_with("io.marketplace.payment.") => {
            required_str(body, "order_id")?;
            required_str(body, "payment_id")?;
        }
        event if event.starts_with("io.marketplace.entitlement.") => {
            required_str(body, "order_id")?;
            required_str(body, "entitlement_id")?;
        }
        event if event.starts_with("io.marketplace.dispute.") => {
            required_str(body, "order_id")?;
            required_str(body, "dispute_id")?;
        }
        _ => {}
    }
    Ok(())
}

fn required_str<'a>(body: &'a Value, field: &str) -> ValidationResult<&'a str> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ValidationError::with_details(
                ValidationCode::MissingRequiredField,
                format!("Missing required string field {field}"),
                json!({ "field": field }),
            )
        })
}

fn required_u64(body: &Value, field: &str) -> ValidationResult<u64> {
    body.get(field).and_then(Value::as_u64).ok_or_else(|| {
        ValidationError::with_details(
            ValidationCode::MissingRequiredField,
            format!("Missing required integer field {field}"),
            json!({ "field": field }),
        )
    })
}

fn required_object<'a>(
    body: &'a Value,
    field: &str,
) -> ValidationResult<&'a serde_json::Map<String, Value>> {
    body.get(field).and_then(Value::as_object).ok_or_else(|| {
        ValidationError::with_details(
            ValidationCode::MissingRequiredField,
            format!("Missing required object field {field}"),
            json!({ "field": field }),
        )
    })
}

fn requires_actor_issuer(event_type: &str) -> bool {
    !matches!(
        event_type,
        "io.marketplace.instance.profile"
            | "io.marketplace.catalog.profile"
            | "io.marketplace.catalog.snapshot.published"
    )
}

fn is_matrix_user_id(value: &str) -> bool {
    value.starts_with('@') && value.contains(':')
}

pub mod fixtures {
    use serde_json::{Value, json};

    pub fn valid_order_created_event() -> Value {
        json!({
            "type": "io.marketplace.order.created",
            "room_id": "!order:customer.example",
            "event_id": "$matrix-order-created",
            "sender": "@market:customer.example",
            "origin_server_ts": 1_777_888_000_000i64,
            "content": {
                "protocol": "io.marketplace",
                "protocol_version": "0.1",
                "event_id": "evt:customer.example:01JORDER",
                "created_at": "2026-05-04T10:00:00Z",
                "issuer": {
                    "instance_id": "customer.example",
                    "actor_id": "customer:customer.example:01JCUST",
                    "matrix_user_id": "@market:customer.example"
                },
                "critical": [],
                "body": {
                    "order_id": "ord:customer.example:01JORDER",
                    "room_id": "!order:customer.example",
                    "customer_id": "customer:customer.example:01JCUST",
                    "seller_id": "seller:shop.example:01JSELLER",
                    "offer_id": "offer:shop.example:01JOFFER",
                    "offer_revision": 3,
                    "catalog_snapshot_id": "snap_01J",
                    "quantity": 1,
                    "price": { "amount": "100.00", "currency": "USD" },
                    "payment_adapter": "mock",
                    "entitlement_type": "booking_slot",
                    "arbiter_instance": "arbiter.example",
                    "arbiter_actor": "arbiter:arbiter.example:default",
                    "arbitration_policy_id": "standard-digital-v1",
                    "arbitration_window": "P14D",
                    "expires_at": "2026-05-04T10:30:00Z"
                }
            }
        })
    }
}
