use morpheus_protocol::{
    MarketplaceEventValidationContext, MarketplaceEventValidationResult, RoomProfile,
    ValidationCode, ValidationError, ValidationResult, assert_sha256_matches,
    parse_object_instance, validate_marketplace_event,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub use morpheus_protocol::{Money, validation_disposition};

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
pub struct ProductRecord {
    pub product_id: String,
    pub seller_id: String,
    pub revision: u64,
    pub terms_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferRecord {
    pub offer_id: String,
    pub product_id: String,
    pub seller_id: String,
    pub revision: u64,
    pub price: Money,
    pub entitlement_type: String,
    pub payment_capture_policy: Option<String>,
    pub offer_terms_hash: Option<String>,
    pub seller_terms_hash: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CatalogIndex {
    pub instance_id: String,
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
                        json!({ "snapshot": snapshot.snapshot_id }),
                    ));
                }
                return Ok(());
            }
            if snapshot.sequence < current.sequence {
                return Err(snapshot_sequence_rollback());
            }
        }
        self.snapshot = Some(snapshot);
        Ok(())
    }

    pub fn upsert_seller(&mut self, seller: SellerRecord) -> ValidationResult<()> {
        self.assert_catalog_instance("seller_id", &seller.seller_id)?;
        self.sellers.insert(seller.seller_id.clone(), seller);
        Ok(())
    }

    pub fn upsert_product(&mut self, product: ProductRecord) -> ValidationResult<()> {
        self.assert_catalog_instance("product_id", &product.product_id)?;
        self.assert_catalog_instance("seller_id", &product.seller_id)?;
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
        self.assert_catalog_instance("offer_id", &offer.offer_id)?;
        self.assert_catalog_instance("product_id", &offer.product_id)?;
        self.assert_catalog_instance("seller_id", &offer.seller_id)?;
        self.assert_seller_active(&offer.seller_id)?;
        let product = self.products.get(&offer.product_id).ok_or_else(|| {
            ValidationError::with_details(
                ValidationCode::CatalogReferenceMismatch,
                format!("Unknown product {}", offer.product_id),
                json!({ "offer": offer.offer_id }),
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
        let product = self.products.get(&offer.product_id)?;
        if product.seller_id != offer.seller_id {
            return None;
        }
        Some(offer)
    }

    pub fn remove_object(&mut self, object_id: &str) {
        if object_id.starts_with("offer:") {
            self.offers.remove(object_id);
        }
        if object_id.starts_with("prod:") {
            self.products.remove(object_id);
            self.offers
                .retain(|_, offer| offer.product_id.as_str() != object_id);
        }
    }

    pub fn offer_count(&self) -> usize {
        self.offers.len()
    }

    fn assert_seller_active(&self, seller_id: &str) -> ValidationResult<()> {
        match self.sellers.get(seller_id) {
            Some(seller) if seller.status == "active" => Ok(()),
            _ => Err(ValidationError::with_details(
                ValidationCode::ActorNotActive,
                format!("Seller {seller_id} is not active"),
                json!({ "sellerId": seller_id }),
            )),
        }
    }

    fn assert_catalog_instance(&self, field: &str, id: &str) -> ValidationResult<()> {
        let actual = parse_object_instance(id)?;
        if actual == self.instance_id {
            Ok(())
        } else {
            Err(ValidationError::with_details(
                ValidationCode::CatalogReferenceMismatch,
                format!("Catalog reference mismatch for {field}"),
                json!({ "expected": self.instance_id, "actual": actual }),
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogSnapshotDocument {
    pub snapshot: SnapshotRecord,
    pub sellers: Vec<SellerRecord>,
    pub products: Vec<ProductRecord>,
    pub offers: Vec<OfferRecord>,
    pub tombstones: Vec<String>,
    pub sequence: u64,
    pub covers_events_until: String,
}

#[derive(Debug, Clone)]
pub struct CatalogDeltaEvent {
    pub event_type: String,
    pub event_id: String,
    pub catalog_sequence: u64,
    pub body: Value,
}

pub fn validate_catalog_snapshot(
    document: &CatalogSnapshotDocument,
    expected_hash: &str,
) -> ValidationResult<()> {
    if !morpheus_protocol::is_protocol_object_id(&document.snapshot.snapshot_id, Some("snap")) {
        return Err(ValidationError::new(
            ValidationCode::InvalidId,
            "Invalid snapshot id",
        ));
    }
    let canonical = json!({
        "sellers": document.sellers.iter().map(|seller| json!({"seller_id": seller.seller_id, "status": seller.status})).collect::<Vec<_>>(),
        "products": document.products.iter().map(|product| json!({"product_id": product.product_id, "seller_id": product.seller_id, "revision": product.revision, "terms_hash": product.terms_hash})).collect::<Vec<_>>(),
        "offers": document.offers.iter().map(|offer| json!({"offer_id": offer.offer_id, "product_id": offer.product_id, "seller_id": offer.seller_id, "revision": offer.revision, "price": offer.price, "entitlement_type": offer.entitlement_type, "payment_capture_policy": offer.payment_capture_policy, "offer_terms_hash": offer.offer_terms_hash, "seller_terms_hash": offer.seller_terms_hash})).collect::<Vec<_>>(),
        "tombstones": document.tombstones,
        "sequence": document.sequence,
        "covers_events_until": document.covers_events_until,
    });
    assert_sha256_matches(&canonical, expected_hash)
}

pub fn replay_catalog_timeline(
    instance_id: &str,
    snapshot: CatalogSnapshotDocument,
    deltas: &[CatalogDeltaEvent],
) -> ValidationResult<CatalogIndex> {
    validate_catalog_snapshot(&snapshot, &snapshot.snapshot.sha256)?;
    let mut catalog = CatalogIndex::new(instance_id);
    catalog.apply_snapshot(snapshot.snapshot.clone())?;
    for seller in snapshot.sellers {
        catalog.upsert_seller(seller)?;
    }
    for product in snapshot.products {
        catalog.upsert_product(product)?;
    }
    for offer in snapshot.offers {
        catalog.upsert_offer(offer)?;
    }
    for tombstone in snapshot.tombstones {
        catalog.remove_object(&tombstone);
    }

    let mut seen = HashSet::new();
    let mut expected_sequence = snapshot.sequence + 1;
    for event in deltas {
        if !seen.insert(event.event_id.clone()) {
            continue;
        }
        if event.catalog_sequence != expected_sequence {
            return Err(ValidationError::with_details(
                ValidationCode::CatalogReferenceMismatch,
                "Catalog delta sequence gap",
                json!({ "expectedSequence": expected_sequence, "actualSequence": event.catalog_sequence }),
            ));
        }
        expected_sequence += 1;
        apply_catalog_delta(&mut catalog, event)?;
    }
    Ok(catalog)
}

fn apply_catalog_delta(
    catalog: &mut CatalogIndex,
    event: &CatalogDeltaEvent,
) -> ValidationResult<()> {
    match event.event_type.as_str() {
        "io.marketplace.actor.seller.announced" | "io.marketplace.actor.seller.suspended" => {
            catalog.upsert_seller(SellerRecord {
                seller_id: string_field(&event.body, "seller_id")?.to_string(),
                status: string_field(&event.body, "status")?.to_string(),
            })
        }
        "io.marketplace.product.upserted" => catalog.upsert_product(ProductRecord {
            product_id: string_field(&event.body, "product_id")?.to_string(),
            seller_id: string_field(&event.body, "seller_id")?.to_string(),
            revision: u64_field(&event.body, "revision")?,
            terms_hash: string_field_opt(&event.body, "terms_hash").map(str::to_string),
        }),
        "io.marketplace.product.withdrawn" => {
            catalog.remove_object(string_field(&event.body, "product_id")?);
            Ok(())
        }
        "io.marketplace.offer.upserted" => {
            let price = event.body.get("price").ok_or_else(|| missing("price"))?;
            catalog.upsert_offer(OfferRecord {
                offer_id: string_field(&event.body, "offer_id")?.to_string(),
                product_id: string_field(&event.body, "product_id")?.to_string(),
                seller_id: string_field(&event.body, "seller_id")?.to_string(),
                revision: u64_field(&event.body, "revision")?,
                price: Money {
                    amount: string_field(price, "amount")?.to_string(),
                    currency: string_field(price, "currency")?.to_string(),
                },
                entitlement_type: event
                    .body
                    .get("entitlement")
                    .and_then(|value| string_field(value, "type").ok())
                    .unwrap_or("external_entitlement")
                    .to_string(),
                payment_capture_policy: event
                    .body
                    .get("payment_terms")
                    .and_then(|value| string_field(value, "capture_policy").ok())
                    .map(str::to_string),
                offer_terms_hash: string_field_opt(&event.body, "offer_terms_hash")
                    .map(str::to_string),
                seller_terms_hash: string_field_opt(&event.body, "seller_terms_hash")
                    .map(str::to_string),
            })
        }
        "io.marketplace.offer.withdrawn" => {
            catalog.remove_object(string_field(&event.body, "offer_id")?);
            Ok(())
        }
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowlistCapability {
    Catalog,
    Orders,
    Arbitration,
    Payments,
    Indexing,
}

impl AllowlistCapability {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Orders => "orders",
            Self::Arbitration => "arbitration",
            Self::Payments => "payments",
            Self::Indexing => "indexing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowlistEntry {
    pub capabilities: Vec<AllowlistCapability>,
    pub status: String,
    pub valid_until_epoch_ms: Option<i64>,
    pub audit_reason: Option<String>,
    pub updated_by: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AllowlistPolicy {
    entries: HashMap<String, AllowlistEntry>,
}

impl AllowlistPolicy {
    pub fn new<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = (String, Vec<String>)>,
    {
        Self {
            entries: entries
                .into_iter()
                .map(|(instance, capabilities)| {
                    (
                        instance,
                        AllowlistEntry {
                            capabilities: capabilities
                                .into_iter()
                                .filter_map(|capability| parse_capability(&capability))
                                .collect(),
                            status: "active".into(),
                            valid_until_epoch_ms: None,
                            audit_reason: None,
                            updated_by: None,
                            updated_at: None,
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn from_entries<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = (String, AllowlistEntry)>,
    {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    pub fn can(&self, instance_id: &str, capability: &str) -> bool {
        self.can_at(instance_id, capability, 0)
    }

    pub fn can_at(&self, instance_id: &str, capability: &str, now_epoch_ms: i64) -> bool {
        self.entries.get(instance_id).is_some_and(|entry| {
            entry.status == "active"
                && entry
                    .valid_until_epoch_ms
                    .is_none_or(|valid_until| valid_until > now_epoch_ms)
                && entry
                    .capabilities
                    .iter()
                    .any(|entry_capability| entry_capability.as_str() == capability)
        })
    }

    pub fn can_replay_existing_order(&self, instance_id: &str) -> bool {
        self.entries.contains_key(instance_id)
    }
}

pub fn validate_allowlist_policy(
    policy: &AllowlistPolicy,
    now_epoch_ms: i64,
) -> ValidationResult<()> {
    for (instance_id, entry) in &policy.entries {
        if instance_id.is_empty() {
            return Err(ValidationError::new(
                ValidationCode::PolicyViolation,
                "Allowlist instance id is required",
            ));
        }
        if entry.capabilities.is_empty()
            || !matches!(entry.status.as_str(), "active" | "revoked")
            || entry.audit_reason.as_deref().is_some_and(str::is_empty)
            || entry
                .updated_by
                .as_deref()
                .is_some_and(|user| !is_matrix_user_id(user))
        {
            return Err(ValidationError::new(
                ValidationCode::PolicyViolation,
                "Invalid allowlist entry",
            ));
        }
        if entry.status == "active"
            && entry
                .valid_until_epoch_ms
                .is_some_and(|valid_until| valid_until <= now_epoch_ms)
        {
            return Err(ValidationError::new(
                ValidationCode::PolicyViolation,
                "Expired allowlist entries must be revoked",
            ));
        }
    }
    Ok(())
}

pub fn should_index_catalog_room(policy: &AllowlistPolicy, instance_id: &str) -> bool {
    policy.can(instance_id, "catalog") && policy.can(instance_id, "indexing")
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
    pub payment_capture_policy: String,
    pub entitlement_type: String,
    pub seller_terms_hash: String,
    pub offer_terms_hash: String,
    pub arbiter_instance: String,
    pub arbiter_actor: String,
    pub arbitration_policy_id: String,
    pub arbitration_policy_version: String,
    pub arbitration_window: String,
    pub expires_at: String,
}

pub fn validate_order_created(
    order: &OrderCreatedBody,
    catalog: &CatalogIndex,
    allowlist: &AllowlistPolicy,
    customer: &CustomerBinding,
) -> ValidationResult<()> {
    if order.quantity != 1 {
        return Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "Order quantity is limited to one in v0.1",
        ));
    }
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
    if parse_object_instance(&order.arbiter_actor)? != order.arbiter_instance {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "arbiter_actor instance must match arbiter_instance",
        ));
    }
    let seller_instance = parse_object_instance(&order.offer_id)?;
    if !allowlist.can(seller_instance, "orders") {
        return Err(ValidationError::new(
            ValidationCode::InstanceNotAllowlisted,
            "Seller instance is not allowlisted for orders",
        ));
    }
    if !allowlist.can(&order.arbiter_instance, "arbitration") {
        return Err(ValidationError::new(
            ValidationCode::InstanceNotAllowlisted,
            "Arbiter instance is not allowlisted",
        ));
    }
    let offer = catalog.get_offer(&order.offer_id).ok_or_else(|| {
        ValidationError::new(ValidationCode::CatalogReferenceMismatch, "Offer not found")
    })?;
    if offer.seller_id != order.seller_id
        || offer.revision != order.offer_revision
        || offer.entitlement_type != order.entitlement_type
        || offer.payment_capture_policy.as_deref() != Some(order.payment_capture_policy.as_str())
        || offer.seller_terms_hash.as_deref() != Some(order.seller_terms_hash.as_str())
        || offer.offer_terms_hash.as_deref() != Some(order.offer_terms_hash.as_str())
    {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Order terms do not match trusted catalog",
        ));
    }
    assert_money_equal(&offer.price, &order.price)
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
    if matches!(
        event_type,
        "io.marketplace.order.created" | "io.marketplace.order.cancelled"
    ) {
        return assert_sender_in(
            sender,
            &[
                (&authorities.customer_as_user, "customer"),
                (&authorities.seller_as_user, "seller"),
            ],
        );
    }
    if matches!(
        event_type,
        "io.marketplace.order.accepted"
            | "io.marketplace.order.rejected"
            | "io.marketplace.order.completed"
    ) {
        return assert_sender(sender, &authorities.seller_as_user, "seller");
    }
    if event_type.starts_with("io.marketplace.payment.") {
        return assert_payment_sender(sender, authorities);
    }
    if event_type.starts_with("io.marketplace.entitlement.") {
        return assert_sender(sender, &authorities.seller_as_user, "seller");
    }
    if matches!(
        event_type,
        "io.marketplace.dispute.ruling.issued" | "io.marketplace.dispute.closed"
    ) {
        return assert_sender(sender, &authorities.arbiter_as_user, "arbiter");
    }
    if matches!(
        event_type,
        "io.marketplace.dispute.opened" | "io.marketplace.dispute.evidence.submitted"
    ) {
        return assert_sender_in(
            sender,
            &[
                (&authorities.seller_as_user, "seller"),
                (&authorities.customer_as_user, "customer"),
                (&authorities.arbiter_as_user, "arbiter"),
            ],
        );
    }
    Ok(())
}

fn assert_payment_sender(sender: &str, authorities: &OrderAuthorities) -> ValidationResult<()> {
    let seller_server = authorities
        .seller_as_user
        .split(':')
        .nth(1)
        .unwrap_or_default();
    let sender_server = sender.split(':').nth(1).unwrap_or_default();
    if sender_server != seller_server {
        return Err(ValidationError::new(
            ValidationCode::UnauthorizedSender,
            "Payment sender must be a seller-instance virtual user",
        ));
    }
    let mut allowed = vec![(&authorities.seller_as_user, "seller")];
    for user in &authorities.payment_as_users {
        allowed.push((user, "payment"));
    }
    assert_sender_in(sender, &allowed)
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

fn assert_sender_in(sender: &str, allowed: &[(&String, &str)]) -> ValidationResult<()> {
    if allowed.iter().any(|(user, _)| user.as_str() == sender) {
        Ok(())
    } else {
        Err(ValidationError::with_details(
            ValidationCode::UnauthorizedSender,
            "Expected marketplace party authority",
            json!({ "sender": sender }),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderState {
    Draft,
    Created,
    Accepted,
    PaymentIntentCreated,
    PaymentAuthorized,
    PaymentCaptured,
    RefundRequested,
    EntitlementGrantedBeforeCapture,
    EntitlementGranted,
    EntitlementActivated,
    EntitlementCompleted,
    Completed,
    Cancelled,
    Rejected,
    Refunded,
    ChargebackOpened,
    DisputeOpenedPrePayment,
    DisputeOpenedAfterCapture,
    DisputeOpenedAfterEntitlement,
    RulingIssuedPrePayment,
    RulingIssuedAfterCapture,
    RulingIssuedAfterEntitlement,
    DisputeResolved,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderFlowEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderDecision {
    pub final_state: OrderState,
}

#[derive(Debug, Clone)]
pub struct OrderTransitionGraph {
    pub state: OrderState,
}

pub type OrderStateMachine = OrderTransitionGraph;

impl Default for OrderTransitionGraph {
    fn default() -> Self {
        Self {
            state: OrderState::Draft,
        }
    }
}

impl OrderTransitionGraph {
    pub fn apply(&mut self, event_type: &str) -> ValidationResult<()> {
        self.state = transition(self.state, event_type)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
struct OrderFlowContext {
    order_id: Option<String>,
    customer_binding: Option<CustomerBinding>,
    order_terms: Option<OrderTerms>,
    payment_intent: Option<PaymentIntentTerms>,
    authorized_payment_id: Option<String>,
    captured_payment_id: Option<String>,
    captured_amount: Option<String>,
    captured_currency: Option<String>,
    entitlement_id: Option<String>,
    dispute_id: Option<String>,
    refund_constraint: Option<(String, String)>,
}

#[derive(Clone)]
struct OrderTerms {
    payment_adapter: String,
    capture_policy: String,
    amount: String,
    currency: String,
    offer_revision: u64,
    seller_terms_hash: String,
    offer_terms_hash: String,
    arbitration_policy_version: String,
}

#[derive(Clone)]
struct PaymentIntentTerms {
    payment_id: String,
    adapter: String,
    amount: String,
    currency: String,
    capture_policy: String,
}

pub fn validate_order_sequence(events: &[OrderFlowEvent]) -> ValidationResult<OrderDecision> {
    let mut machine = OrderTransitionGraph::default();
    let mut context = OrderFlowContext::default();
    for event in events {
        if event.event_type == "io.marketplace.actor.customer.bound" {
            validate_customer_bound(event, &mut context)?;
            continue;
        }
        validate_event_references(event, &mut context)?;
        machine.apply(&event.event_type)?;
    }
    Ok(OrderDecision {
        final_state: machine.state,
    })
}

fn validate_event_references(
    event: &OrderFlowEvent,
    context: &mut OrderFlowContext,
) -> ValidationResult<()> {
    if event.event_type == "io.marketplace.order.created" {
        let order_id = required_string(&event.body, "order_id")?.to_string();
        if context
            .order_id
            .as_deref()
            .is_some_and(|current| current != order_id)
        {
            return Err(ValidationError::new(
                ValidationCode::CatalogReferenceMismatch,
                "Order sequence contains multiple order ids",
            ));
        }
        context.order_id = Some(order_id);
        validate_created_order_terms(event, context)?;
        return Ok(());
    }
    let expected_order_id = context.order_id.as_deref().ok_or_else(|| {
        ValidationError::new(
            ValidationCode::InvalidStateTransition,
            "Order sequence must start with order.created before order-bound events",
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
        "io.marketplace.order.accepted" => validate_order_accepted(event, context),
        "io.marketplace.payment.intent.created" => validate_payment_intent(event, context),
        "io.marketplace.payment.authorized" => {
            context.authorized_payment_id = Some(require_intent_payment_id(event, context)?);
            Ok(())
        }
        "io.marketplace.payment.captured" => validate_payment_capture(event, context),
        "io.marketplace.payment.failed" | "io.marketplace.payment.cancelled" => {
            require_intent_payment_id(event, context).map(|_| ())
        }
        "io.marketplace.payment.refund.requested" | "io.marketplace.payment.refunded" => {
            require_refundable_payment_id(event, context).map(|_| ())
        }
        "io.marketplace.payment.chargeback.opened" => {
            require_captured_payment_id(event, context).map(|_| ())
        }
        "io.marketplace.entitlement.granted" => validate_entitlement_grant(event, context),
        "io.marketplace.entitlement.activated"
        | "io.marketplace.entitlement.completed"
        | "io.marketplace.entitlement.revoked"
        | "io.marketplace.entitlement.expired" => validate_entitlement_lifecycle(event, context),
        "io.marketplace.dispute.opened" => {
            context.dispute_id = Some(required_string(&event.body, "dispute_id")?.to_string());
            Ok(())
        }
        "io.marketplace.dispute.evidence.submitted" | "io.marketplace.dispute.closed" => {
            validate_dispute_lifecycle(event, context)
        }
        "io.marketplace.dispute.ruling.issued" => {
            validate_dispute_lifecycle(event, context)?;
            capture_ruling_remedy(event, context)
        }
        _ => Ok(()),
    }
}

fn validate_customer_bound(
    event: &OrderFlowEvent,
    context: &mut OrderFlowContext,
) -> ValidationResult<()> {
    if context.order_id.is_some() {
        return Err(ValidationError::new(
            ValidationCode::InvalidStateTransition,
            "customer.bound must appear before order.created in an order sequence",
        ));
    }
    let binding = CustomerBinding {
        customer_id: required_string(&event.body, "customer_id")?.to_string(),
        status: required_string(&event.body, "status")?.to_string(),
        accepted_payment_adapters: required_string_array(&event.body, "accepted_payment_adapters")?,
        accepted_arbitration_policies: {
            required_string_array(&event.body, "accepted_arbitration_policies")?
        },
    };
    if binding.status != "active" {
        return Err(ValidationError::new(
            ValidationCode::ActorNotActive,
            "Customer is not active",
        ));
    }
    context.customer_binding = Some(binding);
    Ok(())
}

fn validate_created_order_terms(
    event: &OrderFlowEvent,
    context: &mut OrderFlowContext,
) -> ValidationResult<()> {
    let customer_id = required_string(&event.body, "customer_id")?;
    let payment_adapter = required_string(&event.body, "payment_adapter")?;
    let capture_policy = required_string(&event.body, "payment_capture_policy")?;
    let arbitration_policy_id = required_string(&event.body, "arbitration_policy_id")?;
    let price = event.body.get("price").ok_or_else(|| missing("price"))?;
    let binding = context.customer_binding.as_ref().ok_or_else(|| {
        ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "order.created requires a preceding customer.bound event",
        )
    })?;
    if binding.customer_id != customer_id {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "order.created customer does not match customer.bound",
        ));
    }
    if !binding
        .accepted_payment_adapters
        .contains(&payment_adapter.to_string())
    {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "order.created payment adapter is not accepted by customer.bound",
        ));
    }
    if !binding
        .accepted_arbitration_policies
        .contains(&arbitration_policy_id.to_string())
    {
        return Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "order.created arbitration policy is not accepted by customer.bound",
        ));
    }
    context.order_terms = Some(OrderTerms {
        payment_adapter: payment_adapter.to_string(),
        capture_policy: capture_policy.to_string(),
        amount: required_string(price, "amount")?.to_string(),
        currency: required_string(price, "currency")?.to_string(),
        offer_revision: u64_field(&event.body, "offer_revision")?,
        seller_terms_hash: required_string(&event.body, "seller_terms_hash")?.to_string(),
        offer_terms_hash: required_string(&event.body, "offer_terms_hash")?.to_string(),
        arbitration_policy_version: required_string(&event.body, "arbitration_policy_version")?
            .to_string(),
    });
    Ok(())
}

fn validate_order_accepted(
    event: &OrderFlowEvent,
    context: &OrderFlowContext,
) -> ValidationResult<()> {
    let terms = require_order_terms(context)?;
    let actual = (
        u64_field(&event.body, "offer_revision")?,
        required_string(&event.body, "seller_terms_hash")?,
        required_string(&event.body, "offer_terms_hash")?,
        required_string(&event.body, "payment_capture_policy")?,
        required_string(&event.body, "arbitration_policy_version")?,
    );
    if actual.0 == terms.offer_revision
        && actual.1 == terms.seller_terms_hash
        && actual.2 == terms.offer_terms_hash
        && actual.3 == terms.capture_policy
        && actual.4 == terms.arbitration_policy_version
    {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "order.accepted terms do not match order.created terms",
        ))
    }
}

fn validate_payment_intent(
    event: &OrderFlowEvent,
    context: &mut OrderFlowContext,
) -> ValidationResult<()> {
    if context.payment_intent.is_some() {
        return Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "Order sequence contains multiple payment intents",
        ));
    }
    let intent = PaymentIntentTerms {
        payment_id: required_string(&event.body, "payment_id")?.to_string(),
        adapter: required_string(&event.body, "adapter")?.to_string(),
        amount: required_string(&event.body, "amount")?.to_string(),
        currency: required_string(&event.body, "currency")?.to_string(),
        capture_policy: required_string(&event.body, "capture_policy")?.to_string(),
    };
    let terms = require_order_terms(context)?;
    if intent.adapter != terms.payment_adapter || intent.capture_policy != terms.capture_policy {
        return Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "payment.intent.created terms do not match order.created",
        ));
    }
    assert_money_parts_equal(
        &terms.amount,
        &terms.currency,
        &intent.amount,
        &intent.currency,
    )?;
    context.payment_intent = Some(intent);
    Ok(())
}

fn validate_payment_capture(
    event: &OrderFlowEvent,
    context: &mut OrderFlowContext,
) -> ValidationResult<()> {
    let payment_id = require_intent_payment_id(event, context)?;
    if context.authorized_payment_id.as_deref() != Some(payment_id.as_str()) {
        return Err(ValidationError::new(
            ValidationCode::InvalidStateTransition,
            "payment.captured must reference an authorized payment",
        ));
    }
    let intent = require_payment_intent(context)?;
    if intent.capture_policy == "after_entitlement" && context.entitlement_id.is_none() {
        return Err(ValidationError::new(
            ValidationCode::InvalidStateTransition,
            "payment.captured requires entitlement.granted first when capture_policy=after_entitlement",
        ));
    }
    let adapter = required_string(&event.body, "adapter")?;
    let amount = required_string(&event.body, "amount")?;
    let currency = required_string(&event.body, "currency")?;
    if adapter != intent.adapter {
        return Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "payment.captured adapter does not match payment.intent.created",
        ));
    }
    assert_money_parts_equal(&intent.amount, &intent.currency, amount, currency)?;
    context.captured_payment_id = Some(payment_id);
    context.captured_amount = Some(amount.to_string());
    context.captured_currency = Some(currency.to_string());
    Ok(())
}

fn validate_entitlement_grant(
    event: &OrderFlowEvent,
    context: &mut OrderFlowContext,
) -> ValidationResult<()> {
    let intent = require_payment_intent(context)?;
    if intent.capture_policy == "before_entitlement" && context.captured_payment_id.is_none() {
        return Err(ValidationError::new(
            ValidationCode::InvalidStateTransition,
            "entitlement.granted requires captured payment when capture_policy=before_entitlement",
        ));
    }
    if let Some(payment_id) = string_field_opt(&event.body, "payment_id") {
        let expected = context
            .captured_payment_id
            .as_deref()
            .unwrap_or(intent.payment_id.as_str());
        if payment_id != expected {
            return Err(ValidationError::new(
                ValidationCode::PaymentTermsMismatch,
                "entitlement.granted references a different payment_id",
            ));
        }
    }
    context.entitlement_id = Some(required_string(&event.body, "entitlement_id")?.to_string());
    Ok(())
}

fn validate_entitlement_lifecycle(
    event: &OrderFlowEvent,
    context: &OrderFlowContext,
) -> ValidationResult<()> {
    let entitlement_id = required_string(&event.body, "entitlement_id")?;
    if context.entitlement_id.as_deref() == Some(entitlement_id) {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Entitlement lifecycle event references a different entitlement_id",
        ))
    }
}

fn validate_dispute_lifecycle(
    event: &OrderFlowEvent,
    context: &OrderFlowContext,
) -> ValidationResult<()> {
    let dispute_id = required_string(&event.body, "dispute_id")?;
    if context.dispute_id.as_deref() == Some(dispute_id) {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::CatalogReferenceMismatch,
            "Dispute lifecycle event references a different dispute_id",
        ))
    }
}

fn require_intent_payment_id(
    event: &OrderFlowEvent,
    context: &OrderFlowContext,
) -> ValidationResult<String> {
    let intent = require_payment_intent(context)?;
    let payment_id = required_string(&event.body, "payment_id")?;
    if payment_id == intent.payment_id {
        Ok(payment_id.to_string())
    } else {
        Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "event references a different payment_id",
        ))
    }
}

fn require_captured_payment_id(
    event: &OrderFlowEvent,
    context: &OrderFlowContext,
) -> ValidationResult<String> {
    let payment_id = required_string(&event.body, "payment_id")?;
    if context.captured_payment_id.as_deref() != Some(payment_id) {
        return Err(ValidationError::new(
            ValidationCode::InvalidStateTransition,
            "refund requires a captured payment",
        ));
    }
    let amount = required_string(&event.body, "amount")?;
    let currency = required_string(&event.body, "currency")?;
    required_string(&event.body, "refund_id")?;
    required_string(&event.body, "provider_ref")?;
    let expected = context.refund_constraint.clone().unwrap_or((
        context.captured_amount.clone().unwrap_or_default(),
        context.captured_currency.clone().unwrap_or_default(),
    ));
    assert_money_parts_equal(&expected.0, &expected.1, amount, currency)?;
    Ok(payment_id.to_string())
}

fn require_refundable_payment_id(
    event: &OrderFlowEvent,
    context: &OrderFlowContext,
) -> ValidationResult<String> {
    let payment_id = required_string(&event.body, "payment_id")?;
    let amount = required_string(&event.body, "amount")?;
    let currency = required_string(&event.body, "currency")?;
    required_string(&event.body, "refund_id")?;
    required_string(&event.body, "provider_ref")?;

    if context.captured_payment_id.as_deref() == Some(payment_id) {
        let expected = context.refund_constraint.clone().unwrap_or((
            context.captured_amount.clone().unwrap_or_default(),
            context.captured_currency.clone().unwrap_or_default(),
        ));
        assert_money_parts_equal(&expected.0, &expected.1, amount, currency)?;
        return Ok(payment_id.to_string());
    }

    if context.authorized_payment_id.as_deref() == Some(payment_id) {
        let intent = require_payment_intent(context)?;
        if intent.payment_id != payment_id {
            return Err(ValidationError::new(
                ValidationCode::PaymentTermsMismatch,
                "refund references a different payment_id",
            ));
        }
        if let Some(expected) = &context.refund_constraint {
            assert_money_parts_equal(&expected.0, &expected.1, amount, currency)?;
        } else {
            assert_money_parts_not_greater(&intent.amount, &intent.currency, amount, currency)?;
        }
        return Ok(payment_id.to_string());
    }

    Err(ValidationError::new(
        ValidationCode::InvalidStateTransition,
        "refund requires an authorized or captured payment",
    ))
}

fn capture_ruling_remedy(
    event: &OrderFlowEvent,
    context: &mut OrderFlowContext,
) -> ValidationResult<()> {
    if string_field_opt(&event.body, "ruling") != Some("partial_refund_required") {
        return Ok(());
    }
    let remedy = event.body.get("remedy").ok_or_else(|| missing("remedy"))?;
    context.refund_constraint = Some((
        required_string(remedy, "amount")?.to_string(),
        required_string(remedy, "currency")?.to_string(),
    ));
    Ok(())
}

fn require_payment_intent(context: &OrderFlowContext) -> ValidationResult<&PaymentIntentTerms> {
    context.payment_intent.as_ref().ok_or_else(|| {
        ValidationError::new(
            ValidationCode::InvalidStateTransition,
            "Payment-bound event requires payment.intent.created first",
        )
    })
}

fn require_order_terms(context: &OrderFlowContext) -> ValidationResult<&OrderTerms> {
    context.order_terms.as_ref().ok_or_else(|| {
        ValidationError::new(
            ValidationCode::InvalidStateTransition,
            "Payment-bound event requires order.created terms first",
        )
    })
}

fn transition(state: OrderState, event_type: &str) -> ValidationResult<OrderState> {
    let next = match (state, event_type) {
        (OrderState::Draft, "io.marketplace.order.created") => OrderState::Created,
        (OrderState::Created, "io.marketplace.order.accepted") => OrderState::Accepted,
        (OrderState::Created, "io.marketplace.order.rejected") => OrderState::Rejected,
        (OrderState::Created, "io.marketplace.order.cancelled") => OrderState::Cancelled,
        (OrderState::Accepted, "io.marketplace.payment.intent.created") => {
            OrderState::PaymentIntentCreated
        }
        (OrderState::Accepted, "io.marketplace.dispute.opened") => {
            OrderState::DisputeOpenedPrePayment
        }
        (OrderState::Accepted, "io.marketplace.order.cancelled") => OrderState::Cancelled,
        (OrderState::PaymentIntentCreated, "io.marketplace.payment.authorized") => {
            OrderState::PaymentAuthorized
        }
        (
            OrderState::PaymentIntentCreated,
            "io.marketplace.payment.failed" | "io.marketplace.payment.cancelled",
        ) => OrderState::Cancelled,
        (OrderState::PaymentAuthorized, "io.marketplace.payment.captured") => {
            OrderState::PaymentCaptured
        }
        (OrderState::PaymentAuthorized, "io.marketplace.entitlement.granted") => {
            OrderState::EntitlementGrantedBeforeCapture
        }
        (OrderState::PaymentAuthorized, "io.marketplace.payment.failed") => OrderState::Cancelled,
        (OrderState::PaymentAuthorized, "io.marketplace.payment.refunded") => OrderState::Refunded,
        (OrderState::PaymentCaptured, "io.marketplace.entitlement.granted") => {
            OrderState::EntitlementGranted
        }
        (OrderState::PaymentCaptured, "io.marketplace.dispute.opened") => {
            OrderState::DisputeOpenedAfterCapture
        }
        (OrderState::PaymentCaptured, "io.marketplace.payment.refund.requested") => {
            OrderState::RefundRequested
        }
        (OrderState::PaymentCaptured, "io.marketplace.payment.refunded") => OrderState::Refunded,
        (OrderState::PaymentCaptured, "io.marketplace.payment.chargeback.opened") => {
            OrderState::ChargebackOpened
        }
        (OrderState::RefundRequested, "io.marketplace.payment.refund.requested") => {
            OrderState::RefundRequested
        }
        (OrderState::RefundRequested, "io.marketplace.payment.refunded") => OrderState::Refunded,
        (OrderState::RefundRequested, "io.marketplace.payment.chargeback.opened") => {
            OrderState::ChargebackOpened
        }
        (OrderState::EntitlementGrantedBeforeCapture, "io.marketplace.payment.captured") => {
            OrderState::EntitlementGranted
        }
        (OrderState::EntitlementGrantedBeforeCapture, "io.marketplace.dispute.opened") => {
            OrderState::DisputeOpenedAfterEntitlement
        }
        (
            OrderState::EntitlementGrantedBeforeCapture,
            "io.marketplace.payment.failed" | "io.marketplace.entitlement.revoked",
        ) => OrderState::Cancelled,
        (
            OrderState::EntitlementGrantedBeforeCapture,
            "io.marketplace.payment.chargeback.opened",
        ) => OrderState::ChargebackOpened,
        (OrderState::EntitlementGranted, "io.marketplace.order.completed") => OrderState::Completed,
        (OrderState::EntitlementGranted, "io.marketplace.dispute.opened") => {
            OrderState::DisputeOpenedAfterEntitlement
        }
        (OrderState::EntitlementGranted, "io.marketplace.payment.refund.requested") => {
            OrderState::RefundRequested
        }
        (OrderState::EntitlementGranted, "io.marketplace.payment.refunded") => OrderState::Refunded,
        (OrderState::EntitlementGranted, "io.marketplace.payment.chargeback.opened") => {
            OrderState::ChargebackOpened
        }
        (OrderState::EntitlementGranted, "io.marketplace.entitlement.activated") => {
            OrderState::EntitlementActivated
        }
        (OrderState::EntitlementGranted, "io.marketplace.entitlement.completed") => {
            OrderState::EntitlementCompleted
        }
        (OrderState::EntitlementGranted, "io.marketplace.entitlement.expired") => {
            OrderState::Expired
        }
        (OrderState::EntitlementGranted, "io.marketplace.entitlement.revoked") => {
            OrderState::Cancelled
        }
        (OrderState::EntitlementActivated, "io.marketplace.order.completed") => {
            OrderState::Completed
        }
        (OrderState::EntitlementActivated, "io.marketplace.dispute.opened") => {
            OrderState::DisputeOpenedAfterEntitlement
        }
        (OrderState::EntitlementActivated, "io.marketplace.payment.refund.requested") => {
            OrderState::RefundRequested
        }
        (OrderState::EntitlementActivated, "io.marketplace.payment.refunded") => {
            OrderState::Refunded
        }
        (OrderState::EntitlementActivated, "io.marketplace.payment.chargeback.opened") => {
            OrderState::ChargebackOpened
        }
        (OrderState::EntitlementActivated, "io.marketplace.entitlement.completed") => {
            OrderState::EntitlementCompleted
        }
        (OrderState::EntitlementActivated, "io.marketplace.entitlement.expired") => {
            OrderState::Expired
        }
        (OrderState::EntitlementActivated, "io.marketplace.entitlement.revoked") => {
            OrderState::Cancelled
        }
        (OrderState::EntitlementCompleted, "io.marketplace.order.completed") => {
            OrderState::Completed
        }
        (OrderState::EntitlementCompleted, "io.marketplace.dispute.opened") => {
            OrderState::DisputeOpenedAfterEntitlement
        }
        (OrderState::EntitlementCompleted, "io.marketplace.payment.refund.requested") => {
            OrderState::RefundRequested
        }
        (OrderState::EntitlementCompleted, "io.marketplace.payment.refunded") => {
            OrderState::Refunded
        }
        (OrderState::EntitlementCompleted, "io.marketplace.payment.chargeback.opened") => {
            OrderState::ChargebackOpened
        }
        (OrderState::EntitlementCompleted, "io.marketplace.entitlement.expired") => {
            OrderState::Expired
        }
        (OrderState::EntitlementCompleted, "io.marketplace.entitlement.revoked") => {
            OrderState::Cancelled
        }
        (OrderState::DisputeOpenedPrePayment, "io.marketplace.dispute.evidence.submitted") => {
            OrderState::DisputeOpenedPrePayment
        }
        (OrderState::DisputeOpenedPrePayment, "io.marketplace.dispute.ruling.issued") => {
            OrderState::RulingIssuedPrePayment
        }
        (OrderState::DisputeOpenedAfterCapture, "io.marketplace.dispute.evidence.submitted") => {
            OrderState::DisputeOpenedAfterCapture
        }
        (OrderState::DisputeOpenedAfterCapture, "io.marketplace.dispute.ruling.issued") => {
            OrderState::RulingIssuedAfterCapture
        }
        (
            OrderState::DisputeOpenedAfterEntitlement,
            "io.marketplace.dispute.evidence.submitted",
        ) => OrderState::DisputeOpenedAfterEntitlement,
        (OrderState::DisputeOpenedAfterEntitlement, "io.marketplace.dispute.ruling.issued") => {
            OrderState::RulingIssuedAfterEntitlement
        }
        (OrderState::RulingIssuedPrePayment, "io.marketplace.dispute.evidence.submitted") => {
            OrderState::RulingIssuedPrePayment
        }
        (OrderState::RulingIssuedPrePayment, "io.marketplace.dispute.closed") => {
            OrderState::DisputeResolved
        }
        (OrderState::RulingIssuedAfterCapture, "io.marketplace.dispute.evidence.submitted") => {
            OrderState::RulingIssuedAfterCapture
        }
        (OrderState::RulingIssuedAfterCapture, "io.marketplace.payment.refund.requested") => {
            OrderState::RefundRequested
        }
        (OrderState::RulingIssuedAfterCapture, "io.marketplace.payment.refunded") => {
            OrderState::Refunded
        }
        (OrderState::RulingIssuedAfterCapture, "io.marketplace.payment.chargeback.opened") => {
            OrderState::ChargebackOpened
        }
        (OrderState::RulingIssuedAfterCapture, "io.marketplace.entitlement.granted") => {
            OrderState::EntitlementGranted
        }
        (OrderState::RulingIssuedAfterCapture, "io.marketplace.dispute.closed") => {
            OrderState::DisputeResolved
        }
        (OrderState::RulingIssuedAfterEntitlement, "io.marketplace.dispute.evidence.submitted") => {
            OrderState::RulingIssuedAfterEntitlement
        }
        (OrderState::RulingIssuedAfterEntitlement, "io.marketplace.payment.refund.requested") => {
            OrderState::RefundRequested
        }
        (OrderState::RulingIssuedAfterEntitlement, "io.marketplace.payment.refunded") => {
            OrderState::Refunded
        }
        (OrderState::RulingIssuedAfterEntitlement, "io.marketplace.payment.chargeback.opened") => {
            OrderState::ChargebackOpened
        }
        (OrderState::RulingIssuedAfterEntitlement, "io.marketplace.dispute.closed") => {
            OrderState::DisputeResolved
        }
        (OrderState::Refunded, "io.marketplace.payment.chargeback.opened") => {
            OrderState::ChargebackOpened
        }
        _ => {
            return Err(ValidationError::with_details(
                ValidationCode::InvalidStateTransition,
                "Invalid order state transition",
                json!({ "state": format!("{state:?}"), "eventType": event_type }),
            ));
        }
    };
    Ok(next)
}

#[derive(Debug, Clone)]
pub struct OrderRoomTimelineContext {
    pub room_id: String,
    pub authorities: OrderAuthorities,
    pub required_members: Vec<String>,
    pub members: Vec<String>,
}

pub fn validate_order_room_timeline(
    events: &[Value],
    context: &OrderRoomTimelineContext,
) -> ValidationResult<()> {
    for required_member in &context.required_members {
        if !context.members.contains(required_member) {
            return Err(ValidationError::with_details(
                ValidationCode::RoomMembershipViolation,
                "Order room is missing required member",
                json!({ "requiredMember": required_member }),
            ));
        }
    }
    let mut flow_events = Vec::new();
    let mut unjoined_representatives = Vec::new();
    let mut seller_accepted = false;
    let mut validation_context = MarketplaceEventValidationContext {
        room_profile: Some(RoomProfile::Order),
        ..Default::default()
    };
    for raw_event in events {
        match validate_marketplace_event(raw_event, &mut validation_context)? {
            MarketplaceEventValidationResult::IgnoredUnknownEventType => {}
            MarketplaceEventValidationResult::Accepted(event) => {
                if event.room_id != context.room_id {
                    return Err(ValidationError::new(
                        ValidationCode::CatalogReferenceMismatch,
                        "Order event was replayed into another room",
                    ));
                }
                assert_event_authority(&event.event_type, &event.sender, &context.authorities)?;
                if event.event_type == "io.marketplace.actor.customer.bound" {
                    for representative in event
                        .content
                        .body
                        .get("authorized_representatives")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        if !context
                            .members
                            .iter()
                            .any(|member| member == representative)
                        {
                            unjoined_representatives.push(representative.to_string());
                        }
                    }
                }
                if event.event_type == "io.marketplace.order.accepted" {
                    seller_accepted = true;
                }
                flow_events.push(OrderFlowEvent {
                    event_type: event.event_type,
                    body: event.content.body,
                });
            }
        }
    }
    if !seller_accepted && !unjoined_representatives.is_empty() {
        return Err(ValidationError::new(
            ValidationCode::RoomMembershipViolation,
            "Customer representative disclosed in customer.bound is not joined to the order room",
        ));
    }
    validate_order_sequence(&flow_events).map(|_| ())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArbitrationPolicy {
    pub policy_id: String,
    pub version: String,
    pub arbitration_window: String,
    pub accepted_remedies: Vec<String>,
    pub binding: bool,
}

pub fn validate_arbitration_policy(policy: &ArbitrationPolicy) -> ValidationResult<()> {
    if policy.policy_id.is_empty()
        || policy.version.is_empty()
        || policy.arbitration_window.is_empty()
        || policy.accepted_remedies.is_empty()
    {
        Err(ValidationError::new(
            ValidationCode::MissingRequiredField,
            "Invalid arbitration policy",
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ArbitrationFlowEvent {
    pub event_type: String,
    pub event_id: String,
    pub room_id: String,
    pub body: Value,
}

pub fn validate_arbitration_flow(events: &[ArbitrationFlowEvent]) -> ValidationResult<()> {
    let event_ids = events
        .iter()
        .map(|event| event.event_id.as_str())
        .collect::<HashSet<_>>();
    let mut binding_refund_required = false;
    let mut refund_executed = false;
    for event in events {
        if event.event_type == "io.marketplace.dispute.ruling.issued" {
            let evidence_refs = event
                .body
                .get("evidence_refs")
                .and_then(Value::as_array)
                .ok_or_else(|| missing("evidence_refs"))?;
            if evidence_refs.iter().any(|reference| {
                !reference
                    .as_str()
                    .is_some_and(|value| value.starts_with('$') && event_ids.contains(value))
            }) {
                return Err(ValidationError::new(
                    ValidationCode::CatalogReferenceMismatch,
                    "Dispute ruling evidence_refs must point to Matrix events in the order room",
                ));
            }
            if event.body.get("binding").and_then(Value::as_bool) == Some(true)
                && event.body.get("ruling").and_then(Value::as_str) == Some("refund_required")
            {
                binding_refund_required = true;
            }
        }
        if matches!(
            event.event_type.as_str(),
            "io.marketplace.payment.refund.requested" | "io.marketplace.payment.refunded"
        ) {
            refund_executed = true;
        }
    }
    if binding_refund_required && !refund_executed {
        Err(ValidationError::new(
            ValidationCode::PolicyViolation,
            "Binding refund ruling requires a refund event",
        ))
    } else {
        Ok(())
    }
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
            payment_id: format!("pay:mock.example:{stable}"),
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
    if ["bearer", "access_token=", "token=", "secret="]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        Err(ValidationError::new(
            ValidationCode::PolicyViolation,
            "Entitlement metadata must not contain secret access credentials",
        ))
    } else {
        Ok(())
    }
}

fn parse_capability(value: &str) -> Option<AllowlistCapability> {
    match value {
        "catalog" => Some(AllowlistCapability::Catalog),
        "orders" => Some(AllowlistCapability::Orders),
        "arbitration" => Some(AllowlistCapability::Arbitration),
        "payments" => Some(AllowlistCapability::Payments),
        "indexing" => Some(AllowlistCapability::Indexing),
        _ => None,
    }
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

fn required_string<'a>(body: &'a Value, field: &str) -> ValidationResult<&'a str> {
    string_field(body, field)
}

fn required_string_array(body: &Value, field: &str) -> ValidationResult<Vec<String>> {
    body.get(field)
        .and_then(Value::as_array)
        .filter(|items| items.iter().all(|item| item.as_str().is_some()))
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .ok_or_else(|| missing(field))
}

fn u64_field(body: &Value, field: &str) -> ValidationResult<u64> {
    body.get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| missing(field))
}

fn missing(field: &str) -> ValidationError {
    ValidationError::with_details(
        ValidationCode::MissingRequiredField,
        format!("Missing required field {field}"),
        json!({ "field": field }),
    )
}

fn snapshot_sequence_rollback() -> ValidationError {
    ValidationError::new(
        ValidationCode::RevisionRollback,
        "Snapshot sequence rollback",
    )
}

fn assert_money_equal(expected: &Money, actual: &Money) -> ValidationResult<()> {
    assert_money_parts_equal(
        &expected.amount,
        &expected.currency,
        &actual.amount,
        &actual.currency,
    )
}

fn assert_money_parts_equal(
    expected_amount: &str,
    expected_currency: &str,
    actual_amount: &str,
    actual_currency: &str,
) -> ValidationResult<()> {
    if expected_currency == actual_currency
        && expected_amount.parse::<f64>().unwrap_or(f64::NAN)
            == actual_amount.parse::<f64>().unwrap_or(f64::NAN)
    {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "Money amount or currency mismatch",
        ))
    }
}

fn assert_money_parts_not_greater(
    max_amount: &str,
    expected_currency: &str,
    actual_amount: &str,
    actual_currency: &str,
) -> ValidationResult<()> {
    let max = max_amount.parse::<f64>().unwrap_or(f64::NAN);
    let actual = actual_amount.parse::<f64>().unwrap_or(f64::NAN);
    if expected_currency == actual_currency && actual > 0.0 && actual <= max {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "Refund amount or currency exceeds refundable payment terms",
        ))
    }
}

fn is_matrix_user_id(value: &str) -> bool {
    value.starts_with('@') && value.split(':').count() == 2
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

pub mod fixtures {
    use super::*;

    pub fn valid_catalog() -> CatalogIndex {
        let mut catalog = CatalogIndex::new("shop.example");
        catalog
            .apply_snapshot(SnapshotRecord {
                snapshot_id: "snap:shop.example:01JSNAP".into(),
                sequence: 1,
                sha256: snapshot_hash(),
                covers_events_until: "$snap".into(),
            })
            .unwrap();
        catalog
            .upsert_seller(SellerRecord {
                seller_id: "seller:shop.example:01JSELLER".into(),
                status: "active".into(),
            })
            .unwrap();
        catalog
            .upsert_product(ProductRecord {
                product_id: "prod:shop.example:01JPROD".into(),
                seller_id: "seller:shop.example:01JSELLER".into(),
                revision: 1,
                terms_hash: Some(
                    "sha256:3333333333333333333333333333333333333333333333333333333333333333"
                        .into(),
                ),
            })
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
                payment_capture_policy: Some("before_entitlement".into()),
                offer_terms_hash: Some(
                    "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                        .into(),
                ),
                seller_terms_hash: Some(
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .into(),
                ),
            })
            .unwrap();
        catalog
    }

    pub fn snapshot_hash() -> String {
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()
    }

    pub fn valid_order_created() -> OrderCreatedBody {
        OrderCreatedBody {
            order_id: "ord:customer.example:01JORDER".into(),
            room_id: "!order:customer.example".into(),
            customer_id: "customer:customer.example:01JCUST".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            offer_id: "offer:shop.example:01JOFFER".into(),
            offer_revision: 3,
            catalog_snapshot_id: "snap:shop.example:01JSNAP".into(),
            quantity: 1,
            price: Money {
                amount: "100.00".into(),
                currency: "USD".into(),
            },
            payment_adapter: "stripe".into(),
            payment_capture_policy: "before_entitlement".into(),
            entitlement_type: "booking_slot".into(),
            seller_terms_hash:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".into(),
            offer_terms_hash:
                "sha256:2222222222222222222222222222222222222222222222222222222222222222".into(),
            arbiter_instance: "arbiter.example".into(),
            arbiter_actor: "arbiter:arbiter.example:01JARB".into(),
            arbitration_policy_id: "standard-digital-v1".into(),
            arbitration_policy_version: "1".into(),
            arbitration_window: "P14D".into(),
            expires_at: "2026-05-04T10:30:00Z".into(),
        }
    }

    pub fn valid_customer() -> CustomerBinding {
        CustomerBinding {
            customer_id: "customer:customer.example:01JCUST".into(),
            status: "active".into(),
            accepted_payment_adapters: vec!["stripe".into()],
            accepted_arbitration_policies: vec!["standard-digital-v1".into()],
        }
    }

    pub fn order_allowlist() -> AllowlistPolicy {
        AllowlistPolicy::new([
            (
                "shop.example".to_string(),
                vec![
                    "orders".to_string(),
                    "catalog".to_string(),
                    "indexing".to_string(),
                ],
            ),
            (
                "arbiter.example".to_string(),
                vec!["arbitration".to_string()],
            ),
        ])
    }

    pub fn valid_order_flow() -> Vec<OrderFlowEvent> {
        vec![
            OrderFlowEvent {
                event_type: "io.marketplace.actor.customer.bound".into(),
                body: json!({
                    "customer_id": "customer:customer.example:01JCUST",
                    "status": "active",
                    "accepted_payment_adapters": ["stripe"],
                    "accepted_arbitration_policies": ["standard-digital-v1"]
                }),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.order.created".into(),
                body: serde_json::to_value(valid_order_created()).unwrap(),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.order.accepted".into(),
                body: json!({
                    "order_id": "ord:customer.example:01JORDER",
                    "offer_revision": 3,
                    "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                    "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                    "payment_capture_policy": "before_entitlement",
                    "arbitration_policy_version": "1"
                }),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.payment.intent.created".into(),
                body: json!({
                    "order_id": "ord:customer.example:01JORDER",
                    "payment_id": "pay:customer.example:01JPAY",
                    "adapter": "stripe",
                    "amount": "100.00",
                    "currency": "USD",
                    "capture_policy": "before_entitlement"
                }),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.payment.authorized".into(),
                body: json!({"order_id": "ord:customer.example:01JORDER", "payment_id": "pay:customer.example:01JPAY"}),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.payment.captured".into(),
                body: json!({"order_id": "ord:customer.example:01JORDER", "payment_id": "pay:customer.example:01JPAY", "adapter": "stripe", "amount": "100.00", "currency": "USD"}),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.entitlement.granted".into(),
                body: json!({"order_id": "ord:customer.example:01JORDER", "payment_id": "pay:customer.example:01JPAY", "entitlement_id": "ent:customer.example:01JENT"}),
            },
            OrderFlowEvent {
                event_type: "io.marketplace.order.completed".into(),
                body: json!({"order_id": "ord:customer.example:01JORDER"}),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escrow_refund_after_authorization_is_valid() {
        let mut flow = fixtures::valid_order_flow();
        flow.truncate(5);
        flow.push(OrderFlowEvent {
            event_type: "io.marketplace.payment.refunded".into(),
            body: json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "refund_id": "refund:evm.local:01JREFUND",
                "amount": "100.00",
                "currency": "USD",
                "provider_ref": "evm_escrow:0xabc",
            }),
        });

        let decision = validate_order_sequence(&flow).unwrap();

        assert_eq!(decision.final_state, OrderState::Refunded);
    }

    #[test]
    fn escrow_partial_refund_after_authorization_is_valid() {
        let mut flow = fixtures::valid_order_flow();
        flow.truncate(5);
        flow.push(OrderFlowEvent {
            event_type: "io.marketplace.payment.refunded".into(),
            body: json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "refund_id": "refund:evm.local:01JPARTIAL",
                "amount": "40.00",
                "currency": "USD",
                "provider_ref": "evm_escrow:0xdef",
            }),
        });

        let decision = validate_order_sequence(&flow).unwrap();

        assert_eq!(decision.final_state, OrderState::Refunded);
    }

    #[test]
    fn private_catalog_and_allowlist_edges_are_exercised() {
        let mut catalog = CatalogIndex::new("shop.example");
        catalog
            .apply_snapshot(SnapshotRecord {
                snapshot_id: "snap:shop.example:01JSNAP".into(),
                sequence: 2,
                sha256: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .into(),
                covers_events_until: "$snap".into(),
            })
            .unwrap();
        assert_eq!(
            catalog
                .apply_snapshot(SnapshotRecord {
                    snapshot_id: "snap:shop.example:01JOLD".into(),
                    sequence: 1,
                    sha256:
                        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                            .into(),
                    covers_events_until: "$old".into(),
                })
                .unwrap_err()
                .code,
            ValidationCode::RevisionRollback
        );
        let mut tombstone_snapshot = CatalogSnapshotDocument {
            snapshot: SnapshotRecord {
                snapshot_id: "snap:shop.example:01JTOMB".into(),
                sequence: 1,
                sha256: String::new(),
                covers_events_until: "$snap".into(),
            },
            sellers: vec![SellerRecord {
                seller_id: "seller:shop.example:01JSELLER".into(),
                status: "active".into(),
            }],
            products: vec![ProductRecord {
                product_id: "prod:shop.example:01JPROD".into(),
                seller_id: "seller:shop.example:01JSELLER".into(),
                revision: 1,
                terms_hash: None,
            }],
            offers: vec![],
            tombstones: vec!["prod:shop.example:01JPROD".into()],
            sequence: 1,
            covers_events_until: "$snap".into(),
        };
        let canonical = json!({
            "sellers": [{"seller_id": "seller:shop.example:01JSELLER", "status": "active"}],
            "products": [{"product_id": "prod:shop.example:01JPROD", "seller_id": "seller:shop.example:01JSELLER", "revision": 1, "terms_hash": null}],
            "offers": [],
            "tombstones": ["prod:shop.example:01JPROD"],
            "sequence": 1,
            "covers_events_until": "$snap"
        });
        tombstone_snapshot.snapshot.sha256 =
            morpheus_protocol::sha256_canonical(&canonical).unwrap();
        let replayed = replay_catalog_timeline("shop.example", tombstone_snapshot, &[]).unwrap();
        assert_eq!(replayed.products.len(), 0);

        let mut seller_delta_catalog = CatalogIndex::new("shop.example");
        apply_catalog_delta(
            &mut seller_delta_catalog,
            &CatalogDeltaEvent {
                event_type: "io.marketplace.actor.seller.announced".into(),
                event_id: "$seller".into(),
                catalog_sequence: 1,
                body: json!({"seller_id": "seller:shop.example:01JSELLER", "status": "active"}),
            },
        )
        .unwrap();
        assert!(
            seller_delta_catalog
                .sellers
                .contains_key("seller:shop.example:01JSELLER")
        );

        let mut mismatch_catalog = CatalogIndex::new("shop.example");
        mismatch_catalog
            .upsert_seller(SellerRecord {
                seller_id: "seller:shop.example:01JSELLER".into(),
                status: "active".into(),
            })
            .unwrap();
        mismatch_catalog
            .upsert_seller(SellerRecord {
                seller_id: "seller:shop.example:01JOTHER".into(),
                status: "active".into(),
            })
            .unwrap();
        mismatch_catalog
            .upsert_product(ProductRecord {
                product_id: "prod:shop.example:01JPROD".into(),
                seller_id: "seller:shop.example:01JSELLER".into(),
                revision: 1,
                terms_hash: None,
            })
            .unwrap();
        assert_eq!(
            mismatch_catalog
                .upsert_offer(OfferRecord {
                    offer_id: "offer:shop.example:01JOFFER".into(),
                    product_id: "prod:shop.example:01JPROD".into(),
                    seller_id: "seller:shop.example:01JOTHER".into(),
                    revision: 1,
                    price: Money {
                        amount: "10.00".into(),
                        currency: "USD".into()
                    },
                    entitlement_type: "external_entitlement".into(),
                    payment_capture_policy: None,
                    offer_terms_hash: None,
                    seller_terms_hash: None,
                })
                .unwrap_err()
                .code,
            ValidationCode::CatalogReferenceMismatch
        );
        let mut hidden_catalog = fixtures::valid_catalog();
        hidden_catalog
            .sellers
            .get_mut("seller:shop.example:01JSELLER")
            .unwrap()
            .status = "suspended".into();
        assert!(
            hidden_catalog
                .get_offer("offer:shop.example:01JOFFER")
                .is_none()
        );
        let mut inconsistent_catalog = fixtures::valid_catalog();
        inconsistent_catalog
            .products
            .get_mut("prod:shop.example:01JPROD")
            .unwrap()
            .seller_id = "seller:shop.example:01JOTHER".into();
        assert!(
            inconsistent_catalog
                .get_offer("offer:shop.example:01JOFFER")
                .is_none()
        );
        assert_eq!(
            catalog
                .assert_catalog_instance("seller_id", "seller:other.example:01JSELLER")
                .unwrap_err()
                .code,
            ValidationCode::CatalogReferenceMismatch
        );
        assert!(parse_capability("payments").is_some());
        assert!(parse_capability("unknown").is_none());
        let payment_policy =
            AllowlistPolicy::new([("pay.example".into(), vec!["payments".into()])]);
        assert!(payment_policy.can("pay.example", "payments"));
        assert_eq!(missing("field").code, ValidationCode::MissingRequiredField);
    }

    #[test]
    fn private_order_flow_edges_are_exercised() {
        let authorities = OrderAuthorities {
            seller_as_user: "@seller:shop.example".into(),
            customer_as_user: "@customer:customer.example".into(),
            arbiter_as_user: "@arbiter:arbiter.example".into(),
            payment_as_users: vec!["@payment:shop.example".into()],
        };
        assert_eq!(
            assert_sender_in(
                "@other:shop.example",
                &[(&authorities.seller_as_user, "seller")]
            )
            .unwrap_err()
            .code,
            ValidationCode::UnauthorizedSender
        );

        let mut context = OrderFlowContext {
            order_id: Some("ord:customer.example:01JORDER".into()),
            customer_binding: Some(CustomerBinding {
                customer_id: "customer:customer.example:01JCUST".into(),
                status: "active".into(),
                accepted_payment_adapters: vec!["mock".into()],
                accepted_arbitration_policies: vec!["standard".into()],
            }),
            order_terms: Some(OrderTerms {
                payment_adapter: "mock".into(),
                capture_policy: "before_entitlement".into(),
                amount: "100.00".into(),
                currency: "USD".into(),
                offer_revision: 1,
                seller_terms_hash:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                offer_terms_hash:
                    "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                arbitration_policy_version: "1".into(),
            }),
            payment_intent: Some(PaymentIntentTerms {
                payment_id: "pay:customer.example:01JPAY".into(),
                adapter: "mock".into(),
                amount: "100.00".into(),
                currency: "USD".into(),
                capture_policy: "before_entitlement".into(),
            }),
            authorized_payment_id: Some("pay:customer.example:01JPAY".into()),
            captured_payment_id: Some("pay:customer.example:01JPAY".into()),
            captured_amount: Some("100.00".into()),
            captured_currency: Some("USD".into()),
            entitlement_id: Some("ent:customer.example:01JENT".into()),
            dispute_id: Some("disp:arbiter.example:01JDISP".into()),
            refund_constraint: None,
        };

        assert_eq!(
            validate_order_accepted(
                &OrderFlowEvent {
                    event_type: "io.marketplace.order.accepted".into(),
                    body: json!({
                        "order_id": "ord:customer.example:01JORDER",
                        "offer_revision": 2,
                        "seller_terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "offer_terms_hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "payment_capture_policy": "before_entitlement",
                        "arbitration_policy_version": "1"
                    }),
                },
                &context,
            )
            .unwrap_err()
            .code,
            ValidationCode::PaymentTermsMismatch
        );
        assert!(
            validate_customer_bound(
                &OrderFlowEvent {
                    event_type: "io.marketplace.actor.customer.bound".into(),
                    body: json!({
                        "customer_id": "customer:customer.example:01JCUST",
                        "status": "active",
                        "accepted_payment_adapters": ["mock"],
                        "accepted_arbitration_policies": ["standard"]
                    }),
                },
                &mut OrderFlowContext::default(),
            )
            .is_ok()
        );
        let mut customer_mismatch_context = context.clone();
        assert_eq!(
            validate_created_order_terms(
                &OrderFlowEvent {
                    event_type: "io.marketplace.order.created".into(),
                    body: json!({
                        "customer_id": "customer:customer.example:01JOTHER",
                        "payment_adapter": "mock",
                        "payment_capture_policy": "before_entitlement",
                        "arbitration_policy_id": "standard",
                        "price": {"amount": "100.00", "currency": "USD"},
                        "offer_revision": 1,
                        "seller_terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "offer_terms_hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "arbitration_policy_version": "1"
                    }),
                },
                &mut customer_mismatch_context,
            )
            .unwrap_err()
            .code,
            ValidationCode::CatalogReferenceMismatch
        );
        assert!(validate_event_references(
            &OrderFlowEvent {
                event_type: "io.marketplace.payment.failed".into(),
                body: json!({"order_id": "ord:customer.example:01JORDER", "payment_id": "pay:customer.example:01JPAY"}),
            },
            &mut context,
        )
        .is_ok());

        assert_eq!(
            require_intent_payment_id(
                &OrderFlowEvent {
                    event_type: "io.marketplace.payment.authorized".into(),
                    body: json!({"payment_id": "pay:customer.example:01JOTHER"}),
                },
                &context,
            )
            .unwrap_err()
            .code,
            ValidationCode::PaymentTermsMismatch
        );
        assert_eq!(
            validate_entitlement_lifecycle(
                &OrderFlowEvent {
                    event_type: "io.marketplace.entitlement.activated".into(),
                    body: json!({"entitlement_id": "ent:customer.example:01JOTHER"}),
                },
                &context,
            )
            .unwrap_err()
            .code,
            ValidationCode::CatalogReferenceMismatch
        );
        assert!(
            validate_entitlement_lifecycle(
                &OrderFlowEvent {
                    event_type: "io.marketplace.entitlement.activated".into(),
                    body: json!({"entitlement_id": "ent:customer.example:01JENT"}),
                },
                &context,
            )
            .is_ok()
        );
        assert!(
            validate_entitlement_grant(
                &OrderFlowEvent {
                    event_type: "io.marketplace.entitlement.granted".into(),
                    body: json!({"entitlement_id": "ent:customer.example:01JENT"}),
                },
                &mut context,
            )
            .is_ok()
        );
        assert!(
            capture_ruling_remedy(
                &OrderFlowEvent {
                    event_type: "io.marketplace.dispute.ruling.issued".into(),
                    body: json!({"ruling": "refund_required"}),
                },
                &mut context,
            )
            .is_ok()
        );

        let empty = OrderFlowContext::default();
        assert!(matches!(
            require_payment_intent(&empty),
            Err(ValidationError {
                code: ValidationCode::InvalidStateTransition,
                ..
            })
        ));
        assert!(matches!(
            require_order_terms(&empty),
            Err(ValidationError {
                code: ValidationCode::InvalidStateTransition,
                ..
            })
        ));
    }
}
