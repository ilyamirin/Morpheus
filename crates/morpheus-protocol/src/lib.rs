use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use thiserror::Error;

pub const PROTOCOL_NAME: &str = "io.marketplace";
pub const PROTOCOL_VERSION: &str = "0.1";

pub const PRODUCT_KINDS: &[&str] = &[
    "digital_file",
    "license",
    "account_access",
    "digital_service",
    "booking",
    "subscription",
    "external_entitlement",
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

pub const DISPUTE_RULINGS: &[&str] = &[
    "refund_required",
    "partial_refund_required",
    "entitlement_confirmed",
    "entitlement_reissue_required",
    "service_completion_required",
    "no_fault",
];

const CAPTURE_POLICIES: &[&str] = &["before_entitlement", "after_entitlement"];

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

const OBJECT_ID_KINDS: &[&str] = &[
    "prod", "offer", "ord", "pay", "refund", "ent", "disp", "seller", "customer", "arbiter", "evt",
    "snap",
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceEnvelope {
    pub protocol: String,
    pub protocol_version: String,
    pub protocol_event_id: String,
    pub created_at: String,
    pub issuer: Issuer,
    #[serde(default)]
    pub critical: Vec<String>,
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixMarketplaceEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub room_id: String,
    pub event_id: String,
    pub sender: String,
    pub origin_server_ts: i64,
    #[serde(default)]
    pub unsigned: Option<Value>,
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
    UnsupportedProtocolVersion,
    RoomProfileViolation,
    UnauthorizedSender,
    InstanceNotAllowlisted,
    ActorNotActive,
    RevisionRollback,
    MissingRequiredField,
    UnknownCriticalExtension,
    InvalidStateTransition,
    InvalidId,
    CatalogReferenceMismatch,
    PaymentTermsMismatch,
    HashMismatch,
    RedactedEvent,
    RoomMembershipViolation,
    PrivacyViolation,
    PolicyViolation,
    DuplicateEvent,
    UnknownEventType,
}

impl ValidationCode {
    pub fn disposition(self) -> ValidationDisposition {
        match self {
            Self::RoomProfileViolation | Self::MissingRequiredField => {
                ValidationDisposition::Retryable
            }
            _ => ValidationDisposition::Terminal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationDisposition {
    Retryable,
    Terminal,
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

    pub fn disposition(&self) -> ValidationDisposition {
        self.code.disposition()
    }
}

pub type ValidationResult<T> = Result<T, ValidationError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedActorId {
    pub kind: String,
    pub instance_id: String,
    pub local_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomProfile {
    Catalog,
    Order,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketplaceEventValidationResult {
    Accepted(Box<MatrixMarketplaceEvent>),
    IgnoredUnknownEventType,
}

#[derive(Debug, Default)]
pub struct MarketplaceEventValidationContext {
    pub room_profile: Option<RoomProfile>,
    pub supported_critical: HashSet<String>,
    pub seen_protocol_events: HashMap<String, SeenProtocolEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenProtocolEvent {
    pub matrix_event_id: String,
    pub body_hash: String,
}

pub fn validation_disposition(code: ValidationCode) -> ValidationDisposition {
    code.disposition()
}

pub fn validate_event_envelope(raw_event: &Value) -> ValidationResult<ValidatedMarketplaceEvent> {
    match validate_marketplace_event(raw_event, &mut MarketplaceEventValidationContext::default())?
    {
        MarketplaceEventValidationResult::Accepted(event) => Ok(ValidatedMarketplaceEvent {
            event_type: event.event_type,
            room_id: event.room_id,
            matrix_event_id: event.event_id,
            marketplace_event_id: event.content.protocol_event_id,
            sender: event.sender,
            issuer: event.content.issuer,
            body: event.content.body,
        }),
        MarketplaceEventValidationResult::IgnoredUnknownEventType => Err(ValidationError::new(
            ValidationCode::UnknownEventType,
            "Unknown marketplace event type",
        )),
    }
}

pub fn validate_marketplace_event(
    raw_event: &Value,
    context: &mut MarketplaceEventValidationContext,
) -> ValidationResult<MarketplaceEventValidationResult> {
    let event: MatrixMarketplaceEvent =
        serde_json::from_value(raw_event.clone()).map_err(|err| {
            ValidationError::with_details(
                ValidationCode::MissingRequiredField,
                "Invalid Matrix marketplace event",
                json!({ "error": err.to_string() }),
            )
        })?;

    validate_generic_event_shape(&event)?;
    if event
        .unsigned
        .as_ref()
        .and_then(|unsigned| unsigned.get("redacted_because"))
        .is_some()
    {
        return Err(ValidationError::with_details(
            ValidationCode::RedactedEvent,
            "Redacted marketplace events are not protocol-valid",
            json!({ "eventId": event.event_id }),
        ));
    }
    if event.sender != event.content.issuer.matrix_user_id {
        return Err(ValidationError::with_details(
            ValidationCode::UnauthorizedSender,
            "Matrix sender must match issuer matrix_user_id",
            json!({ "sender": event.sender, "issuer": event.content.issuer.matrix_user_id }),
        ));
    }

    if !is_known_event_type(&event.event_type) {
        if !event.content.critical.is_empty() {
            return Err(ValidationError::with_details(
                ValidationCode::UnknownCriticalExtension,
                "Unknown event type has critical extensions",
                json!({ "eventType": event.event_type, "critical": event.content.critical }),
            ));
        }
        return Ok(MarketplaceEventValidationResult::IgnoredUnknownEventType);
    }

    assert_supported_critical(&event.content.critical, &context.supported_critical)?;
    if let Some(room_profile) = context.room_profile {
        assert_event_allowed_in_room(room_profile, &event.event_type)?;
    }
    validate_body_shape(&event.event_type, &event.content.body)?;
    if event.event_type == "io.marketplace.order.created"
        && event
            .content
            .body
            .get("room_id")
            .and_then(Value::as_str)
            .is_some_and(|room_id| room_id != event.room_id)
    {
        return Err(ValidationError::with_details(
            ValidationCode::CatalogReferenceMismatch,
            "Order room mismatch between Matrix room_id and content.body.room_id",
            json!({ "eventRoomId": event.room_id, "bodyRoomId": event.content.body.get("room_id") }),
        ));
    }
    assert_protocol_event_not_replayed(&event, &mut context.seen_protocol_events)?;

    Ok(MarketplaceEventValidationResult::Accepted(Box::new(event)))
}

pub fn is_known_event_type(event_type: &str) -> bool {
    is_catalog_event_type(event_type) || is_order_event_type(event_type)
}

pub fn is_catalog_event_type(event_type: &str) -> bool {
    CATALOG_EVENT_TYPES.contains(&event_type)
}

pub fn is_order_event_type(event_type: &str) -> bool {
    ORDER_EVENT_TYPES.contains(&event_type)
}

pub fn assert_event_allowed_in_room(
    room_profile: RoomProfile,
    event_type: &str,
) -> ValidationResult<()> {
    let allowed = match room_profile {
        RoomProfile::Catalog => is_catalog_event_type(event_type),
        RoomProfile::Order => is_order_event_type(event_type),
    };
    if allowed {
        Ok(())
    } else {
        Err(ValidationError::with_details(
            ValidationCode::RoomProfileViolation,
            "Event type is not allowed in this room profile",
            json!({ "roomProfile": format!("{room_profile:?}"), "eventType": event_type }),
        ))
    }
}

pub fn parse_actor_id(actor_id: &str) -> ValidationResult<ParsedActorId> {
    let parts = actor_id.split(':').collect::<Vec<_>>();
    if parts.len() != 3
        || !matches!(parts[0], "seller" | "customer" | "arbiter")
        || !is_valid_instance_id(parts[1])
        || !is_valid_local_id(parts[2])
    {
        return Err(invalid_id("actor", actor_id));
    }
    Ok(ParsedActorId {
        kind: parts[0].to_string(),
        instance_id: parts[1].to_string(),
        local_id: parts[2].to_string(),
    })
}

pub fn parse_object_instance(id: &str) -> ValidationResult<&str> {
    let parts = id.split(':').collect::<Vec<_>>();
    if parts.len() != 3
        || !is_object_id_kind(parts[0])
        || !is_valid_instance_id(parts[1])
        || !is_valid_local_id(parts[2])
    {
        return Err(invalid_id("object", id));
    }
    Ok(parts[1])
}

pub fn is_protocol_object_id(id: &str, kind: Option<&str>) -> bool {
    let parts = id.split(':').collect::<Vec<_>>();
    parts.len() == 3
        && is_object_id_kind(parts[0])
        && kind.is_none_or(|expected| parts[0] == expected)
        && is_valid_instance_id(parts[1])
        && is_valid_local_id(parts[2])
}

pub fn is_valid_instance_id(instance_id: &str) -> bool {
    let labels = instance_id.split('.').collect::<Vec<_>>();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
                && label
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
                && label
                    .chars()
                    .last()
                    .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        })
}

pub fn is_valid_local_id(local_id: &str) -> bool {
    (3..=64).contains(&local_id.len())
        && local_id
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        && local_id
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

pub fn canonical_json(value: &Value) -> ValidationResult<String> {
    Ok(serde_json::to_string(&sort_canonical(value))
        .expect("serializing serde_json::Value cannot fail"))
}

pub fn sha256_canonical(value: &Value) -> ValidationResult<String> {
    let encoded = canonical_json(value)?;
    let digest = Sha256::digest(encoded.as_bytes());
    Ok(format!("sha256:{}", hex::encode(digest)))
}

pub fn canonical_json_sha256(value: &Value) -> ValidationResult<String> {
    sha256_canonical(value)
}

pub fn assert_sha256_matches(value: &Value, expected: &str) -> ValidationResult<()> {
    if !is_sha256_hash(expected) {
        return Err(ValidationError::with_details(
            ValidationCode::HashMismatch,
            "Expected hash must use sha256:<64 hex> format",
            json!({ "expected": expected }),
        ));
    }
    let actual = sha256_canonical(value)?;
    if actual == expected {
        Ok(())
    } else {
        Err(ValidationError::with_details(
            ValidationCode::HashMismatch,
            "Canonical hash mismatch",
            json!({ "expected": expected, "actual": actual }),
        ))
    }
}

pub fn validate_marketplace_privacy(event_type: &str, body: &Value) -> ValidationResult<()> {
    let text = body.to_string().to_ascii_lowercase();
    if is_catalog_event_type(event_type)
        && [
            "customer_id",
            "order_id",
            "payment_id",
            "entitlement_id",
            "dispute_id",
        ]
        .iter()
        .any(|field| body.get(*field).is_some() || text.contains(*field))
    {
        return Err(ValidationError::new(
            ValidationCode::PrivacyViolation,
            "Catalog events must not contain order or customer data",
        ));
    }
    if is_order_event_type(event_type)
        && ["bearer ", "access_token=", "token=", "secret=", "password="]
            .iter()
            .any(|needle| text.contains(needle))
    {
        return Err(ValidationError::new(
            ValidationCode::PrivacyViolation,
            "Order events must not contain bearer secrets or private credentials",
        ));
    }
    Ok(())
}

pub fn validate_extension_name(name: &str) -> ValidationResult<()> {
    if name.starts_with("io.marketplace.") {
        Err(ValidationError::new(
            ValidationCode::PolicyViolation,
            "Extension names outside the standard protocol must not use io.marketplace.*",
        ))
    } else if name.contains('.') {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::PolicyViolation,
            "Extension names must use reverse-DNS namespaces",
        ))
    }
}

pub fn validate_min_consumer_version(
    min_consumer_version: &str,
    supported_version: &str,
) -> ValidationResult<()> {
    if min_consumer_version <= supported_version {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::UnsupportedProtocolVersion,
            "Protocol downgrade or unsupported minimum consumer version",
        ))
    }
}

pub fn validate_sender_issuer_server(sender: &str, issuer_instance: &str) -> ValidationResult<()> {
    let sender_server = sender.split(':').nth(1).unwrap_or_default();
    if sender_server == issuer_instance {
        Ok(())
    } else {
        Err(ValidationError::with_details(
            ValidationCode::UnauthorizedSender,
            "Matrix sender server must match issuer instance",
            json!({ "sender": sender, "issuerInstance": issuer_instance }),
        ))
    }
}

pub fn validate_retention_policy(
    retain_catalog_tombstones: bool,
    retain_completed_entitlements: bool,
) -> ValidationResult<()> {
    if retain_catalog_tombstones && retain_completed_entitlements {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::PolicyViolation,
            "Retention policy must keep catalog tombstones and completed entitlement records",
        ))
    }
}

pub fn validate_backfill_page_event_ids(event_ids: &[String]) -> ValidationResult<()> {
    let mut seen = HashSet::new();
    for event_id in event_ids {
        if !seen.insert(event_id) {
            return Err(ValidationError::with_details(
                ValidationCode::DuplicateEvent,
                "Backfill page contains duplicate Matrix event ids",
                json!({ "eventId": event_id }),
            ));
        }
    }
    Ok(())
}

pub fn validate_snapshot_cache_entry(
    previous_hash: Option<&str>,
    actual_hash: &str,
) -> ValidationResult<()> {
    if previous_hash.is_some_and(|previous| previous != actual_hash) {
        Err(ValidationError::new(
            ValidationCode::HashMismatch,
            "Snapshot cache entry changed hash for the same sequence",
        ))
    } else {
        Ok(())
    }
}

pub fn validate_appservice_sender_namespace(
    sender: &str,
    server_name: &str,
    localpart_prefix: &str,
) -> ValidationResult<()> {
    let expected_suffix = format!(":{server_name}");
    let expected_prefix = format!("@{localpart_prefix}");
    if sender.starts_with(&expected_prefix) && sender.ends_with(&expected_suffix) {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::UnauthorizedSender,
            "Marketplace AS sender is outside configured namespace",
        ))
    }
}

pub fn validate_appservice_transaction(
    previous_event_ids: Option<&[String]>,
    event_ids: &[String],
) -> ValidationResult<()> {
    if previous_event_ids.is_some_and(|previous| previous != event_ids) {
        Err(ValidationError::new(
            ValidationCode::DuplicateEvent,
            "AppService transactions must be idempotent",
        ))
    } else {
        Ok(())
    }
}

fn validate_generic_event_shape(event: &MatrixMarketplaceEvent) -> ValidationResult<()> {
    if !event.room_id.starts_with('!')
        || !event.event_id.starts_with('$')
        || !is_matrix_user_id(&event.sender)
        || event.origin_server_ts < 0
    {
        return Err(ValidationError::new(
            ValidationCode::MissingRequiredField,
            "Matrix event fields have invalid shape",
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
    if !is_protocol_object_id(&event.content.protocol_event_id, Some("evt")) {
        return Err(ValidationError::new(
            ValidationCode::InvalidId,
            "Invalid protocol_event_id",
        ));
    }
    if !is_valid_instance_id(&event.content.issuer.instance_id) {
        return Err(ValidationError::new(
            ValidationCode::InvalidId,
            "Invalid issuer.instance_id",
        ));
    }
    if let Some(actor_id) = &event.content.issuer.actor_id {
        parse_actor_id(actor_id)?;
    }
    if requires_actor_issuer(&event.event_type) && event.content.issuer.actor_id.is_none() {
        return Err(ValidationError::new(
            ValidationCode::MissingRequiredField,
            format!("{} requires issuer.actor_id", event.event_type),
        ));
    }
    if !is_matrix_user_id(&event.content.issuer.matrix_user_id) {
        return Err(ValidationError::new(
            ValidationCode::MissingRequiredField,
            "Invalid issuer.matrix_user_id",
        ));
    }
    let _ = DateTime::parse_from_rfc3339(&event.content.created_at)
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
    Ok(())
}

fn assert_supported_critical(
    critical: &[String],
    supported: &HashSet<String>,
) -> ValidationResult<()> {
    let unsupported = critical
        .iter()
        .filter(|extension| !supported.contains(*extension))
        .cloned()
        .collect::<Vec<_>>();
    if unsupported.is_empty() {
        Ok(())
    } else {
        Err(ValidationError::with_details(
            ValidationCode::UnknownCriticalExtension,
            "Unsupported critical extension",
            json!({ "unsupported": unsupported }),
        ))
    }
}

fn assert_protocol_event_not_replayed(
    event: &MatrixMarketplaceEvent,
    seen: &mut HashMap<String, SeenProtocolEvent>,
) -> ValidationResult<()> {
    let protocol_event_id = &event.content.protocol_event_id;
    let body_hash = sha256_canonical(&event.content.body)?;
    match seen.get(protocol_event_id) {
        None => {
            seen.insert(
                protocol_event_id.clone(),
                SeenProtocolEvent {
                    matrix_event_id: event.event_id.clone(),
                    body_hash,
                },
            );
            Ok(())
        }
        Some(previous)
            if previous.matrix_event_id == event.event_id && previous.body_hash == body_hash =>
        {
            Ok(())
        }
        Some(previous) => Err(ValidationError::with_details(
            ValidationCode::DuplicateEvent,
            "protocol_event_id replay with different Matrix event or body hash",
            json!({ "protocolEventId": protocol_event_id, "previous": { "matrixEventId": previous.matrix_event_id, "bodyHash": previous.body_hash }, "actual": { "matrixEventId": event.event_id, "bodyHash": body_hash } }),
        )),
    }
}

fn validate_body_shape(event_type: &str, body: &Value) -> ValidationResult<()> {
    match event_type {
        "io.marketplace.instance.profile" => {
            required_instance_id(body, "instance_id")?;
            required_instance_id(body, "matrix_server_name")?;
            required_str(body, "application_service_id")?;
            required_room_id(body, "catalog_room_id")?;
            required_string_array(body, "protocol_versions")?;
            required_string_array(body, "payment_adapters")?;
            required_enum_array(body, "entitlement_types", ENTITLEMENT_TYPES)?;
            required_string_array(body, "arbitration_policies")?;
        }
        "io.marketplace.catalog.profile" => {
            required_instance_id(body, "instance_id")?;
            required_bool(body, "snapshot_required")?;
            required_bool(body, "delta_required")?;
        }
        "io.marketplace.actor.seller.announced" => {
            required_object_id(body, "seller_id", "seller")?;
            required_enum(body, "status", &["active", "suspended"])?;
            required_str(body, "display_name")?;
            required_url(body, "legal_profile_ref")?;
            required_url(body, "terms_ref")?;
            required_sha256(body, "terms_hash")?;
            required_string_array(body, "supported_payment_adapters")?;
            required_enum_array(body, "supported_entitlement_types", ENTITLEMENT_TYPES)?;
        }
        "io.marketplace.actor.seller.suspended" => {
            required_object_id(body, "seller_id", "seller")?;
            required_literal(body, "status", "suspended")?;
        }
        "io.marketplace.actor.customer.bound" => {
            required_object_id(body, "customer_id", "customer")?;
            required_enum(body, "status", &["active", "suspended"])?;
            required_str(body, "display_name")?;
            required_instance_id(body, "instance_id")?;
            required_matrix_user_array(body, "authorized_representatives")?;
            required_string_array(body, "accepted_payment_adapters")?;
            required_string_array(body, "accepted_arbitration_policies")?;
        }
        "io.marketplace.catalog.snapshot.published" => {
            required_object_id(body, "snapshot_id", "snap")?;
            required_u64(body, "sequence")?;
            required_literal(body, "format", "application/json+io.marketplace.catalog.v0")?;
            required_str(body, "uri")?;
            required_sha256(body, "sha256")?;
            required_event_id(body, "covers_events_until")?;
            required_u64(body, "product_count")?;
            required_u64(body, "offer_count")?;
            required_utc(body, "created_at")?;
        }
        "io.marketplace.product.upserted" => {
            required_object_id(body, "product_id", "prod")?;
            required_object_id(body, "seller_id", "seller")?;
            required_positive_u64(body, "revision")?;
            required_enum(body, "status", &["active", "withdrawn"])?;
            required_enum(body, "kind", PRODUCT_KINDS)?;
            required_str(body, "title")?;
            required_str(body, "description")?;
            required_string_array(body, "categories")?;
            required_string_array(body, "tags")?;
            required_array(body, "media")?;
            required_sha256(body, "terms_hash")?;
        }
        "io.marketplace.product.withdrawn" => {
            required_object_id(body, "product_id", "prod")?;
            required_positive_u64(body, "revision")?;
        }
        "io.marketplace.offer.upserted" => {
            required_object_id(body, "offer_id", "offer")?;
            required_object_id(body, "product_id", "prod")?;
            required_object_id(body, "seller_id", "seller")?;
            required_positive_u64(body, "revision")?;
            required_enum(body, "status", &["active", "withdrawn"])?;
            required_money(body, "price")?;
            let payment_terms = required_object(body, "payment_terms")?;
            required_enum_map(payment_terms, "capture_policy", CAPTURE_POLICIES)?;
            required_enum_map(payment_terms, "adapter_policy", &["seller_supported"])?;
            let entitlement = required_object(body, "entitlement")?;
            required_enum_map(entitlement, "type", ENTITLEMENT_TYPES)?;
            required_enum_map(entitlement, "delivery", &["external"])?;
            let availability = required_object(body, "availability")?;
            required_enum_map(availability, "mode", &["unlimited", "limited"])?;
            required_sha256(body, "seller_terms_hash")?;
            required_sha256(body, "offer_terms_hash")?;
        }
        "io.marketplace.offer.withdrawn" => {
            required_object_id(body, "offer_id", "offer")?;
            required_positive_u64(body, "revision")?;
        }
        "io.marketplace.inventory.updated" => {
            required_object_id(body, "offer_id", "offer")?;
            required_positive_u64(body, "revision")?;
            required_u64(body, "available_quantity")?;
        }
        "io.marketplace.order.created" => {
            required_object_id(body, "order_id", "ord")?;
            required_room_id(body, "room_id")?;
            required_object_id(body, "customer_id", "customer")?;
            required_object_id(body, "seller_id", "seller")?;
            required_object_id(body, "offer_id", "offer")?;
            required_positive_u64(body, "offer_revision")?;
            required_object_id(body, "catalog_snapshot_id", "snap")?;
            let quantity = required_positive_u64(body, "quantity")?;
            if quantity != 1 {
                return Err(ValidationError::new(
                    ValidationCode::PaymentTermsMismatch,
                    "Order quantity is limited to one in v0.1",
                ));
            }
            required_money(body, "price")?;
            required_str(body, "payment_adapter")?;
            required_enum(body, "payment_capture_policy", CAPTURE_POLICIES)?;
            required_enum(body, "entitlement_type", ENTITLEMENT_TYPES)?;
            required_instance_id(body, "arbiter_instance")?;
            required_object_id(body, "arbiter_actor", "arbiter")?;
            required_sha256(body, "seller_terms_hash")?;
            required_sha256(body, "offer_terms_hash")?;
            required_str(body, "arbitration_policy_id")?;
            required_str(body, "arbitration_policy_version")?;
            required_str(body, "arbitration_window")?;
            required_utc(body, "expires_at")?;
        }
        "io.marketplace.order.accepted" => {
            required_object_id(body, "order_id", "ord")?;
            required_positive_u64(body, "offer_revision")?;
            required_sha256(body, "seller_terms_hash")?;
            required_sha256(body, "offer_terms_hash")?;
            required_enum(body, "payment_capture_policy", CAPTURE_POLICIES)?;
            required_str(body, "arbitration_policy_version")?;
        }
        "io.marketplace.order.cancelled"
        | "io.marketplace.order.rejected"
        | "io.marketplace.order.completed" => {
            required_object_id(body, "order_id", "ord")?;
        }
        "io.marketplace.payment.intent.created" => {
            required_object_id(body, "order_id", "ord")?;
            required_object_id(body, "payment_id", "pay")?;
            required_str(body, "adapter")?;
            required_amount(body, "amount")?;
            required_currency(body, "currency")?;
            required_enum(body, "capture_policy", CAPTURE_POLICIES)?;
            required_str(body, "idempotency_key")?;
            required_str(body, "provider_ref")?;
            let confirmation = required_object(body, "confirmation")?;
            required_str_map(confirmation, "method")?;
            required_url_map(confirmation, "uri")?;
            required_utc(body, "expires_at")?;
        }
        "io.marketplace.payment.authorized"
        | "io.marketplace.payment.failed"
        | "io.marketplace.payment.cancelled"
        | "io.marketplace.payment.chargeback.opened" => {
            required_object_id(body, "order_id", "ord")?;
            required_object_id(body, "payment_id", "pay")?;
        }
        "io.marketplace.payment.captured" => {
            required_object_id(body, "order_id", "ord")?;
            required_object_id(body, "payment_id", "pay")?;
            required_str(body, "adapter")?;
            required_amount(body, "amount")?;
            required_currency(body, "currency")?;
            required_str(body, "provider_ref")?;
            required_evidence(body)?;
        }
        "io.marketplace.payment.refund.requested" | "io.marketplace.payment.refunded" => {
            required_object_id(body, "order_id", "ord")?;
            required_object_id(body, "payment_id", "pay")?;
            required_object_id(body, "refund_id", "refund")?;
            required_amount(body, "amount")?;
            required_currency(body, "currency")?;
            required_str(body, "provider_ref")?;
            required_evidence(body)?;
        }
        "io.marketplace.entitlement.granted" => {
            required_object_id(body, "order_id", "ord")?;
            if body.get("payment_id").is_some() {
                required_object_id(body, "payment_id", "pay")?;
            }
            required_object_id(body, "entitlement_id", "ent")?;
            let entitlement_type = required_enum(body, "type", ENTITLEMENT_TYPES)?;
            required_str(body, "external_ref")?;
            if matches!(entitlement_type, "booking_slot" | "subscription_access") {
                required_utc(body, "valid_from")?;
                required_utc(body, "valid_until")?;
            }
            if matches!(
                entitlement_type,
                "service_delivery" | "external_entitlement"
            ) {
                required_evidence(body)?;
            }
            if body.get("evidence").is_some() {
                return required_evidence(body);
            }
        }
        "io.marketplace.entitlement.activated"
        | "io.marketplace.entitlement.completed"
        | "io.marketplace.entitlement.revoked"
        | "io.marketplace.entitlement.expired" => {
            required_object_id(body, "order_id", "ord")?;
            required_object_id(body, "entitlement_id", "ent")?;
        }
        "io.marketplace.dispute.opened" | "io.marketplace.dispute.closed" => {
            required_object_id(body, "order_id", "ord")?;
            required_object_id(body, "dispute_id", "disp")?;
        }
        "io.marketplace.dispute.evidence.submitted" => {
            required_object_id(body, "order_id", "ord")?;
            required_object_id(body, "dispute_id", "disp")?;
            required_evidence(body)?;
        }
        "io.marketplace.dispute.ruling.issued" => {
            required_object_id(body, "order_id", "ord")?;
            required_object_id(body, "dispute_id", "disp")?;
            required_enum(body, "ruling", DISPUTE_RULINGS)?;
            required_str(body, "reason_code")?;
            required_object(body, "remedy")?;
            required_array(body, "evidence_refs")?;
            required_bool(body, "binding")?;
        }
        _ => {}
    }
    Ok(())
}

fn sort_canonical(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(sort_canonical).collect()),
        Value::Object(map) => {
            let sorted = map
                .iter()
                .map(|(key, value)| (key.clone(), sort_canonical(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(Map::from_iter(sorted))
        }
        _ => value.clone(),
    }
}

fn is_object_id_kind(kind: &str) -> bool {
    OBJECT_ID_KINDS.contains(&kind)
}

fn invalid_id(id_type: &str, id: &str) -> ValidationError {
    ValidationError::with_details(
        ValidationCode::InvalidId,
        format!("Invalid {id_type} id: {id}"),
        json!({ "idType": id_type, "id": id }),
    )
}

fn is_matrix_user_id(value: &str) -> bool {
    value.starts_with('@') && value.split(':').count() == 2
}

fn requires_actor_issuer(event_type: &str) -> bool {
    !matches!(
        event_type,
        "io.marketplace.instance.profile"
            | "io.marketplace.catalog.profile"
            | "io.marketplace.catalog.snapshot.published"
    )
}

fn required_str<'a>(body: &'a Value, field: &str) -> ValidationResult<&'a str> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing(field, "string"))
}

fn required_str_map<'a>(body: &'a Map<String, Value>, field: &str) -> ValidationResult<&'a str> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing(field, "string"))
}

fn required_u64(body: &Value, field: &str) -> ValidationResult<u64> {
    body.get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| missing(field, "integer"))
}

fn required_positive_u64(body: &Value, field: &str) -> ValidationResult<u64> {
    let value = required_u64(body, field)?;
    if value > 0 {
        Ok(value)
    } else {
        Err(missing(field, "positive integer"))
    }
}

fn required_bool(body: &Value, field: &str) -> ValidationResult<bool> {
    body.get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| missing(field, "boolean"))
}

fn required_array<'a>(body: &'a Value, field: &str) -> ValidationResult<&'a Vec<Value>> {
    body.get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| missing(field, "array"))
}

fn required_object<'a>(body: &'a Value, field: &str) -> ValidationResult<&'a Map<String, Value>> {
    body.get(field)
        .and_then(Value::as_object)
        .ok_or_else(|| missing(field, "object"))
}

fn required_literal(body: &Value, field: &str, expected: &str) -> ValidationResult<()> {
    if required_str(body, field)? == expected {
        Ok(())
    } else {
        Err(missing(field, expected))
    }
}

fn required_enum<'a>(body: &'a Value, field: &str, allowed: &[&str]) -> ValidationResult<&'a str> {
    let value = required_str(body, field)?;
    if allowed.contains(&value) {
        Ok(value)
    } else {
        Err(missing(field, "enum"))
    }
}

fn required_enum_map<'a>(
    body: &'a Map<String, Value>,
    field: &str,
    allowed: &[&str],
) -> ValidationResult<&'a str> {
    let value = required_str_map(body, field)?;
    if allowed.contains(&value) {
        Ok(value)
    } else {
        Err(missing(field, "enum"))
    }
}

fn required_string_array(body: &Value, field: &str) -> ValidationResult<()> {
    let values = required_array(body, field)?;
    if values
        .iter()
        .all(|value| value.as_str().is_some_and(|text| !text.is_empty()))
    {
        Ok(())
    } else {
        Err(missing(field, "string array"))
    }
}

fn required_matrix_user_array(body: &Value, field: &str) -> ValidationResult<()> {
    let values = required_array(body, field)?;
    if values
        .iter()
        .all(|value| value.as_str().is_some_and(is_matrix_user_id))
    {
        Ok(())
    } else {
        Err(missing(field, "Matrix user array"))
    }
}

fn required_enum_array(body: &Value, field: &str, allowed: &[&str]) -> ValidationResult<()> {
    let values = required_array(body, field)?;
    if values
        .iter()
        .all(|value| value.as_str().is_some_and(|text| allowed.contains(&text)))
    {
        Ok(())
    } else {
        Err(missing(field, "enum array"))
    }
}

fn required_instance_id(body: &Value, field: &str) -> ValidationResult<()> {
    let value = required_str(body, field)?;
    if is_valid_instance_id(value) {
        Ok(())
    } else {
        Err(invalid_id("instance", value))
    }
}

fn required_object_id(body: &Value, field: &str, kind: &str) -> ValidationResult<()> {
    let value = required_str(body, field)?;
    if is_protocol_object_id(value, Some(kind)) {
        Ok(())
    } else {
        Err(invalid_id("object", value))
    }
}

fn required_room_id(body: &Value, field: &str) -> ValidationResult<()> {
    if required_str(body, field)?.starts_with('!') {
        Ok(())
    } else {
        Err(missing(field, "room id"))
    }
}

fn required_event_id(body: &Value, field: &str) -> ValidationResult<()> {
    if required_str(body, field)?.starts_with('$') {
        Ok(())
    } else {
        Err(missing(field, "event id"))
    }
}

fn required_url(body: &Value, field: &str) -> ValidationResult<()> {
    let value = required_str(body, field)?;
    if value.starts_with("https://") || value.starts_with("http://") {
        Ok(())
    } else {
        Err(missing(field, "url"))
    }
}

fn required_url_map(body: &Map<String, Value>, field: &str) -> ValidationResult<()> {
    let value = required_str_map(body, field)?;
    if value.starts_with("https://") || value.starts_with("http://") {
        Ok(())
    } else {
        Err(missing(field, "url"))
    }
}

fn required_utc(body: &Value, field: &str) -> ValidationResult<()> {
    let value = required_str(body, field)?;
    let _ = DateTime::parse_from_rfc3339(value)
        .map_err(|_| missing(field, "UTC timestamp"))?
        .with_timezone(&Utc);
    if value.ends_with('Z') {
        Ok(())
    } else {
        Err(missing(field, "UTC timestamp"))
    }
}

fn required_money(body: &Value, field: &str) -> ValidationResult<()> {
    let money = required_object(body, field)?;
    required_amount_map(money, "amount")?;
    required_currency_map(money, "currency")
}

fn required_amount(body: &Value, field: &str) -> ValidationResult<()> {
    let value = required_str(body, field)?;
    if is_money_amount(value) {
        Ok(())
    } else {
        Err(missing(field, "money amount"))
    }
}

fn required_amount_map(body: &Map<String, Value>, field: &str) -> ValidationResult<()> {
    let value = required_str_map(body, field)?;
    if is_money_amount(value) {
        Ok(())
    } else {
        Err(missing(field, "money amount"))
    }
}

fn required_currency(body: &Value, field: &str) -> ValidationResult<()> {
    let value = required_str(body, field)?;
    if is_currency(value) {
        Ok(())
    } else {
        Err(missing(field, "currency"))
    }
}

fn required_currency_map(body: &Map<String, Value>, field: &str) -> ValidationResult<()> {
    let value = required_str_map(body, field)?;
    if is_currency(value) {
        Ok(())
    } else {
        Err(missing(field, "currency"))
    }
}

fn required_sha256(body: &Value, field: &str) -> ValidationResult<()> {
    let value = required_str(body, field)?;
    if is_sha256_hash(value) {
        Ok(())
    } else {
        Err(missing(field, "sha256 hash"))
    }
}

fn required_evidence(body: &Value) -> ValidationResult<()> {
    let evidence = required_object(body, "evidence")?;
    required_str_map(evidence, "kind")?;
    let uri = required_str_map(evidence, "uri")?;
    if !(uri.starts_with("mxc://") || uri.starts_with("https://") || uri.starts_with("http://")) {
        return Err(missing("evidence.uri", "uri"));
    }
    let sha256 = required_str_map(evidence, "sha256")?;
    if is_sha256_hash(sha256) {
        Ok(())
    } else {
        Err(missing("evidence.sha256", "sha256 hash"))
    }
}

fn is_money_amount(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    !parts.is_empty()
        && parts.len() <= 2
        && !parts[0].is_empty()
        && parts[0].chars().all(|ch| ch.is_ascii_digit())
        && parts.get(1).is_none_or(|fraction| {
            !fraction.is_empty()
                && fraction.len() <= 8
                && fraction.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn is_currency(value: &str) -> bool {
    value.len() == 3 && value.chars().all(|ch| ch.is_ascii_uppercase())
}

fn is_sha256_hash(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
}

fn missing(field: &str, expected: &str) -> ValidationError {
    ValidationError::with_details(
        ValidationCode::MissingRequiredField,
        format!("Missing or invalid {expected} field {field}"),
        json!({ "field": field }),
    )
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
                "protocol_event_id": "evt:customer.example:01JORDER",
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
                    "catalog_snapshot_id": "snap:shop.example:01JSNAP",
                    "quantity": 1,
                    "price": { "amount": "100.00", "currency": "USD" },
                    "payment_adapter": "mock",
                    "payment_capture_policy": "before_entitlement",
                    "entitlement_type": "booking_slot",
                    "arbiter_instance": "arbiter.example",
                    "arbiter_actor": "arbiter:arbiter.example:01JARB",
                    "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                    "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                    "arbitration_policy_id": "standard-digital-v1",
                    "arbitration_policy_version": "1",
                    "arbitration_window": "P14D",
                    "expires_at": "2026-05-04T10:30:00Z"
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_schema_helpers_cover_rejection_edges() {
        validate_body_shape("io.marketplace.unknown", &json!({})).unwrap();
        let hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        validate_body_shape(
            "io.marketplace.offer.upserted",
            &json!({
                "offer_id": "offer:shop.example:01JOFFER",
                "product_id": "prod:shop.example:01JPROD",
                "seller_id": "seller:shop.example:01JSELLER",
                "revision": 1,
                "status": "active",
                "price": {"amount": "100.00", "currency": "USD"},
                "payment_terms": {"capture_policy": "after_entitlement", "adapter_policy": "seller_supported"},
                "entitlement": {"type": "booking_slot", "delivery": "external"},
                "availability": {"mode": "limited"},
                "seller_terms_hash": hash,
                "offer_terms_hash": hash
            }),
        )
        .unwrap();
        validate_body_shape(
            "io.marketplace.order.created",
            &json!({
                "order_id": "ord:customer.example:01JORDER",
                "room_id": "!order:customer.example",
                "customer_id": "customer:customer.example:01JCUST",
                "seller_id": "seller:shop.example:01JSELLER",
                "offer_id": "offer:shop.example:01JOFFER",
                "offer_revision": 1,
                "catalog_snapshot_id": "snap:shop.example:01JSNAP",
                "quantity": 1,
                "price": {"amount": "100.00", "currency": "USD"},
                "payment_adapter": "mock",
                "payment_capture_policy": "after_entitlement",
                "entitlement_type": "booking_slot",
                "arbiter_instance": "arbiter.example",
                "arbiter_actor": "arbiter:arbiter.example:01JARB",
                "seller_terms_hash": hash,
                "offer_terms_hash": hash,
                "arbitration_policy_id": "standard",
                "arbitration_policy_version": "1",
                "arbitration_window": "P14D",
                "expires_at": "2026-05-04T10:30:00Z"
            }),
        )
        .unwrap();
        validate_body_shape(
            "io.marketplace.order.accepted",
            &json!({
                "order_id": "ord:customer.example:01JORDER",
                "offer_revision": 1,
                "seller_terms_hash": hash,
                "offer_terms_hash": hash,
                "payment_capture_policy": "after_entitlement",
                "arbitration_policy_version": "1"
            }),
        )
        .unwrap();
        validate_body_shape(
            "io.marketplace.payment.intent.created",
            &json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "adapter": "mock",
                "amount": "100.00",
                "currency": "USD",
                "capture_policy": "after_entitlement",
                "idempotency_key": "idem",
                "provider_ref": "mock_pi",
                "confirmation": {"method": "redirect", "uri": "https://pay.example"},
                "expires_at": "2026-05-04T10:30:00Z"
            }),
        )
        .unwrap();
        validate_body_shape(
            "io.marketplace.entitlement.granted",
            &json!({
                "order_id": "ord:customer.example:01JORDER",
                "entitlement_id": "ent:customer.example:01JENT",
                "type": "booking_slot",
                "external_ref": "booking",
                "valid_from": "2026-05-04T10:00:00Z",
                "valid_until": "2026-05-04T11:00:00Z",
                "evidence": {"kind": "booking", "uri": "https://delivery.example/e", "sha256": hash}
            }),
        )
        .unwrap();

        assert_eq!(
            required_literal(&json!({"status": "active"}), "status", "suspended")
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_enum_map(
                &serde_json::Map::from_iter([("mode".into(), json!("bad"))]),
                "mode",
                &["ok"],
            )
            .unwrap_err()
            .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_string_array(&json!({"items": ["ok", ""]}), "items")
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_matrix_user_array(&json!({"users": ["bad"]}), "users")
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_enum_array(&json!({"items": ["bad"]}), "items", &["ok"])
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_instance_id(&json!({"instance": "bad"}), "instance")
                .unwrap_err()
                .code,
            ValidationCode::InvalidId
        );
        assert_eq!(
            required_room_id(&json!({"room": "bad"}), "room")
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_event_id(&json!({"event": "bad"}), "event")
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_url(&json!({"uri": "mxc://x"}), "uri")
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_url_map(
                &serde_json::Map::from_iter([("uri".into(), json!("mxc://x"))]),
                "uri",
            )
            .unwrap_err()
            .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_utc(&json!({"at": "2026-05-04T10:00:00+03:00"}), "at")
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_amount(&json!({"amount": "10.bad"}), "amount")
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_currency_map(
                &serde_json::Map::from_iter([("currency".into(), json!("usd"))]),
                "currency",
            )
            .unwrap_err()
            .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_sha256(&json!({"hash": "sha256:BAD"}), "hash")
                .unwrap_err()
                .code,
            ValidationCode::MissingRequiredField
        );
        assert_eq!(
            required_evidence(
                &json!({"evidence": {"kind": "x", "uri": "file:///x", "sha256": "bad"}})
            )
            .unwrap_err()
            .code,
            ValidationCode::MissingRequiredField
        );
    }
}
