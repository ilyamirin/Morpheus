use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub code: String,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedEventsResponse {
    pub status: String,
    pub room_id: String,
    pub event_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionSummaryResponse {
    pub catalog: CatalogSummary,
    pub orders: usize,
    pub payments: usize,
    pub entitlements: usize,
    pub disputes: usize,
    pub arbitration_rulings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSummary {
    pub sellers: usize,
    pub products: usize,
    pub offers: usize,
    pub tombstones: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsResponse {
    pub events: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogListResponse {
    pub items: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdersResponse {
    pub orders: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SellerAnnounceRequest {
    pub seller_id: String,
    pub display_name: String,
    pub legal_profile_ref: String,
    pub terms_ref: String,
    pub terms_hash: String,
    pub supported_payment_adapters: Vec<String>,
    pub supported_entitlement_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductUpsertRequest {
    pub seller_id: String,
    pub product_id: String,
    pub revision: i64,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub terms_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferUpsertRequest {
    pub seller_id: String,
    pub product_id: String,
    pub offer_id: String,
    pub revision: i64,
    pub price: Value,
    pub payment_capture_policy: String,
    pub seller_terms_hash: String,
    pub offer_terms_hash: String,
    pub entitlement_type: String,
    pub availability_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferWithdrawRequest {
    pub seller_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyerOrderCreateRequest {
    pub customer_id: String,
    pub customer_display_name: String,
    pub order_id: String,
    pub room_id: String,
    pub seller_id: String,
    pub offer_id: String,
    pub offer_revision: i64,
    pub catalog_snapshot_id: String,
    pub price: Value,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderActionRequest {
    pub actor_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderAcceptRequest {
    pub actor_id: String,
    pub offer_revision: i64,
    pub seller_terms_hash: String,
    pub offer_terms_hash: String,
    pub payment_capture_policy: String,
    pub arbitration_policy_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentIntentRequest {
    pub actor_id: String,
    pub payment_id: String,
    pub adapter: String,
    pub amount: String,
    pub currency: String,
    pub capture_policy: String,
    pub idempotency_key: String,
    pub provider_ref: String,
    pub confirmation: Value,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentCaptureRequest {
    pub actor_id: String,
    pub payment_id: String,
    pub adapter: String,
    pub amount: String,
    pub currency: String,
    pub provider_ref: String,
    pub evidence: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitlementGrantRequest {
    pub actor_id: String,
    pub payment_id: String,
    pub entitlement_id: String,
    pub entitlement_type: String,
    pub external_ref: String,
    pub evidence: Value,
}
