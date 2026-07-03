use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use morpheus_config::{EvmEscrowConfig, EvmEscrowTokenConfig};
use morpheus_protocol::ValidationError;
use morpheus_server::{
    MatrixPublisher, RemoteCatalogSource, ServerConfig, SynapseMatrixPublisher, build_router,
    build_router_with_publisher, sync_remote_catalog_once,
};
use morpheus_store::{
    CatalogOfferProjectionRecord, EventStore, InMemoryEventStore, OrderProjectionRecord,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[derive(Clone)]
struct SubmittedOnlyPublisher;

#[async_trait]
impl MatrixPublisher for SubmittedOnlyPublisher {
    async fn publish(&self, events: Vec<Value>) -> Result<Vec<Value>, ValidationError> {
        Ok(events)
    }

    fn ingest_after_publish(&self) -> bool {
        false
    }
}

#[derive(Clone, Default)]
struct RecordingPublisher {
    joined_rooms: Arc<Mutex<Vec<String>>>,
    published_events: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl MatrixPublisher for RecordingPublisher {
    async fn publish(&self, events: Vec<Value>) -> Result<Vec<Value>, ValidationError> {
        self.published_events.lock().unwrap().extend(events.clone());
        Ok(events)
    }

    async fn ensure_room_joined(&self, room_id: &str) -> Result<(), ValidationError> {
        self.joined_rooms.lock().unwrap().push(room_id.to_string());
        Ok(())
    }

    fn ingest_after_publish(&self) -> bool {
        false
    }
}

fn server_config() -> ServerConfig {
    ServerConfig {
        instance_id: "shop.example".into(),
        matrix_server_name: "shop.example".into(),
        catalog_room_id: "!catalog:shop.example".into(),
        catalog_room_alias: Some("#marketplace-catalog:shop.example".into()),
        order_room_alias_prefix: Some("#marketplace-order-".into()),
        appservice_sender_localpart: "market".into(),
        homeserver_token: "hs-token".into(),
        admin_token: "admin-token".into(),
        seller_token: "seller-token".into(),
        buyer_token: "buyer-token".into(),
        evm_escrow: Some(EvmEscrowConfig {
            enabled: true,
            chain_id: 31337,
            rpc_url_env: "EVM_RPC_URL".into(),
            escrow_contract: "0x0000000000000000000000000000000000000001".into(),
            default_token: "0x0000000000000000000000000000000000000002".into(),
            confirmations: 1,
            poll_interval_secs: 1,
            start_block: Some(0),
            max_scan_blocks: Some(100),
            rescan_depth: Some(3),
            deployments_path: None,
            tokens: vec![EvmEscrowTokenConfig {
                symbol: "USDC".into(),
                contract: "0x0000000000000000000000000000000000000002".into(),
                decimals: 6,
                currency: "USDC".into(),
            }],
        }),
    }
}

async fn send_admin_request(
    store: InMemoryEventStore,
    method: &str,
    uri: &str,
    authorization: Option<&str>,
) -> (StatusCode, Value) {
    send_admin_request_with_config(server_config(), store, method, uri, authorization).await
}

async fn send_admin_request_with_config(
    config: ServerConfig,
    store: InMemoryEventStore,
    method: &str,
    uri: &str,
    authorization: Option<&str>,
) -> (StatusCode, Value) {
    let app = build_router(config, store);
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(authorization) = authorization {
        request = request.header("authorization", authorization);
    }
    let response = app
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, body)
}

async fn send_json_request(
    store: InMemoryEventStore,
    method: &str,
    uri: &str,
    authorization: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let app = build_router(server_config(), store);
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(authorization) = authorization {
        request = request.header("authorization", authorization);
    }
    let response = app
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, body)
}

async fn send_ui_request(uri: &str) -> (StatusCode, Option<String>) {
    let app = build_router(server_config(), InMemoryEventStore::default());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let _ = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    (status, content_type)
}

async fn send_ui_body_request(uri: &str) -> (StatusCode, Option<String>, String) {
    let app = build_router(server_config(), InMemoryEventStore::default());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    (status, content_type, body)
}

async fn start_remote_catalog_server(
    sellers_status: StatusCode,
    products_status: StatusCode,
    offers_status: StatusCode,
) -> String {
    let app = axum::Router::new()
        .route(
            "/api/v1/catalog/sellers",
            axum::routing::get(move || async move {
                (
                    sellers_status,
                    axum::Json(json!({
                        "items": [{
                            "seller_id": "seller:remote.example:01JSELLER",
                            "issuer_instance": "remote.example",
                            "status": "active",
                            "body": { "display_name": "Remote Seller" }
                        }],
                        "status": "live"
                    })),
                )
            }),
        )
        .route(
            "/api/v1/catalog/products",
            axum::routing::get(move || async move {
                (
                    products_status,
                    axum::Json(json!({
                        "items": [{
                            "product_id": "prod:remote.example:01JPROD",
                            "seller_id": "seller:remote.example:01JSELLER",
                            "revision": 1,
                            "body": { "title": "Remote Product" }
                        }],
                        "status": "live"
                    })),
                )
            }),
        )
        .route(
            "/api/v1/catalog/offers",
            axum::routing::get(move || async move {
                (
                    offers_status,
                    axum::Json(json!({
                        "items": [{
                            "offer_id": "offer:remote.example:01JOFFER",
                            "product_id": "prod:remote.example:01JPROD",
                            "seller_id": "seller:remote.example:01JSELLER",
                            "revision": 1,
                            "price": { "amount": "10.00", "currency": "USD" },
                            "inventory_kind": "unlimited",
                            "body": { "revision": 1 }
                        }],
                        "status": "live",
                        "code": "REMOTE_CATALOG_UNAVAILABLE",
                        "error": "remote catalog temporarily unavailable"
                    })),
                )
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

async fn start_remote_catalog_server_without_offers() -> String {
    let app = axum::Router::new()
        .route(
            "/api/v1/catalog/sellers",
            axum::routing::get(|| async move {
                axum::Json(json!({
                    "items": [{
                        "seller_id": "seller:remote.example:01JSELLER",
                        "issuer_instance": "remote.example",
                        "status": "active",
                        "body": { "display_name": "Remote Seller" }
                    }],
                    "status": "live"
                }))
            }),
        )
        .route(
            "/api/v1/catalog/products",
            axum::routing::get(|| async move {
                axum::Json(json!({
                    "items": [{
                        "product_id": "prod:remote.example:01JPROD",
                        "seller_id": "seller:remote.example:01JSELLER",
                        "revision": 1,
                        "body": { "title": "Remote Product" }
                    }],
                    "status": "live"
                }))
            }),
        )
        .route(
            "/api/v1/catalog/offers",
            axum::routing::get(|| async move {
                axum::Json(json!({
                    "items": [],
                    "status": "live"
                }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn assert_contains_all(body: &str, expected: &[&str]) {
    for text in expected {
        assert!(body.contains(text), "missing {text:?}");
    }
}

fn assert_contains_none(body: &str, unexpected: &[&str]) {
    for text in unexpected {
        assert!(!body.contains(text), "unexpected {text:?}");
    }
}

async fn store_with_admin_projection_data() -> InMemoryEventStore {
    let store = InMemoryEventStore::default();
    store
        .upsert_catalog_seller(
            "seller:shop.example:01JSELLER",
            "shop.example",
            "active",
            json!({ "status": "active" }),
        )
        .await
        .unwrap();
    store
        .upsert_catalog_product(
            "prod:shop.example:01JPROD",
            "seller:shop.example:01JSELLER",
            1,
            json!({ "revision": 1 }),
        )
        .await
        .unwrap();
    store
        .upsert_catalog_offer(CatalogOfferProjectionRecord {
            offer_id: "offer:shop.example:01JOFFER".into(),
            product_id: "prod:shop.example:01JPROD".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            revision: 1,
            price: json!({ "amount": "100.00", "currency": "USD" }),
            inventory_kind: "booking_slot".into(),
            body: json!({
                "revision": 1,
                "payment_terms": {"capture_policy": "before_entitlement"},
                "entitlement": {"type": "external_entitlement"},
                "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            }),
        })
        .await
        .unwrap();
    store
        .tombstone_catalog_object(
            "offer:shop.example:01JOLD",
            "offer",
            json!({ "reason": "removed" }),
        )
        .await
        .unwrap();
    store
        .upsert_order(OrderProjectionRecord {
            order_id: "ord:customer.example:01JORDER".into(),
            room_id: "!order:customer.example".into(),
            customer_id: "customer:customer.example:01JCUST".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            offer_id: "offer:shop.example:01JOFFER".into(),
            status: "created".into(),
            body: json!({ "order_id": "ord:customer.example:01JORDER" }),
        })
        .await
        .unwrap();
    store
        .record_order_event(
            "ord:customer.example:01JORDER",
            "evt:shop.example:01JMARKET3",
            "io.marketplace.order.created",
            json!({ "status": "created" }),
        )
        .await
        .unwrap();
    store
}

fn fixture_buyer_order_request(order_id: &str, offer_id: &str) -> Value {
    json!({
        "customer_id": "customer:shop.example:01JCUST",
        "customer_display_name": "Fixture Customer",
        "order_id": order_id,
        "seller_id": "seller:shop.example:01JSELLER",
        "offer_id": offer_id,
        "offer_revision": 1,
        "catalog_snapshot_id": "snap:shop.example:01JSNAP",
        "price": {"amount": "100.00", "currency": "USD"},
        "payment_adapter": "mock",
        "payment_capture_policy": "before_entitlement",
        "entitlement_type": "external_entitlement",
        "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        "arbiter_instance": "cases.example",
        "arbiter_actor": "arbiter:cases.example:01JARBITER",
        "arbitration_policy_id": "standard-digital-v1",
        "arbitration_policy_version": "1",
        "arbitration_window": "P14D",
        "expires_at": "2026-05-06T10:30:00Z"
    })
}

async fn seed_mock_order_created(store: InMemoryEventStore, order_id: &str) -> InMemoryEventStore {
    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/buyer/orders",
        Some("Bearer buyer-token"),
        fixture_buyer_order_request(order_id, "offer:shop.example:01JOFFER"),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    store
}

async fn seed_mock_order_with_payment_intent(
    store: InMemoryEventStore,
    order_id: &str,
    payment_id: &str,
) -> InMemoryEventStore {
    let store = seed_mock_order_created(store, order_id).await;

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        &format!("/api/v1/seller/orders/{order_id}/accept"),
        Some("Bearer seller-token"),
        json!({
            "actor_id": "seller:shop.example:01JSELLER",
            "offer_revision": 1,
            "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "payment_capture_policy": "before_entitlement",
            "arbitration_policy_version": "1"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        &format!("/api/v1/seller/orders/{order_id}/payment-intent"),
        Some("Bearer seller-token"),
        json!({
            "actor_id": "seller:shop.example:01JSELLER",
            "payment_id": payment_id,
            "adapter": "mock",
            "amount": "100.00",
            "currency": "USD",
            "capture_policy": "before_entitlement",
            "idempotency_key": format!("idem:{payment_id}"),
            "provider_ref": format!("mock:{payment_id}"),
            "confirmation": {"method": "redirect", "uri": "https://shop.example/pay/confirm"},
            "expires_at": "2026-05-06T10:30:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    store
}

#[tokio::test]
async fn ui_html_routes_return_ok_without_auth() {
    for uri in ["/ui/admin", "/ui/seller", "/ui/buyer"] {
        let (status, _) = send_ui_request(uri).await;

        assert_eq!(status, StatusCode::OK, "{uri}");
    }
}

#[tokio::test]
async fn admin_ui_uses_auto_refresh_instead_of_per_metric_refresh_buttons() {
    let (status, content_type, body) = send_ui_body_request("/ui/admin").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/html; charset=utf-8"));
    assert_contains_all(
        &body,
        &[
            "Operator status",
            "Monitor",
            "Incidents",
            "Policy",
            "Maintenance",
            "Debug",
            "Admin settings",
            "Overall status",
            "Last refresh",
            "Needs attention",
            "Catalog health",
            "Settlement",
            "Risk",
            "Incident queue",
            "Suggested action",
            "Copy event id",
            "Trusted instances",
            "capability-chip",
            "Maintenance actions",
            "Confirm maintenance action",
            "Debug console",
            "Open debug",
            "Auto refresh pending",
            r#"id="admin-overall-status""#,
            r#"id="admin-error-count""#,
            r#"id="admin-incident-list""#,
            r#"id="admin-policy-cards""#,
            r#"id="admin-debug-panel""#,
            r#"data-admin-settings-toggle"#,
            r#"data-admin-settings-panel"#,
            r#"data-maintenance-confirm"#,
            r#"data-admin-debug-toggle"#,
            r#"data-text="admin-auto-refresh""#,
            r#"data-action="admin-rebuild""#,
            r#"data-form="admin-replay""#,
        ],
    );
    assert_contains_none(
        &body,
        &[
            r#"data-refresh="admin""#,
            r#"data-action="admin-health""#,
            r#"data-action="admin-ready""#,
            r#"data-action="admin-config""#,
            r#"data-action="admin-allowlist""#,
            r#"data-action="admin-summary""#,
            r#"data-action="admin-events""#,
            "Admin bearer token</span>\n          <input",
        ],
    );
}

#[tokio::test]
async fn seller_ui_contains_storefront_anchors_and_hooks() {
    let (status, content_type, body) = send_ui_body_request("/ui/seller").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/html; charset=utf-8"));
    assert_contains_all(
        &body,
        &[
            "My Store",
            "Quick Add",
            "Add listing",
            "Seller settings",
            "seller-sync-status",
            "Store summary",
            "Published listings",
            "Orders needing action",
            "Publish listing also activates the seller and saves the product.",
            "Product image",
            "Image upload only attaches a cover.",
            r#"placeholder="e.g. Custom Cover Book""#,
            r#"placeholder="books""#,
            r#"placeholder="19.90""#,
            r#"data-product-image-input"#,
            r#"data-product-image-preview"#,
            r#"data-product-image-clear"#,
            "Demo preview",
            "Preview only",
            "No orders need seller action",
            "Back to store",
            "Store",
            "Orders",
            "Advanced",
            "advanced-panel",
            "seller-settings-panel",
            "seller-quick-add-panel",
            "seller-product-card",
            r#"data-page="seller""#,
            r#"data-token="seller""#,
            r#"data-seller-settings-toggle"#,
            r#"data-seller-settings-panel"#,
            r#"data-seller-quick-add-toggle"#,
            r#"data-seller-quick-add-panel"#,
            r#"data-seller-quick-add-overlay"#,
            r#"data-form="seller-announce""#,
            r#"data-form="seller-product""#,
            r#"data-form="seller-offer""#,
            r#"data-action="seller-orders""#,
            r#"id="seller-orders-rows""#,
            r#"id="seller-order-count""#,
            r#"id="result-panel""#,
            r#"id="morpheus-ui-config""#,
            r#""instance_id":"shop.example""#,
        ],
    );
    assert_contains_none(
        &body,
        &[
            "Profile -&gt; Product -&gt; Offer -&gt; Publish",
            "Save product",
            r#"data-form="seller-order-action""#,
            "seller-order-action-card",
            r#"name="title" value="Soft Runner""#,
            r#"name="amount" value="100.00""#,
            "Activate seller</button>",
        ],
    );
}

#[tokio::test]
async fn buyer_ui_contains_gallery_checkout_anchors_and_hooks() {
    let (status, content_type, body) = send_ui_body_request("/ui/buyer").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/html; charset=utf-8"));
    assert_contains_all(
        &body,
        &[
            "Marketplace",
            "Gallery",
            "Buy",
            "checkout-sheet",
            "buyer-sync-status",
            "buyer-settings-panel",
            r#"data-token-settings-toggle"#,
            r#"data-token-settings-panel"#,
            "Debug tools",
            "Demo preview",
            "Load catalog to buy",
            "Live projected offer",
            "Browse catalog",
            "Discover",
            "Orders",
            "Advanced",
            "advanced-panel",
            r#"data-page="buyer""#,
            r#"data-token="buyer""#,
            r#"data-form="buyer-create-order""#,
            r#"data-form="buyer-order-tools""#,
            r#"data-action="buyer-catalog""#,
            r#"data-action="buyer-orders""#,
            r#"id="buyer-sellers""#,
            r#"id="buyer-products""#,
            r#"id="buyer-offers""#,
            r#"id="buyer-orders-rows""#,
            r#"id="buyer-order-count""#,
            r#"id="result-panel""#,
            r#"id="morpheus-ui-config""#,
            r#""instance_id":"shop.example""#,
        ],
    );
    assert_contains_none(&body, &["Selected offer", r#"data-demo-buy"#]);
}

#[tokio::test]
async fn buyer_ui_contains_evm_escrow_wallet_hooks() {
    let (status, content_type, body) = send_ui_body_request("/ui/buyer").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/html; charset=utf-8"));
    assert_contains_all(&body, &["app.bundle.js", r#"data-page="buyer""#]);
}

#[tokio::test]
async fn app_js_contains_evm_escrow_hooks() {
    let (status, content_type, body) = send_ui_body_request("/ui/assets/app.js").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type
            .as_deref()
            .is_some_and(|value| value.contains("javascript")),
        "{content_type:?}"
    );
    assert_contains_all(
        &body,
        &[
            "evm_escrow",
            "requestEvmEscrowDeposit",
            "approve",
            "deposit",
            "data-evm-address",
            "wallet_plan_ready",
        ],
    );
    assert_contains_none(
        &body,
        &[
            r#"buyer_evm_address: demoEvmAddress"#,
            r#"seller_evm_address: demoEvmAddress"#,
            r#"arbiter_evm_address: demoEvmAddress"#,
        ],
    );
}

#[tokio::test]
async fn ui_bundle_asset_returns_javascript_without_auth() {
    let (status, content_type, body) = send_ui_body_request("/ui/assets/app.bundle.js").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type
            .as_deref()
            .is_some_and(|value| value.contains("javascript")),
        "{content_type:?}"
    );
    assert_contains_all(&body, &["viem", "writeContract", "evm_escrow"]);
}

#[tokio::test]
async fn ui_javascript_does_not_ship_shop_example_as_runtime_default() {
    let (status, content_type, body) = send_ui_body_request("/ui/assets/app.js").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type
            .as_deref()
            .is_some_and(|value| value.contains("javascript")),
        "{content_type:?}"
    );
    assert_contains_none(&body, &["shop.example"]);
}

#[tokio::test]
async fn ui_javascript_tracks_projection_pending_buyer_orders() {
    let (status, content_type, body) = send_ui_body_request("/ui/assets/app.js").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type
            .as_deref()
            .is_some_and(|value| value.contains("javascript")),
        "{content_type:?}"
    );
    assert_contains_all(
        &body,
        &[
            "pendingOrders",
            "markBuyerOrderPending",
            "initBuyerSettings",
            "setBuyerSettingsOpen",
            "setBuyerSyncStatus",
            "isLiveProjectedOffer",
            "Live offer unavailable",
            "Browse catalog",
            "Order submitted. Confirmation may take a few seconds.",
            "Projection pending",
            "Refresh orders",
            "projection_timeout",
            "data-seller-offer-withdraw",
            "publishSellerListing",
            "resetSellerDraftIds",
            "compressProductImage",
            "primaryMediaImage",
            "validateSellerListing",
            "LISTING_FORM_INCOMPLETE",
            "function silentGet",
            "silent: true",
            "const publishOptions = { silent: true, result: false }",
            "Publish listing",
        ],
    );
    assert_contains_none(
        &body,
        &[
            r#"api("/api/v1/catalog/sellers", { action: "GET /api/v1/catalog/sellers" })"#,
            r#"api("/api/v1/catalog/products", { action: "GET /api/v1/catalog/products" })"#,
            r#"api("/api/v1/catalog/offers", { action: "GET /api/v1/catalog/offers" })"#,
            r#"api(path, { tokenRole: role, action: `GET ${path}` })"#,
        ],
    );
}

#[tokio::test]
async fn ui_javascript_renders_status_aware_seller_order_actions() {
    let (status, content_type, body) = send_ui_body_request("/ui/assets/app.js").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type
            .as_deref()
            .is_some_and(|value| value.contains("javascript")),
        "{content_type:?}"
    );
    assert_contains_all(
        &body,
        &[
            "function sellerOrderActions(status)",
            "entitlement_granted",
            "function sellerOrderActionRow(order)",
            "function markSellerListingPending",
            "function setSellerSyncStatus",
            "function setSellerQuickAddOpen",
            "Accept order",
            "Request payment",
            "Confirm payment",
            "Grant access",
            "Completed - no further seller action needed.",
            "No seller action available",
        ],
    );
}

#[tokio::test]
async fn ui_css_asset_returns_text_css_without_auth() {
    let (status, content_type) = send_ui_request("/ui/assets/app.css").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/css"));
}

#[tokio::test]
async fn ui_favicon_asset_returns_svg_without_auth() {
    let (status, content_type) = send_ui_request("/ui/assets/favicon.svg").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("image/svg+xml"));
}

#[tokio::test]
async fn ui_product_image_asset_returns_png_without_auth() {
    let (status, content_type) = send_ui_request("/ui/assets/products/sneakers.png").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("image/png"));
}

#[tokio::test]
async fn ui_seed_product_image_asset_returns_jpeg_without_auth() {
    let (status, content_type) =
        send_ui_request("/ui/assets/products/seed/fashionprod0101.jpg").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("image/jpeg"));
}

#[tokio::test]
async fn ui_javascript_maps_seeded_products_to_exact_images() {
    let (status, content_type, body) = send_ui_body_request("/ui/assets/app.js").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type
            .as_deref()
            .is_some_and(|value| value.contains("javascript")),
        "{content_type:?}"
    );
    assert_contains_all(
        &body,
        &[
            "prod:fashion.example:FASHIONPROD0101",
            "/ui/assets/products/seed/fashionprod0101.jpg",
        ],
    );
}

#[tokio::test]
async fn ui_js_asset_returns_javascript_without_auth() {
    let (status, content_type) = send_ui_request("/ui/assets/app.js").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type
            .as_deref()
            .is_some_and(|value| value.contains("javascript")),
        "{content_type:?}"
    );
}

#[tokio::test]
async fn synapse_publisher_uses_network_loop_without_direct_ingest() {
    let publisher = SynapseMatrixPublisher::new(
        "http://synapse.test".into(),
        "as-token".into(),
        "@market:shop.example".into(),
    );

    assert!(!publisher.ingest_after_publish());
}

#[tokio::test]
async fn transaction_endpoint_requires_synapse_token() {
    let app = build_router(server_config(), InMemoryEventStore::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/_matrix/app/v1/transactions/txn-1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"events":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn transaction_endpoint_accepts_valid_token() {
    let app = build_router(server_config(), InMemoryEventStore::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/_matrix/app/v1/transactions/txn-1?access_token=hs-token")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"events":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn transaction_endpoint_accepts_synapse_bearer_token() {
    let app = build_router(server_config(), InMemoryEventStore::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/_matrix/app/v1/transactions/txn-1")
                .header("authorization", "Bearer hs-token")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"events":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn appservice_invite_auto_joins_local_sender_room() {
    let publisher = RecordingPublisher::default();
    let joined_rooms = publisher.joined_rooms.clone();
    let app =
        build_router_with_publisher(server_config(), InMemoryEventStore::default(), publisher);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/_matrix/app/v1/transactions/txn-invite")
                .header("authorization", "Bearer hs-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "events": [{
                            "type": "m.room.member",
                            "event_id": "$invite-shop",
                            "room_id": "!remote-order:customer.example",
                            "sender": "@market:customer.example",
                            "state_key": "@market:shop.example",
                            "origin_server_ts": 1_777_888_000_000i64,
                            "content": {"membership": "invite"}
                        }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        joined_rooms.lock().unwrap().as_slice(),
        ["!remote-order:customer.example"]
    );
}

#[tokio::test]
async fn admin_endpoints_reject_missing_malformed_and_wrong_bearer_auth() {
    let endpoints = [
        ("GET", "/admin/config"),
        ("GET", "/admin/allowlist"),
        ("POST", "/admin/catalog/rebuild"),
        ("POST", "/admin/evm-escrow/replay"),
        ("GET", "/admin/evm-escrow/status"),
        ("GET", "/admin/orders/ord:customer.example:01JORDER"),
        ("POST", "/admin/orders/ord:customer.example:01JORDER/replay"),
    ];
    let rejected_authorization = [
        None,
        Some("admin-token"),
        Some("Basic admin-token"),
        Some("Bearer wrong-token"),
    ];

    for (method, uri) in endpoints {
        for authorization in rejected_authorization {
            let (status, body) =
                send_admin_request(InMemoryEventStore::default(), method, uri, authorization).await;

            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
            assert_eq!(
                body,
                json!({ "error": "unauthorized", "code": "ADMIN_UNAUTHORIZED" }),
                "{method} {uri}"
            );
        }
    }
}

#[tokio::test]
async fn admin_evm_escrow_replay_rejects_missing_rpc_url_env() {
    let store = InMemoryEventStore::default();
    store
        .set_evm_escrow_checkpoint(31337, "0x0000000000000000000000000000000000000001", 12)
        .await
        .unwrap();

    let (status, body) = send_admin_request(
        store,
        "POST",
        "/admin/evm-escrow/replay",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body,
        json!({
            "code": "EVM_ESCROW_RPC_URL_MISSING",
            "error": "missing EVM RPC URL env EVM_RPC_URL",
        })
    );
}

#[tokio::test]
async fn admin_evm_escrow_status_reports_checkpoint_and_config() {
    let store = InMemoryEventStore::default();
    store
        .set_evm_escrow_checkpoint(31337, "0x0000000000000000000000000000000000000001", 12)
        .await
        .unwrap();
    let (status, body) = send_admin_request(
        store,
        "GET",
        "/admin/evm-escrow/status",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["enabled"], true);
    assert_eq!(body["chain_id"], 31337);
    assert_eq!(body["rescan_depth"], 3);
    assert_eq!(body["checkpoint"]["latest_scanned_block"], 12);
    assert_eq!(body["watcher"]["mode"], "embedded");
    let watcher = body["watcher"].as_object().unwrap();
    assert!(watcher.contains_key("last_scan"));
    assert!(watcher.contains_key("last_error"));
}

#[tokio::test]
async fn admin_evm_escrow_status_reads_checkpoint_case_insensitively() {
    let mut config = server_config();
    let mixed_case_contract = "0xE7f1725E7734CE288F8367e1Bb143E90bb3F0512";
    config.evm_escrow.as_mut().unwrap().escrow_contract = mixed_case_contract.into();
    let store = InMemoryEventStore::default();
    store
        .set_evm_escrow_checkpoint(31337, &mixed_case_contract.to_lowercase(), 42)
        .await
        .unwrap();

    let (status, body) = send_admin_request_with_config(
        config,
        store,
        "GET",
        "/admin/evm-escrow/status",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["checkpoint"]["latest_scanned_block"], 42);
    assert_eq!(body["escrow_contract"], mixed_case_contract);
}

#[tokio::test]
async fn admin_config_accepts_bearer_auth_and_reports_configured_tokens() {
    let (status, body) = send_admin_request(
        InMemoryEventStore::default(),
        "GET",
        "/admin/config",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "admin": {
                "auth_scheme": "Bearer",
                "token_configured": true,
            },
            "appservice": {
                "homeserver_token_configured": true,
            },
        })
    );
}

#[tokio::test]
async fn admin_allowlist_accepts_bearer_auth_and_returns_deterministic_empty_policy() {
    let (status, body) = send_admin_request(
        InMemoryEventStore::default(),
        "GET",
        "/admin/allowlist",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "allowlist": [],
            "configured": false,
            "source": "server_config",
        })
    );
}

#[tokio::test]
async fn admin_rooms_bootstrap_reports_configured_runtime_rooms() {
    let (status, body) = send_admin_request(
        InMemoryEventStore::default(),
        "POST",
        "/admin/rooms/bootstrap",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "status": "ready",
            "catalog_room_id": "!catalog:shop.example",
            "catalog_room_alias": "#marketplace-catalog:shop.example",
            "order_room_alias_prefix": "#marketplace-order-",
        })
    );
}

#[tokio::test]
async fn admin_catalog_rebuild_accepts_bearer_auth_and_reports_projection_counts() {
    let store = store_with_admin_projection_data().await;
    let (status, body) = send_admin_request(
        store,
        "POST",
        "/admin/catalog/rebuild",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        body,
        json!({
            "status": "scheduled",
            "catalog": {
                "sellers": 1,
                "products": 1,
                "offers": 1,
                "tombstones": 1,
            },
        })
    );
}

#[tokio::test]
async fn admin_order_replay_accepts_bearer_auth_and_reports_order_context() {
    let store = store_with_admin_projection_data().await;
    let (status, body) = send_admin_request(
        store,
        "POST",
        "/admin/orders/ord:customer.example:01JORDER/replay",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        body,
        json!({
            "order_id": "ord:customer.example:01JORDER",
            "status": "scheduled",
            "order": {
                "current_status": "created",
                "event_count": 1,
            },
        })
    );
}

#[tokio::test]
async fn admin_projection_summary_reports_runtime_counts() {
    let store = store_with_admin_projection_data().await;
    let (status, body) = send_admin_request(
        store,
        "GET",
        "/admin/projections/summary",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["catalog"]["sellers"], 1);
    assert_eq!(body["orders"], 1);
}

#[tokio::test]
async fn seller_announce_requires_seller_token_and_local_seller_actor() {
    let store = InMemoryEventStore::default();
    let request = json!({
        "seller_id": "seller:shop.example:01JSELLER",
        "display_name": "Fixture Seller",
        "legal_profile_ref": "https://shop.example/legal",
        "terms_ref": "https://shop.example/terms",
        "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "supported_payment_adapters": ["mock"],
        "supported_entitlement_types": ["external_entitlement"]
    });

    let (unauthorized, _) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/seller/announce",
        Some("Bearer buyer-token"),
        request.clone(),
    )
    .await;
    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/seller/announce",
        Some("Bearer seller-token"),
        request,
    )
    .await;

    assert_eq!(unauthorized, StatusCode::UNAUTHORIZED);
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(store.catalog_sellers().await.unwrap().len(), 1);
}

#[tokio::test]
async fn seller_product_upsert_preserves_uploaded_product_image_metadata() {
    let store = InMemoryEventStore::default();
    let seller = json!({
        "seller_id": "seller:shop.example:01JSELLER",
        "display_name": "Fixture Seller",
        "legal_profile_ref": "https://shop.example/legal",
        "terms_ref": "https://shop.example/terms",
        "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "supported_payment_adapters": ["mock"],
        "supported_entitlement_types": ["external_entitlement"]
    });
    let image_src = "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAAAQABAAD";
    let product = json!({
        "seller_id": "seller:shop.example:01JSELLER",
        "product_id": "prod:shop.example:01JPRODIMG",
        "revision": 1,
        "title": "Book with cover",
        "description": "A product with an uploaded image.",
        "kind": "digital_service",
        "categories": ["books"],
        "tags": ["morpheus"],
        "image_src": image_src,
        "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    });

    let (seller_status, _) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/seller/announce",
        Some("Bearer seller-token"),
        seller,
    )
    .await;
    let (product_status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/seller/products",
        Some("Bearer seller-token"),
        product,
    )
    .await;

    assert_eq!(seller_status, StatusCode::ACCEPTED);
    assert_eq!(product_status, StatusCode::ACCEPTED, "{body}");
    let products = store.catalog_products().await.unwrap();
    assert_eq!(products.len(), 1);
    assert_eq!(products[0].body["image_src"], image_src);
    assert_eq!(products[0].body["media"][0]["kind"], "image");
    assert_eq!(products[0].body["media"][0]["uri"], image_src);
    assert_eq!(products[0].body["media"][0]["role"], "primary");
}

#[tokio::test]
async fn public_write_submits_without_local_projection_when_publisher_uses_synapse_loop() {
    let store = InMemoryEventStore::default();
    let app = build_router_with_publisher(server_config(), store.clone(), SubmittedOnlyPublisher);
    let request = json!({
        "seller_id": "seller:shop.example:01JSELLER",
        "display_name": "Fixture Seller",
        "legal_profile_ref": "https://shop.example/legal",
        "terms_ref": "https://shop.example/terms",
        "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "supported_payment_adapters": ["mock"],
        "supported_entitlement_types": ["external_entitlement"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/seller/announce")
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["status"], "submitted");
    assert_eq!(body["room_id"], "!catalog:shop.example");
    assert_eq!(body["event_ids"].as_array().unwrap().len(), 1);
    assert!(store.catalog_sellers().await.unwrap().is_empty());
}

#[tokio::test]
async fn remote_catalog_sync_failure_keeps_cached_projection_items() {
    let store = InMemoryEventStore::default();
    let live_base =
        start_remote_catalog_server(StatusCode::OK, StatusCode::OK, StatusCode::OK).await;
    let source = RemoteCatalogSource {
        instance_id: "remote.example".into(),
        morpheus_url: live_base,
    };
    let report = sync_remote_catalog_once(&store, &source).await.unwrap();
    assert_eq!(report.status, "live");
    assert_eq!(store.catalog_sellers().await.unwrap().len(), 1);
    assert_eq!(store.catalog_products().await.unwrap().len(), 1);
    assert_eq!(store.catalog_offers().await.unwrap().len(), 1);

    let unavailable_base = start_remote_catalog_server(
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::OK,
        StatusCode::OK,
    )
    .await;
    let source = RemoteCatalogSource {
        instance_id: "remote.example".into(),
        morpheus_url: unavailable_base,
    };
    let report = sync_remote_catalog_once(&store, &source).await.unwrap();

    assert_eq!(report.status, "cached");
    assert_eq!(store.catalog_sellers().await.unwrap().len(), 1);
    assert_eq!(store.catalog_products().await.unwrap().len(), 1);
    assert_eq!(store.catalog_offers().await.unwrap().len(), 1);
}

#[tokio::test]
async fn remote_catalog_live_sync_tombstones_missing_remote_offers() {
    let store = InMemoryEventStore::default();
    let live_base =
        start_remote_catalog_server(StatusCode::OK, StatusCode::OK, StatusCode::OK).await;
    let source = RemoteCatalogSource {
        instance_id: "remote.example".into(),
        morpheus_url: live_base,
    };
    let report = sync_remote_catalog_once(&store, &source).await.unwrap();
    assert_eq!(report.status, "live");

    let empty_base = start_remote_catalog_server_without_offers().await;
    let source = RemoteCatalogSource {
        instance_id: "remote.example".into(),
        morpheus_url: empty_base,
    };
    let report = sync_remote_catalog_once(&store, &source).await.unwrap();
    let (status, body) = send_admin_request(store, "GET", "/api/v1/catalog/offers", None).await;

    assert_eq!(report.status, "live");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn remote_catalog_sync_failure_report_includes_source_and_actionable_error() {
    let store = InMemoryEventStore::default();
    let unavailable_base = start_remote_catalog_server(
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::OK,
        StatusCode::OK,
    )
    .await;
    let source = RemoteCatalogSource {
        instance_id: "remote.example".into(),
        morpheus_url: unavailable_base,
    };

    let report = sync_remote_catalog_once(&store, &source).await.unwrap();

    assert_eq!(report.source.instance_id, "remote.example");
    assert_eq!(report.source.morpheus_url, source.morpheus_url);
    assert_eq!(report.status, "cached");
    let error = report.error.unwrap();
    assert_eq!(error.code, "REMOTE_CATALOG_UNAVAILABLE");
    assert!(error.message.contains("remote catalog returned 503"));
}

#[tokio::test]
async fn seller_actor_from_foreign_instance_is_forbidden() {
    let (status, body) = send_json_request(
        InMemoryEventStore::default(),
        "POST",
        "/api/v1/seller/announce",
        Some("Bearer seller-token"),
        json!({
            "seller_id": "seller:other.example:01JSELLER",
            "display_name": "Foreign Seller",
            "legal_profile_ref": "https://other.example/legal",
            "terms_ref": "https://other.example/terms",
            "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "supported_payment_adapters": ["mock"],
            "supported_entitlement_types": ["external_entitlement"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "ACTOR_FORBIDDEN");
}

#[tokio::test]
async fn buyer_order_create_publishes_customer_binding_before_order_created() {
    let store = store_with_admin_projection_data().await;
    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/buyer/orders",
        Some("Bearer buyer-token"),
        json!({
            "customer_id": "customer:shop.example:01JCUST",
            "customer_display_name": "Fixture Customer",
            "order_id": "ord:shop.example:01JORDER2",
            "seller_id": "seller:shop.example:01JSELLER",
            "offer_id": "offer:shop.example:01JOFFER",
            "offer_revision": 1,
            "catalog_snapshot_id": "snap:shop.example:01JSNAP",
            "price": {"amount": "100.00", "currency": "USD"},
            "payment_adapter": "mock",
            "payment_capture_policy": "before_entitlement",
            "entitlement_type": "external_entitlement",
            "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "arbiter_instance": "cases.example",
            "arbiter_actor": "arbiter:cases.example:01JARBITER",
            "arbitration_policy_id": "standard-digital-v1",
            "arbitration_policy_version": "1",
            "arbitration_window": "P14D",
            "expires_at": "2026-05-04T10:30:00Z"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(
        body["room_id"],
        "!marketplace-order-ord-shop-example-01jorder2:shop.example"
    );
    let events = store
        .marketplace_events_by_room("!marketplace-order-ord-shop-example-01jorder2:shop.example")
        .await
        .unwrap();
    assert_eq!(events[0].event_type, "io.marketplace.actor.customer.bound");
    assert_eq!(events[1].event_type, "io.marketplace.order.created");
    assert_eq!(
        events[1].body["room_id"],
        "!marketplace-order-ord-shop-example-01jorder2:shop.example"
    );
}

#[tokio::test]
async fn buyer_order_create_accepts_evm_escrow_in_customer_binding() {
    let store = store_with_admin_projection_data().await;
    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/buyer/orders",
        Some("Bearer buyer-token"),
        json!({
            "customer_id": "customer:shop.example:01JCUST",
            "customer_display_name": "Fixture Customer",
            "order_id": "ord:shop.example:01JEVMFLOW",
            "seller_id": "seller:shop.example:01JSELLER",
            "offer_id": "offer:shop.example:01JOFFER",
            "offer_revision": 1,
            "catalog_snapshot_id": "snap:shop.example:01JSNAP",
            "price": {"amount": "100.00", "currency": "USD"},
            "payment_adapter": "evm_escrow",
            "payment_capture_policy": "before_entitlement",
            "entitlement_type": "external_entitlement",
            "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "arbiter_instance": "cases.example",
            "arbiter_actor": "arbiter:cases.example:01JARBITER",
            "arbitration_policy_id": "standard-digital-v1",
            "arbitration_policy_version": "1",
            "arbitration_window": "P14D",
            "expires_at": "2026-05-04T10:30:00Z"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let events = store
        .marketplace_events_by_room("!marketplace-order-ord-shop-example-01jevmflow:shop.example")
        .await
        .unwrap();
    assert_eq!(events[0].event_type, "io.marketplace.actor.customer.bound");
    assert_eq!(
        events[0].body["accepted_payment_adapters"],
        json!(["evm_escrow"])
    );
}

#[tokio::test]
async fn buyer_order_create_rejects_foreign_customer_actor() {
    let (status, body) = send_json_request(
        store_with_admin_projection_data().await,
        "POST",
        "/api/v1/buyer/orders",
        Some("Bearer buyer-token"),
        json!({
            "customer_id": "customer:other.example:01JCUST",
            "customer_display_name": "Fixture Customer",
            "order_id": "ord:shop.example:01JORDER4",
            "seller_id": "seller:shop.example:01JSELLER",
            "offer_id": "offer:shop.example:01JOFFER",
            "offer_revision": 1,
            "catalog_snapshot_id": "snap:shop.example:01JSNAP",
            "price": {"amount": "100.00", "currency": "USD"},
            "payment_adapter": "mock",
            "payment_capture_policy": "before_entitlement",
            "entitlement_type": "external_entitlement",
            "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "arbiter_instance": "cases.example",
            "arbiter_actor": "arbiter:cases.example:01JARBITER",
            "arbitration_policy_id": "standard-digital-v1",
            "arbitration_policy_version": "1",
            "arbitration_window": "P14D",
            "expires_at": "2026-05-06T10:30:00Z"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "ACTOR_FORBIDDEN");
}

#[tokio::test]
async fn buyer_order_create_accepts_local_customer_for_remote_catalog_offer() {
    let store = InMemoryEventStore::default();
    store
        .upsert_catalog_seller(
            "seller:books.example:BOOKSSELLER01",
            "books.example",
            "active",
            json!({ "status": "active" }),
        )
        .await
        .unwrap();
    store
        .upsert_catalog_product(
            "prod:books.example:BOOKSPROD0101",
            "seller:books.example:BOOKSSELLER01",
            1,
            json!({ "revision": 1, "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }),
        )
        .await
        .unwrap();
    store
        .upsert_catalog_offer(CatalogOfferProjectionRecord {
            offer_id: "offer:books.example:ORPHAN".into(),
            product_id: "prod:books.example:MISSING".into(),
            seller_id: "seller:books.example:BOOKSSELLER01".into(),
            revision: 1,
            price: json!({ "amount": "99.00", "currency": "USD" }),
            inventory_kind: "unlimited".into(),
            body: json!({
                "revision": 1,
                "payment_terms": {"capture_policy": "before_entitlement"},
                "entitlement": {"type": "external_entitlement"},
                "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            }),
        })
        .await
        .unwrap();
    store
        .upsert_catalog_offer(CatalogOfferProjectionRecord {
            offer_id: "offer:books.example:BOOKSOFFER0101".into(),
            product_id: "prod:books.example:BOOKSPROD0101".into(),
            seller_id: "seller:books.example:BOOKSSELLER01".into(),
            revision: 1,
            price: json!({ "amount": "26.00", "currency": "USD" }),
            inventory_kind: "unlimited".into(),
            body: json!({
                "revision": 1,
                "payment_terms": {"capture_policy": "before_entitlement"},
                "entitlement": {"type": "external_entitlement"},
                "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            }),
        })
        .await
        .unwrap();

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/buyer/orders",
        Some("Bearer buyer-token"),
        json!({
            "customer_id": "customer:shop.example:01JCUST",
            "customer_display_name": "Fixture Customer",
            "order_id": "ord:shop.example:01JORDER5",
            "seller_id": "seller:books.example:BOOKSSELLER01",
            "offer_id": "offer:books.example:BOOKSOFFER0101",
            "offer_revision": 1,
            "catalog_snapshot_id": "snap:books.example:01JSNAP",
            "price": {"amount": "26.00", "currency": "USD"},
            "payment_adapter": "mock",
            "payment_capture_policy": "before_entitlement",
            "entitlement_type": "external_entitlement",
            "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "arbiter_instance": "cases.example",
            "arbiter_actor": "arbiter:cases.example:01JARBITER",
            "arbitration_policy_id": "standard-digital-v1",
            "arbitration_policy_version": "1",
            "arbitration_window": "P14D",
            "expires_at": "2026-05-06T10:30:00Z"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(
        store
            .order("ord:shop.example:01JORDER5")
            .await
            .unwrap()
            .unwrap()
            .seller_id,
        "seller:books.example:BOOKSSELLER01"
    );
}

#[tokio::test]
async fn buyer_catalog_offers_hide_orphan_offer_projections() {
    let store = store_with_admin_projection_data().await;
    store
        .upsert_catalog_offer(CatalogOfferProjectionRecord {
            offer_id: "offer:shop.example:ORPHAN".into(),
            product_id: "prod:shop.example:MISSING".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            revision: 1,
            price: json!({ "amount": "99.00", "currency": "USD" }),
            inventory_kind: "unlimited".into(),
            body: json!({
                "revision": 1,
                "payment_terms": {"capture_policy": "before_entitlement"},
                "entitlement": {"type": "external_entitlement"},
                "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            }),
        })
        .await
        .unwrap();

    let (status, body) = send_admin_request(store, "GET", "/api/v1/catalog/offers", None).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["offer_id"], "offer:shop.example:01JOFFER");
}

#[tokio::test]
async fn buyer_catalog_offers_hide_withdrawn_offer_projection() {
    let store = store_with_admin_projection_data().await;
    store
        .tombstone_catalog_object(
            "offer:shop.example:01JOFFER",
            "offer",
            json!({ "reason": "seller_withdrawn" }),
        )
        .await
        .unwrap();

    let (list_status, list_body) =
        send_admin_request(store.clone(), "GET", "/api/v1/catalog/offers", None).await;
    let (show_status, show_body) = send_admin_request(
        store,
        "GET",
        "/api/v1/catalog/offers/offer:shop.example:01JOFFER",
        None,
    )
    .await;

    assert_eq!(list_status, StatusCode::OK, "{list_body}");
    assert_eq!(list_body["items"].as_array().unwrap().len(), 0);
    assert_eq!(show_status, StatusCode::NOT_FOUND, "{show_body}");
    assert_eq!(show_body["code"], "OFFER_NOT_FOUND");
}

#[tokio::test]
async fn seller_offer_withdraw_publishes_tombstone_projection() {
    let store = store_with_admin_projection_data().await;
    let (withdraw_status, withdraw_body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/seller/offers/offer:shop.example:01JOFFER/withdraw",
        Some("Bearer seller-token"),
        json!({
            "seller_id": "seller:shop.example:01JSELLER",
            "revision": 1,
            "reason": "seller_withdrawn"
        }),
    )
    .await;
    let (list_status, list_body) =
        send_admin_request(store, "GET", "/api/v1/catalog/offers", None).await;

    assert_eq!(withdraw_status, StatusCode::ACCEPTED, "{withdraw_body}");
    assert_eq!(list_status, StatusCode::OK, "{list_body}");
    assert_eq!(list_body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn buyer_order_create_rejects_withdrawn_offer_without_creating_order() {
    let store = store_with_admin_projection_data().await;
    store
        .tombstone_catalog_object(
            "offer:shop.example:01JOFFER",
            "offer",
            json!({ "reason": "seller_withdrawn" }),
        )
        .await
        .unwrap();
    let order_id = "ord:shop.example:01JWITHDRAWN";

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/buyer/orders",
        Some("Bearer buyer-token"),
        fixture_buyer_order_request(order_id, "offer:shop.example:01JOFFER"),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "OFFER_WITHDRAWN");
    assert_eq!(body["details"]["offer_id"], "offer:shop.example:01JOFFER");
    assert!(store.order(order_id).await.unwrap().is_none());
    assert!(store.order_events(order_id).await.unwrap().is_empty());
}

#[tokio::test]
async fn seller_order_action_joins_order_room_before_publish() {
    let store = store_with_admin_projection_data().await;
    let publisher = RecordingPublisher::default();
    let joined_rooms = publisher.joined_rooms.clone();
    let app = build_router_with_publisher(server_config(), store, publisher);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/seller/orders/ord:customer.example:01JORDER/accept")
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "actor_id": "seller:shop.example:01JSELLER",
                        "offer_revision": 1,
                        "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                        "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                        "payment_capture_policy": "before_entitlement",
                        "arbitration_policy_version": "1"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        joined_rooms.lock().unwrap().as_slice(),
        ["!order:customer.example"]
    );
}

#[tokio::test]
async fn seller_order_complete_retry_is_idempotent_without_room_join() {
    let store = store_with_admin_projection_data().await;
    store
        .upsert_order(OrderProjectionRecord {
            order_id: "ord:customer.example:01JORDER".into(),
            room_id: "!order:customer.example".into(),
            customer_id: "customer:customer.example:01JCUST".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            offer_id: "offer:shop.example:01JOFFER".into(),
            status: "completed".into(),
            body: json!({ "order_id": "ord:customer.example:01JORDER" }),
        })
        .await
        .unwrap();
    store
        .record_order_event(
            "ord:customer.example:01JORDER",
            "evt:shop.example:01JCOMPLETE",
            "io.marketplace.order.completed",
            json!({ "order_id": "ord:customer.example:01JORDER" }),
        )
        .await
        .unwrap();

    let publisher = RecordingPublisher::default();
    let joined_rooms = publisher.joined_rooms.clone();
    let app = build_router_with_publisher(server_config(), store, publisher);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/seller/orders/ord:customer.example:01JORDER/complete")
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "actor_id": "seller:shop.example:01JSELLER" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["event_ids"], json!([]));
    assert!(joined_rooms.lock().unwrap().is_empty());
}

#[tokio::test]
async fn seller_order_complete_withdraws_purchased_offer() {
    let store = store_with_admin_projection_data().await;
    let publisher = RecordingPublisher::default();
    let joined_rooms = publisher.joined_rooms.clone();
    let published_events = publisher.published_events.clone();
    let app = build_router_with_publisher(server_config(), store, publisher);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/seller/orders/ord:customer.example:01JORDER/complete")
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "actor_id": "seller:shop.example:01JSELLER" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        joined_rooms.lock().unwrap().as_slice(),
        ["!order:customer.example"]
    );
    let events = published_events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["type"], "io.marketplace.order.completed");
    assert_eq!(events[0]["room_id"], "!order:customer.example");
    assert_eq!(events[1]["type"], "io.marketplace.offer.withdrawn");
    assert_eq!(events[1]["room_id"], "!catalog:shop.example");
    assert_eq!(
        events[1]["content"]["body"]["offer_id"],
        "offer:shop.example:01JOFFER"
    );
    assert_eq!(events[1]["content"]["body"]["revision"], 1);
    assert_eq!(events[1]["content"]["body"]["reason"], "sold");
}

async fn insert_evm_order(store: &InMemoryEventStore, order_id: &str, seller_id: &str) {
    store
        .upsert_order(OrderProjectionRecord {
            order_id: order_id.into(),
            room_id: "!order:shop.example".into(),
            customer_id: "customer:shop.example:01JCUST".into(),
            seller_id: seller_id.into(),
            offer_id: "offer:shop.example:01JOFFER".into(),
            status: "accepted".into(),
            body: json!({
                "order_id": order_id,
                "customer_id": "customer:shop.example:01JCUST",
                "seller_id": seller_id,
                "offer_id": "offer:shop.example:01JOFFER",
                "offer_revision": 1,
                "price": {"amount": "25.00", "currency": "USDC"},
                "payment_adapter": "evm_escrow",
                "payment_capture_policy": "before_entitlement",
                "arbiter_actor": "arbiter:shop.example:01JARBITER"
            }),
        })
        .await
        .unwrap();
}

fn evm_intent_request(actor_id: &str, payment_id: &str, buyer_evm_address: &str) -> Value {
    json!({
        "actor_id": actor_id,
        "payment_id": payment_id,
        "buyer_evm_address": buyer_evm_address,
        "seller_evm_address": "0x0000000000000000000000000000000000000003",
        "arbiter_evm_address": "0x0000000000000000000000000000000000000005"
    })
}

#[tokio::test]
async fn seller_evm_payment_intent_returns_confirmation_metadata() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JEVMORDER";
    insert_evm_order(&store, order_id, "seller:shop.example:01JSELLER").await;
    let publisher = RecordingPublisher::default();
    let published_events = publisher.published_events.clone();
    let app = build_router_with_publisher(server_config(), store, publisher);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/seller/orders/{order_id}/evm-escrow/payment-intent"
                ))
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    evm_intent_request(
                        "seller:shop.example:01JSELLER",
                        "pay:shop.example:01JPAYEVM",
                        "0x0000000000000000000000000000000000000004",
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let events = published_events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "io.marketplace.payment.intent.created");
    let body = &events[0]["content"]["body"];
    assert_eq!(body["order_id"], order_id);
    assert_eq!(body["payment_id"], "pay:shop.example:01JPAYEVM");
    assert_eq!(body["adapter"], "evm_escrow");
    assert_eq!(body["amount"], "25.00");
    assert_eq!(body["currency"], "USDC");
    assert_eq!(body["capture_policy"], "before_entitlement");
    assert_eq!(body["confirmation"]["method"], "evm_escrow_deposit");
    assert!(
        body["confirmation"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("https://shop.example/evm-escrow/"))
    );
    assert_eq!(body["confirmation"]["adapter"], "evm_escrow");
    assert_eq!(body["confirmation"]["chain_id"], 31337);
    assert_eq!(
        body["confirmation"]["token"],
        "0x0000000000000000000000000000000000000002"
    );
    assert_eq!(
        body["confirmation"]["escrow_contract"],
        "0x0000000000000000000000000000000000000001"
    );
    assert_eq!(
        body["confirmation"]["buyer_evm_address"],
        "0x0000000000000000000000000000000000000004"
    );
    assert_eq!(
        body["confirmation"]["seller_evm_address"],
        "0x0000000000000000000000000000000000000003"
    );
    assert_eq!(
        body["confirmation"]["arbiter_evm_address"],
        "0x0000000000000000000000000000000000000005"
    );
    assert!(
        body["confirmation"]["order_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("0x") && hash.len() == 66)
    );
}

#[tokio::test]
async fn buyer_orders_include_evm_payment_confirmation_projection() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JEVMPROJECT";
    insert_evm_order(&store, order_id, "seller:shop.example:01JSELLER").await;
    store
        .upsert_payment(
            "pay:shop.example:01JPAYPROJECT",
            order_id,
            "intent_created",
            json!({
                "order_id": order_id,
                "payment_id": "pay:shop.example:01JPAYPROJECT",
                "adapter": "evm_escrow",
                "amount": "25.00",
                "currency": "USDC",
                "confirmation": {
                    "method": "evm_escrow_deposit",
                    "chain_id": 31337,
                    "token": "0x0000000000000000000000000000000000000002",
                    "amount_units": "25000000",
                    "escrow_contract": "0x0000000000000000000000000000000000000001",
                    "order_hash": "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "buyer_evm_address": "0x0000000000000000000000000000000000000004",
                    "seller_evm_address": "0x0000000000000000000000000000000000000003",
                    "arbiter_evm_address": "0x0000000000000000000000000000000000000005"
                }
            }),
        )
        .await
        .unwrap();

    let (status, body) = send_json_request(
        store,
        "GET",
        "/api/v1/buyer/orders",
        Some("Bearer buyer-token"),
        json!({}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let orders = body["orders"].as_array().expect("orders array");
    let order = orders
        .iter()
        .find(|order| order["order_id"] == order_id)
        .expect("evm order");
    assert_eq!(order["payment"]["body"]["adapter"], "evm_escrow");
    assert_eq!(
        order["payment"]["body"]["confirmation"]["order_hash"],
        "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
    );
    assert_eq!(
        order["payment"]["body"]["confirmation"]["buyer_evm_address"],
        "0x0000000000000000000000000000000000000004"
    );
}

#[tokio::test]
async fn admin_order_show_returns_payment_confirmation_for_arbiter_tools() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JARBEVM";
    insert_evm_order(&store, order_id, "seller:shop.example:01JSELLER").await;
    store
        .upsert_payment(
            "pay:shop.example:01JARBPAY",
            order_id,
            "authorized",
            json!({
                "order_id": order_id,
                "payment_id": "pay:shop.example:01JARBPAY",
                "adapter": "evm_escrow",
                "currency": "USDC",
                "confirmation": {
                    "chain_id": 31337,
                    "token": "0x0000000000000000000000000000000000000002",
                    "amount_units": "25000000",
                    "escrow_contract": "0x0000000000000000000000000000000000000001",
                    "order_hash": "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "buyer_evm_address": "0x0000000000000000000000000000000000000004",
                    "seller_evm_address": "0x0000000000000000000000000000000000000003",
                    "arbiter_evm_address": "0x0000000000000000000000000000000000000005"
                }
            }),
        )
        .await
        .unwrap();

    let (status, body) = send_admin_request(
        store,
        "GET",
        &format!("/admin/orders/{order_id}"),
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["order"]["payment"]["body"]["confirmation"]["order_hash"],
        "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
    );
}

#[tokio::test]
async fn seller_evm_payment_intent_rejects_invalid_evm_address() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JEVMBADADDR";
    insert_evm_order(&store, order_id, "seller:shop.example:01JSELLER").await;
    let app = build_router_with_publisher(server_config(), store, RecordingPublisher::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/seller/orders/{order_id}/evm-escrow/payment-intent"
                ))
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    evm_intent_request(
                        "seller:shop.example:01JSELLER",
                        "pay:shop.example:01JPAYBADADDR",
                        "0x0000000000000000000000000000000000000000",
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "INVALID_EVM_ADDRESS");
}

#[tokio::test]
async fn seller_evm_payment_intent_rejects_wrong_order_seller() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JEVMBADSELLER";
    insert_evm_order(&store, order_id, "seller:shop.example:01JOTHER").await;
    let app = build_router_with_publisher(server_config(), store, RecordingPublisher::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/seller/orders/{order_id}/evm-escrow/payment-intent"
                ))
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    evm_intent_request(
                        "seller:shop.example:01JSELLER",
                        "pay:shop.example:01JPAYBADSELLER",
                        "0x0000000000000000000000000000000000000004",
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "ACTOR_FORBIDDEN");
}

#[tokio::test]
async fn seller_evm_payment_intent_rejects_conflicting_duplicate_payment_id() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JEVMIDEM";
    insert_evm_order(&store, order_id, "seller:shop.example:01JSELLER").await;
    store
        .record_order_event(
            order_id,
            "evt:shop.example:01JEXISTING",
            "io.marketplace.payment.intent.created",
            json!({
                "order_id": order_id,
                "payment_id": "pay:shop.example:01JPAYIDEM",
                "adapter": "evm_escrow",
                "amount": "25.00",
                "currency": "USDC",
                "capture_policy": "before_entitlement",
                "idempotency_key": "evm_escrow:pay:shop.example:01JPAYIDEM",
                "provider_ref": "evm_escrow:0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                "confirmation": {
                    "method": "evm_escrow_deposit",
                    "uri": "https://shop.example/evm-escrow/0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "adapter": "evm_escrow",
                    "chain_id": 31337,
                    "token": "0x0000000000000000000000000000000000000002",
                    "token_currency": "USDC",
                    "amount_units": "25000000",
                    "escrow_contract": "0x0000000000000000000000000000000000000001",
                    "order_hash": "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "buyer_evm_address": "0x0000000000000000000000000000000000000006",
                    "seller_evm_address": "0x0000000000000000000000000000000000000003",
                    "arbiter_actor": "arbiter:shop.example:01JARBITER",
                    "arbiter_evm_address": "0x0000000000000000000000000000000000000005"
                },
                "expires_at": "2026-05-04T10:30:00Z"
            }),
        )
        .await
        .unwrap();

    let (status, body) = send_json_request(
        store,
        "POST",
        &format!("/api/v1/seller/orders/{order_id}/evm-escrow/payment-intent"),
        Some("Bearer seller-token"),
        evm_intent_request(
            "seller:shop.example:01JSELLER",
            "pay:shop.example:01JPAYIDEM",
            "0x0000000000000000000000000000000000000004",
        ),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
async fn seller_evm_payment_intent_rejects_missing_config() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JEVMNOCFG";
    store
        .upsert_order(OrderProjectionRecord {
            order_id: order_id.into(),
            room_id: "!order:shop.example".into(),
            customer_id: "customer:shop.example:01JCUST".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            offer_id: "offer:shop.example:01JOFFER".into(),
            status: "accepted".into(),
            body: json!({
                "order_id": order_id,
                "customer_id": "customer:shop.example:01JCUST",
                "seller_id": "seller:shop.example:01JSELLER",
                "offer_id": "offer:shop.example:01JOFFER",
                "offer_revision": 1,
                "price": {"amount": "25.00", "currency": "USDC"},
                "payment_adapter": "evm_escrow",
                "payment_capture_policy": "before_entitlement",
                "arbiter_actor": "arbiter:shop.example:01JARBITER"
            }),
        })
        .await
        .unwrap();
    let mut config = server_config();
    config.evm_escrow = None;
    let app = build_router_with_publisher(config, store, RecordingPublisher::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/seller/orders/{order_id}/evm-escrow/payment-intent"
                ))
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "actor_id": "seller:shop.example:01JSELLER",
                        "payment_id": "pay:shop.example:01JPAYNOCFG",
                        "buyer_evm_address": "0x0000000000000000000000000000000000000004",
                        "seller_evm_address": "0x0000000000000000000000000000000000000003",
                        "arbiter_evm_address": "0x0000000000000000000000000000000000000005"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "EVM_ESCROW_NOT_CONFIGURED");
}

#[tokio::test]
async fn seller_evm_payment_intent_rejects_non_evm_order() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JMOCKORDER";
    store
        .upsert_order(OrderProjectionRecord {
            order_id: order_id.into(),
            room_id: "!order:shop.example".into(),
            customer_id: "customer:shop.example:01JCUST".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            offer_id: "offer:shop.example:01JOFFER".into(),
            status: "accepted".into(),
            body: json!({
                "order_id": order_id,
                "customer_id": "customer:shop.example:01JCUST",
                "seller_id": "seller:shop.example:01JSELLER",
                "offer_id": "offer:shop.example:01JOFFER",
                "offer_revision": 1,
                "price": {"amount": "25.00", "currency": "USDC"},
                "payment_adapter": "mock",
                "payment_capture_policy": "before_entitlement",
                "arbiter_actor": "arbiter:shop.example:01JARBITER"
            }),
        })
        .await
        .unwrap();
    let app = build_router_with_publisher(server_config(), store, RecordingPublisher::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/seller/orders/{order_id}/evm-escrow/payment-intent"
                ))
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "actor_id": "seller:shop.example:01JSELLER",
                        "payment_id": "pay:shop.example:01JPAYMOCK",
                        "buyer_evm_address": "0x0000000000000000000000000000000000000004",
                        "seller_evm_address": "0x0000000000000000000000000000000000000003",
                        "arbiter_evm_address": "0x0000000000000000000000000000000000000005"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "ORDER_PAYMENT_ADAPTER_MISMATCH");
}

#[tokio::test]
async fn seller_order_action_accepts_browser_encoded_order_id_path() {
    let store = store_with_admin_projection_data().await;
    let app = build_router_with_publisher(server_config(), store, SubmittedOnlyPublisher);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/seller/orders/ord%3Acustomer.example%3A01JORDER/accept")
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "actor_id": "seller:shop.example:01JSELLER",
                        "offer_revision": 1,
                        "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                        "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                        "payment_capture_policy": "before_entitlement",
                        "arbitration_policy_version": "1"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn seller_payment_capture_publishes_authorized_before_captured() {
    let store = store_with_admin_projection_data().await;
    let order_id = "ord:shop.example:01JORDER3";
    let create = json!({
        "customer_id": "customer:shop.example:01JCUST",
        "customer_display_name": "Fixture Customer",
        "order_id": order_id,
        "seller_id": "seller:shop.example:01JSELLER",
        "offer_id": "offer:shop.example:01JOFFER",
        "offer_revision": 1,
        "catalog_snapshot_id": "snap:shop.example:01JSNAP",
        "price": {"amount": "100.00", "currency": "USD"},
        "payment_adapter": "mock",
        "payment_capture_policy": "before_entitlement",
        "entitlement_type": "external_entitlement",
        "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        "arbiter_instance": "cases.example",
        "arbiter_actor": "arbiter:cases.example:01JARBITER",
        "arbitration_policy_id": "standard-digital-v1",
        "arbitration_policy_version": "1",
        "arbitration_window": "P14D",
        "expires_at": "2026-05-06T10:30:00Z"
    });
    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/buyer/orders",
        Some("Bearer buyer-token"),
        create,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        &format!("/api/v1/seller/orders/{order_id}/accept"),
        Some("Bearer seller-token"),
        json!({
            "actor_id": "seller:shop.example:01JSELLER",
            "offer_revision": 1,
            "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "payment_capture_policy": "before_entitlement",
            "arbitration_policy_version": "1"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        &format!("/api/v1/seller/orders/{order_id}/payment-intent"),
        Some("Bearer seller-token"),
        json!({
            "actor_id": "seller:shop.example:01JSELLER",
            "payment_id": "pay:shop.example:01JPAY3",
            "adapter": "mock",
            "amount": "100.00",
            "currency": "USD",
            "capture_policy": "before_entitlement",
            "idempotency_key": "idem:shop.example:01JPAY3",
            "provider_ref": "mock:pi_01JPAY3",
            "confirmation": {"method": "redirect", "uri": "https://shop.example/pay/confirm"},
            "expires_at": "2026-05-06T10:30:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        &format!("/api/v1/seller/orders/{order_id}/payment-capture"),
        Some("Bearer seller-token"),
        json!({
            "actor_id": "seller:shop.example:01JSELLER",
            "payment_id": "pay:shop.example:01JPAY3",
            "adapter": "mock",
            "amount": "100.00",
            "currency": "USD",
            "provider_ref": "mock:cap_01JPAY3",
            "evidence": {
                "kind": "capture",
                "uri": "https://shop.example/evidence/capture",
                "sha256": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let events = store
        .order_events(order_id)
        .await
        .unwrap()
        .into_iter()
        .map(|event| event.event_type)
        .collect::<Vec<_>>();
    assert!(events.contains(&"io.marketplace.payment.authorized".to_string()));
    assert!(events.contains(&"io.marketplace.payment.captured".to_string()));
}

#[tokio::test]
async fn seller_payment_intent_rejects_wrong_order_seller() {
    let order_id = "ord:shop.example:01JORDER_WRONG_PAY_INTENT";
    let store = seed_mock_order_created(store_with_admin_projection_data().await, order_id).await;

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        &format!("/api/v1/seller/orders/{order_id}/payment-intent"),
        Some("Bearer seller-token"),
        json!({
            "actor_id": "seller:shop.example:01JOTHER",
            "payment_id": "pay:shop.example:01JWRONGINTENT",
            "adapter": "mock",
            "amount": "100.00",
            "currency": "USD",
            "capture_policy": "before_entitlement",
            "idempotency_key": "idem:shop.example:01JWRONGINTENT",
            "provider_ref": "mock:pi_01JWRONGINTENT",
            "confirmation": {"method": "redirect", "uri": "https://shop.example/pay/confirm"},
            "expires_at": "2026-05-06T10:30:00Z"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "ACTOR_FORBIDDEN");
}

#[tokio::test]
async fn seller_payment_capture_rejects_wrong_order_seller() {
    let store = seed_mock_order_with_payment_intent(
        store_with_admin_projection_data().await,
        "ord:shop.example:01JORDER_WRONG_CAPTURE",
        "pay:shop.example:01JWRONGCAPTURE",
    )
    .await;

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/seller/orders/ord:shop.example:01JORDER_WRONG_CAPTURE/payment-capture",
        Some("Bearer seller-token"),
        json!({
            "actor_id": "seller:shop.example:01JOTHER",
            "payment_id": "pay:shop.example:01JWRONGCAPTURE",
            "adapter": "mock",
            "amount": "100.00",
            "currency": "USD",
            "provider_ref": "mock:cap_01JWRONGCAPTURE",
            "evidence": {
                "kind": "capture",
                "uri": "https://shop.example/evidence/capture",
                "sha256": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "ACTOR_FORBIDDEN");
}

#[tokio::test]
async fn seller_payment_capture_retry_is_idempotent_after_capture() {
    let store = store_with_admin_projection_data().await;
    let order_id = "ord:shop.example:01JORDER4";
    let create = json!({
        "customer_id": "customer:shop.example:01JCUST",
        "customer_display_name": "Fixture Customer",
        "order_id": order_id,
        "seller_id": "seller:shop.example:01JSELLER",
        "offer_id": "offer:shop.example:01JOFFER",
        "offer_revision": 1,
        "catalog_snapshot_id": "snap:shop.example:01JSNAP",
        "price": {"amount": "100.00", "currency": "USD"},
        "payment_adapter": "mock",
        "payment_capture_policy": "before_entitlement",
        "entitlement_type": "external_entitlement",
        "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        "arbiter_instance": "cases.example",
        "arbiter_actor": "arbiter:cases.example:01JARBITER",
        "arbitration_policy_id": "standard-digital-v1",
        "arbitration_policy_version": "1",
        "arbitration_window": "P14D",
        "expires_at": "2026-05-06T10:30:00Z"
    });
    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        "/api/v1/buyer/orders",
        Some("Bearer buyer-token"),
        create,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        &format!("/api/v1/seller/orders/{order_id}/accept"),
        Some("Bearer seller-token"),
        json!({
            "actor_id": "seller:shop.example:01JSELLER",
            "offer_revision": 1,
            "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "payment_capture_policy": "before_entitlement",
            "arbitration_policy_version": "1"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    let (status, body) = send_json_request(
        store.clone(),
        "POST",
        &format!("/api/v1/seller/orders/{order_id}/payment-intent"),
        Some("Bearer seller-token"),
        json!({
            "actor_id": "seller:shop.example:01JSELLER",
            "payment_id": "pay:shop.example:01JPAY4",
            "adapter": "mock",
            "amount": "100.00",
            "currency": "USD",
            "capture_policy": "before_entitlement",
            "idempotency_key": "idem:shop.example:01JPAY4",
            "provider_ref": "mock:pi_01JPAY4",
            "confirmation": {"method": "redirect", "uri": "https://shop.example/pay/confirm"},
            "expires_at": "2026-05-06T10:30:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    for _ in 0..2 {
        let (status, body) = send_json_request(
            store.clone(),
            "POST",
            &format!("/api/v1/seller/orders/{order_id}/payment-capture"),
            Some("Bearer seller-token"),
            json!({
                "actor_id": "seller:shop.example:01JSELLER",
                "payment_id": "pay:shop.example:01JPAY4",
                "adapter": "mock",
                "amount": "100.00",
                "currency": "USD",
                "provider_ref": "mock:cap_01JPAY4",
                "evidence": {
                    "kind": "capture",
                    "uri": "https://shop.example/evidence/capture",
                    "sha256": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    }

    let events = store.order_events(order_id).await.unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "io.marketplace.payment.authorized")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "io.marketplace.payment.captured")
            .count(),
        1
    );
}
