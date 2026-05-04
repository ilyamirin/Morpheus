use std::collections::{HashMap, HashSet};

pub use morpheus_protocol::Money;
use morpheus_protocol::{ValidationCode, ValidationError, ValidationResult, parse_object_instance};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRecord {
    pub snapshot_id: String,
    pub sequence: u64,
    pub sha256: String,
    pub covers_events_until: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SellerRecord {
    pub seller_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProductRecord {
    product_id: String,
    seller_id: String,
    revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferRecord {
    pub offer_id: String,
    pub product_id: String,
    pub seller_id: String,
    pub revision: u64,
    pub price: Money,
    pub entitlement_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct CatalogIndex {
    instance_id: String,
    snapshot: Option<SnapshotRecord>,
    sellers: HashMap<String, SellerRecord>,
    products: HashMap<String, ProductRecord>,
    offers: HashMap<String, OfferRecord>,
}

impl CatalogIndex {
    pub fn new(instance_id: impl Into<String>) -> Self {
        Self {
            instance_id: instance_id.into(),
            ..Self::default()
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: SnapshotRecord) -> ValidationResult<()> {
        if let Some(current) = &self.snapshot {
            if snapshot.sequence == current.sequence {
                if snapshot.sha256 != current.sha256 {
                    return Err(ValidationError::with_details(
                        ValidationCode::CatalogReferenceMismatch,
                        "Snapshot hash mismatch",
                        json!({ "snapshot_id": snapshot.snapshot_id }),
                    ));
                }
                return Ok(());
            }
            if snapshot.sequence < current.sequence {
                return Err(ValidationError::new(
                    ValidationCode::RevisionRollback,
                    "Snapshot sequence rollback",
                ));
            }
        }
        self.snapshot = Some(snapshot);
        Ok(())
    }

    pub fn upsert_seller(&mut self, seller: SellerRecord) -> ValidationResult<()> {
        self.assert_instance("seller_id", &seller.seller_id)?;
        self.sellers.insert(seller.seller_id.clone(), seller);
        Ok(())
    }

    pub fn upsert_product(
        &mut self,
        product_id: impl Into<String>,
        seller_id: impl Into<String>,
        revision: u64,
    ) -> ValidationResult<()> {
        let product = ProductRecord {
            product_id: product_id.into(),
            seller_id: seller_id.into(),
            revision,
        };
        self.assert_instance("product_id", &product.product_id)?;
        self.assert_instance("seller_id", &product.seller_id)?;
        self.assert_seller_active(&product.seller_id)?;
        if let Some(current) = self.products.get(&product.product_id)
            && product.revision <= current.revision
        {
            return Err(ValidationError::new(
                ValidationCode::RevisionRollback,
                "Product revision rollback",
            ));
        }
        self.products.insert(product.product_id.clone(), product);
        Ok(())
    }

    pub fn upsert_offer(&mut self, offer: OfferRecord) -> ValidationResult<()> {
        self.assert_instance("offer_id", &offer.offer_id)?;
        self.assert_instance("product_id", &offer.product_id)?;
        self.assert_instance("seller_id", &offer.seller_id)?;
        self.assert_seller_active(&offer.seller_id)?;
        let product = self.products.get(&offer.product_id).ok_or_else(|| {
            ValidationError::new(
                ValidationCode::CatalogReferenceMismatch,
                format!("Unknown product {}", offer.product_id),
            )
        })?;
        if product.seller_id != offer.seller_id {
            return Err(ValidationError::new(
                ValidationCode::CatalogReferenceMismatch,
                "Product seller mismatch",
            ));
        }
        if let Some(current) = self.offers.get(&offer.offer_id)
            && offer.revision <= current.revision
        {
            return Err(ValidationError::new(
                ValidationCode::RevisionRollback,
                "Offer revision rollback",
            ));
        }
        self.offers.insert(offer.offer_id.clone(), offer);
        Ok(())
    }

    pub fn get_offer(&self, offer_id: &str) -> Option<&OfferRecord> {
        let offer = self.offers.get(offer_id)?;
        let seller = self.sellers.get(&offer.seller_id)?;
        if seller.status != "active" {
            return None;
        }
        Some(offer)
    }

    fn assert_seller_active(&self, seller_id: &str) -> ValidationResult<()> {
        match self.sellers.get(seller_id) {
            Some(seller) if seller.status == "active" => Ok(()),
            _ => Err(ValidationError::new(
                ValidationCode::ActorNotActive,
                format!("Seller {seller_id} is not active"),
            )),
        }
    }

    fn assert_instance(&self, field: &str, id: &str) -> ValidationResult<()> {
        let actual = parse_object_instance(id)?;
        if actual != self.instance_id {
            return Err(ValidationError::with_details(
                ValidationCode::CatalogReferenceMismatch,
                format!("Catalog reference mismatch for {field}"),
                json!({ "expected": self.instance_id, "actual": actual }),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct AllowlistPolicy {
    entries: HashMap<String, HashSet<String>>,
}

impl AllowlistPolicy {
    pub fn new<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = (String, Vec<String>)>,
    {
        Self {
            entries: entries
                .into_iter()
                .map(|(instance, caps)| (instance, caps.into_iter().collect()))
                .collect(),
        }
    }

    pub fn can(&self, instance_id: &str, capability: &str) -> bool {
        self.entries
            .get(instance_id)
            .is_some_and(|caps| caps.contains(capability))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerBinding {
    pub customer_id: String,
    pub status: String,
    pub accepted_payment_adapters: Vec<String>,
    pub accepted_arbitration_policies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderCreatedBody {
    pub order_id: String,
    pub room_id: String,
    pub customer_id: String,
    pub seller_id: String,
    pub offer_id: String,
    pub offer_revision: u64,
    pub catalog_snapshot_id: String,
    pub quantity: u64,
    pub price: Money,
    pub payment_adapter: String,
    pub entitlement_type: String,
    pub arbiter_instance: String,
    pub arbiter_actor: String,
    pub arbitration_policy_id: String,
    pub arbitration_window: String,
    pub expires_at: String,
}

pub fn validate_order_created(
    order: &OrderCreatedBody,
    catalog: &CatalogIndex,
    allowlist: &AllowlistPolicy,
    customer: &CustomerBinding,
) -> ValidationResult<()> {
    if customer.customer_id != order.customer_id {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Order customer does not match customer binding",
        ));
    }
    if customer.status != "active" {
        return Err(ValidationError::new(
            ValidationCode::ActorNotActive,
            "Customer is not active",
        ));
    }
    if !customer
        .accepted_payment_adapters
        .contains(&order.payment_adapter)
    {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Order payment adapter is not accepted by customer binding",
        ));
    }
    if !customer
        .accepted_arbitration_policies
        .contains(&order.arbitration_policy_id)
    {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Order arbitration policy is not accepted by customer binding",
        ));
    }
    let seller_instance = parse_object_instance(&order.offer_id)?;
    if !allowlist.can(seller_instance, "orders") {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Seller instance is not allowlisted for orders",
        ));
    }
    if !allowlist.can(&order.arbiter_instance, "arbitration") {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Arbiter instance is not allowlisted",
        ));
    }
    let offer = catalog.get_offer(&order.offer_id).ok_or_else(|| {
        ValidationError::new(ValidationCode::CatalogReferenceMismatch, "Offer not found")
    })?;
    if offer.seller_id != order.seller_id
        || offer.revision != order.offer_revision
        || offer.entitlement_type != order.entitlement_type
    {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Order terms do not match trusted catalog",
        ));
    }
    assert_money_equal(&offer.price, &order.price)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderAuthorities {
    pub seller_as_user: String,
    pub customer_as_user: String,
    pub arbiter_as_user: String,
    pub payment_as_users: Vec<String>,
}

pub fn assert_event_authority(
    event_type: &str,
    sender: &str,
    authorities: &OrderAuthorities,
) -> ValidationResult<()> {
    let payment = event_type.starts_with("io.marketplace.payment.");
    let entitlement = event_type.starts_with("io.marketplace.entitlement.");
    if payment {
        if sender == authorities.seller_as_user
            || authorities
                .payment_as_users
                .iter()
                .any(|user| user == sender)
        {
            return Ok(());
        }
        return Err(ValidationError::new(
            ValidationCode::UnauthorizedSender,
            "Expected seller/payment authority",
        ));
    }
    if entitlement
        || matches!(
            event_type,
            "io.marketplace.order.accepted"
                | "io.marketplace.order.rejected"
                | "io.marketplace.order.completed"
        )
    {
        return assert_sender(sender, &authorities.seller_as_user, "seller");
    }
    if matches!(
        event_type,
        "io.marketplace.dispute.ruling.issued" | "io.marketplace.dispute.closed"
    ) {
        return assert_sender(sender, &authorities.arbiter_as_user, "arbiter");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentIntent {
    pub payment_id: String,
    pub provider_ref: String,
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentCapture {
    pub payment_id: String,
    pub provider_ref: String,
}

pub trait PaymentAdapter {
    fn create_intent(&self, order_id: &str, amount: &str, currency: &str) -> PaymentIntent;
    fn authorize(&self, payment_id: &str) -> String;
    fn capture(&self, payment_id: &str) -> PaymentCapture;
    fn refund(&self, payment_id: &str) -> String;
    fn verify_webhook(&self, provider_ref: &str) -> bool;
}

#[derive(Debug, Clone, Default)]
pub struct MockPaymentAdapter;

impl PaymentAdapter for MockPaymentAdapter {
    fn create_intent(&self, order_id: &str, amount: &str, currency: &str) -> PaymentIntent {
        let stable = stable_ref(order_id);
        PaymentIntent {
            payment_id: format!("pay:mock:{stable}"),
            provider_ref: format!("mock_pi_{stable}"),
            amount: amount.into(),
            currency: currency.into(),
        }
    }

    fn authorize(&self, payment_id: &str) -> String {
        format!("mock_auth_{}", stable_ref(payment_id))
    }

    fn capture(&self, payment_id: &str) -> PaymentCapture {
        PaymentCapture {
            payment_id: payment_id.into(),
            provider_ref: format!("mock_ch_{}", stable_ref(payment_id)),
        }
    }

    fn refund(&self, payment_id: &str) -> String {
        format!("mock_rf_{}", stable_ref(payment_id))
    }

    fn verify_webhook(&self, provider_ref: &str) -> bool {
        provider_ref.starts_with("mock_")
    }
}

pub fn validate_entitlement_secret_safety(value: &str) -> ValidationResult<()> {
    let lower = value.to_ascii_lowercase();
    if lower.contains("bearer")
        || lower.contains("access_token=")
        || lower.contains("token=")
        || lower.contains("secret=")
    {
        Err(ValidationError::new(
            ValidationCode::PolicyViolation,
            "Entitlement metadata must not contain secret access credentials",
        ))
    } else {
        Ok(())
    }
}

fn stable_ref(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn assert_sender(sender: &str, expected: &str, role: &str) -> ValidationResult<()> {
    if sender == expected {
        Ok(())
    } else {
        Err(ValidationError::with_details(
            ValidationCode::UnauthorizedSender,
            format!("Expected {role} authority"),
            json!({ "sender": sender, "expected": expected }),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderFlowEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub body: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderState {
    Draft,
    Created,
    Accepted,
    PaymentIntentCreated,
    PaymentAuthorized,
    PaymentCaptured,
    EntitlementGrantedBeforeCapture,
    EntitlementGranted,
    Completed,
    Cancelled,
    Rejected,
    Refunded,
    DisputeResolved,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderDecision {
    pub final_state: OrderState,
}

#[derive(Default)]
struct OrderContext {
    order_id: Option<String>,
    payment_id: Option<String>,
    captured_payment_id: Option<String>,
    entitlement_id: Option<String>,
    capture_policy: Option<String>,
}

pub fn validate_order_sequence(events: &[OrderFlowEvent]) -> ValidationResult<OrderDecision> {
    let mut state = OrderState::Draft;
    let mut context = OrderContext::default();

    for event in events {
        if event.event_type == "io.marketplace.actor.customer.bound" {
            continue;
        }
        validate_order_event_refs(event, &mut context)?;
        state = apply_order_transition(state, &event.event_type, &context)?;
    }

    Ok(OrderDecision { final_state: state })
}

fn validate_order_event_refs(
    event: &OrderFlowEvent,
    context: &mut OrderContext,
) -> ValidationResult<()> {
    if event.event_type == "io.marketplace.order.created" {
        context.order_id = Some(required_string(&event.body, "order_id")?.to_string());
        return Ok(());
    }
    let expected_order_id = context.order_id.as_deref().ok_or_else(|| {
        ValidationError::new(
            ValidationCode::InvalidStateTransition,
            "Order sequence must start with order.created",
        )
    })?;
    let actual_order_id = required_string(&event.body, "order_id")?;
    if actual_order_id != expected_order_id {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Order event references a different order_id",
        ));
    }
    match event.event_type.as_str() {
        "io.marketplace.payment.intent.created" => {
            context.payment_id = Some(required_string(&event.body, "payment_id")?.to_string());
            context.capture_policy =
                Some(required_string(&event.body, "capture_policy")?.to_string());
        }
        "io.marketplace.payment.captured" => {
            let payment_id = required_string(&event.body, "payment_id")?;
            if Some(payment_id) != context.payment_id.as_deref() {
                return Err(ValidationError::new(
                    ValidationCode::PaymentTermsMismatch,
                    "payment.captured references a different payment_id",
                ));
            }
            context.captured_payment_id = Some(payment_id.to_string());
        }
        "io.marketplace.entitlement.granted" => {
            if context.capture_policy.as_deref() == Some("before_entitlement")
                && context.captured_payment_id.is_none()
            {
                return Err(ValidationError::new(
                    ValidationCode::InvalidStateTransition,
                    "entitlement.granted requires captured payment when capture_policy=before_entitlement",
                ));
            }
            context.entitlement_id =
                Some(required_string(&event.body, "entitlement_id")?.to_string());
        }
        _ => {}
    }
    Ok(())
}

fn apply_order_transition(
    state: OrderState,
    event_type: &str,
    context: &OrderContext,
) -> ValidationResult<OrderState> {
    let next = match (state, event_type) {
        (OrderState::Draft, "io.marketplace.order.created") => OrderState::Created,
        (OrderState::Created, "io.marketplace.order.accepted") => OrderState::Accepted,
        (OrderState::Created, "io.marketplace.order.cancelled") => OrderState::Cancelled,
        (OrderState::Created, "io.marketplace.order.rejected") => OrderState::Rejected,
        (OrderState::Accepted, "io.marketplace.payment.intent.created") => {
            OrderState::PaymentIntentCreated
        }
        (OrderState::PaymentIntentCreated, "io.marketplace.payment.authorized") => {
            OrderState::PaymentAuthorized
        }
        (OrderState::PaymentAuthorized, "io.marketplace.payment.captured") => {
            OrderState::PaymentCaptured
        }
        (OrderState::PaymentAuthorized, "io.marketplace.entitlement.granted") => {
            OrderState::EntitlementGrantedBeforeCapture
        }
        (OrderState::PaymentCaptured, "io.marketplace.entitlement.granted") => {
            OrderState::EntitlementGranted
        }
        (OrderState::EntitlementGrantedBeforeCapture, "io.marketplace.payment.captured") => {
            OrderState::EntitlementGranted
        }
        (OrderState::EntitlementGranted, "io.marketplace.order.completed") => OrderState::Completed,
        (OrderState::PaymentCaptured, "io.marketplace.payment.refunded") => OrderState::Refunded,
        _ => {
            return Err(ValidationError::with_details(
                ValidationCode::InvalidStateTransition,
                "Invalid order state transition",
                json!({ "state": format!("{state:?}"), "event_type": event_type, "order_id": context.order_id }),
            ));
        }
    };
    Ok(next)
}

fn required_string<'a>(body: &'a Value, key: &str) -> ValidationResult<&'a str> {
    body.get(key).and_then(Value::as_str).ok_or_else(|| {
        ValidationError::with_details(
            ValidationCode::MissingRequiredField,
            format!("Missing required field {key}"),
            json!({ "field": key }),
        )
    })
}

fn assert_money_equal(expected: &Money, actual: &Money) -> ValidationResult<()> {
    let expected_amount = expected.amount.parse::<f64>().unwrap_or(f64::NAN);
    let actual_amount = actual.amount.parse::<f64>().unwrap_or(f64::NAN);
    if expected.currency == actual.currency
        && (expected_amount - actual_amount).abs() < f64::EPSILON
    {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "Order price does not match offer price",
        ))
    }
}

pub mod fixtures {
    use super::*;

    pub fn valid_catalog() -> CatalogIndex {
        let mut catalog = CatalogIndex::new("shop.example");
        catalog
            .apply_snapshot(SnapshotRecord {
                snapshot_id: "snap_01J".into(),
                sequence: 1,
                sha256: "sha256:abc".into(),
                covers_events_until: "$a".into(),
            })
            .unwrap();
        catalog
            .upsert_seller(SellerRecord {
                seller_id: "seller:shop.example:01JSELLER".into(),
                status: "active".into(),
            })
            .unwrap();
        catalog
            .upsert_product(
                "prod:shop.example:01JPROD",
                "seller:shop.example:01JSELLER",
                7,
            )
            .unwrap();
        catalog
            .upsert_offer(OfferRecord {
                offer_id: "offer:shop.example:01JOFFER".into(),
                product_id: "prod:shop.example:01JPROD".into(),
                seller_id: "seller:shop.example:01JSELLER".into(),
                revision: 3,
                price: Money {
                    amount: "100.00".into(),
                    currency: "USD".into(),
                },
                entitlement_type: "booking_slot".into(),
            })
            .unwrap();
        catalog
    }

    pub fn valid_order_created() -> OrderCreatedBody {
        OrderCreatedBody {
            order_id: "ord:customer.example:01JORDER".into(),
            room_id: "!order:customer.example".into(),
            customer_id: "customer:customer.example:01JCUST".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            offer_id: "offer:shop.example:01JOFFER".into(),
            offer_revision: 3,
            catalog_snapshot_id: "snap_01J".into(),
            quantity: 1,
            price: Money {
                amount: "100.00".into(),
                currency: "USD".into(),
            },
            payment_adapter: "mock".into(),
            entitlement_type: "booking_slot".into(),
            arbiter_instance: "arbiter.example".into(),
            arbiter_actor: "arbiter:arbiter.example:default".into(),
            arbitration_policy_id: "standard-digital-v1".into(),
            arbitration_window: "P14D".into(),
            expires_at: "2026-05-04T10:30:00Z".into(),
        }
    }

    pub fn valid_order_flow() -> Vec<OrderFlowEvent> {
        vec![
            OrderFlowEvent {
                event_type: "io.marketplace.actor.customer.bound".into(),
                body: json!({ "customer_id": "customer:customer.example:01JCUST", "status": "active" }),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.order.created".into(),
                body: serde_json::to_value(valid_order_created()).unwrap(),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.order.accepted".into(),
                body: json!({ "order_id": "ord:customer.example:01JORDER" }),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.payment.intent.created".into(),
                body: json!({
                    "order_id": "ord:customer.example:01JORDER",
                    "payment_id": "pay:shop.example:01JPAY",
                    "adapter": "mock",
                    "amount": "100.00",
                    "currency": "USD",
                    "capture_policy": "before_entitlement"
                }),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.payment.authorized".into(),
                body: json!({ "order_id": "ord:customer.example:01JORDER", "payment_id": "pay:shop.example:01JPAY" }),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.payment.captured".into(),
                body: json!({ "order_id": "ord:customer.example:01JORDER", "payment_id": "pay:shop.example:01JPAY" }),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.entitlement.granted".into(),
                body: json!({ "order_id": "ord:customer.example:01JORDER", "payment_id": "pay:shop.example:01JPAY", "entitlement_id": "ent:shop.example:01JENT" }),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.order.completed".into(),
                body: json!({ "order_id": "ord:customer.example:01JORDER" }),
            },
        ]
    }
}
