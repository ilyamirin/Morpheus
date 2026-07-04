use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use morpheus_protocol::ValidationError;
use morpheus_server::{
    AuthPrincipal, AuthRole, AuthServerConfig, AuthSessionSeed, MatrixPublisher,
    RemoteCatalogSource, ServerConfig, SynapseMatrixPublisher, build_router,
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
        auth: AuthServerConfig::static_tokens("admin-token", "seller-token", "buyer-token"),
    }
}

fn server_config_with_sessions(sessions: Vec<AuthSessionSeed>) -> ServerConfig {
    let mut config = server_config();
    config.auth = AuthServerConfig::static_tokens_with_sessions(
        "admin-token",
        "seller-token",
        "buyer-token",
        sessions,
    );
    config
}

fn server_config_with_oidc_test_token(code: &str, id_token: &str) -> ServerConfig {
    let mut config = server_config();
    config.auth = AuthServerConfig::oidc_test(
        "https://idp.example/realms/morpheus",
        "https://idp.example/realms/morpheus/protocol/openid-connect/auth",
        "https://idp.example/realms/morpheus/protocol/openid-connect/token",
        "morpheus",
        "secret",
        "http://127.0.0.1:8080/auth/callback",
        "test-session-secret",
        vec![(code.into(), id_token.into())],
    );
    config
}

async fn send_admin_request(
    store: InMemoryEventStore,
    method: &str,
    uri: &str,
    authorization: Option<&str>,
) -> (StatusCode, Value) {
    let app = build_router(server_config(), store);
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

async fn send_request_with_cookie(
    config: ServerConfig,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let app = build_router(config, InMemoryEventStore::default());
    let mut request = Request::builder().method(method).uri(uri);
    if body.is_some() {
        request = request.header("content-type", "application/json");
    }
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    let response = app
        .oneshot(
            request
                .body(match body {
                    Some(body) => Body::from(body.to_string()),
                    None => Body::empty(),
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, body)
}

async fn send_request_returning_headers(
    config: ServerConfig,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
) -> (StatusCode, http::HeaderMap, Value) {
    let app = build_router(config, InMemoryEventStore::default());
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    let response = app
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, headers, body)
}

fn insecure_test_id_token(nonce: &str) -> String {
    morpheus_server::encode_insecure_test_id_token(json!({
        "iss": "https://idp.example/realms/morpheus",
        "aud": "morpheus",
        "sub": "user:seller@example.com",
        "name": "Seller User",
        "exp": 4_102_444_800i64,
        "nonce": nonce,
        "roles": ["seller"],
        "morpheus_sellers": ["seller:shop.example:01JSELLER"],
        "morpheus_customers": [],
    }))
}

fn query_param(location: &str, key: &str) -> String {
    let query = location
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or("");
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value.to_string())
        .unwrap_or_default()
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
    assert!(body.contains(r#""auth_mode":"static_tokens""#));
    assert!(body.contains("morpheus-ui-config"));
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
async fn ui_javascript_supports_oidc_session_auth_without_browser_bearer_tokens() {
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
            r#"const SESSION_AUTH = UI_CONFIG.auth_mode === "oidc""#,
            r#"if (tokenRole && !SESSION_AUTH) headers.authorization"#,
            r#"credentials: SESSION_AUTH ? "same-origin" : "same-origin""#,
            r#"document.body.dataset.authMode = UI_CONFIG.auth_mode || "static_tokens""#,
            r#"window.location.href = `/auth/login?return_to=${encodeURIComponent(window.location.pathname + window.location.hash)}`"#,
        ],
    );
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
                "auth_scheme": "Bearer or Session",
                "auth_mode": "static_tokens",
                "token_configured": true,
            },
            "appservice": {
                "homeserver_token_configured": true,
            },
        })
    );
}

#[tokio::test]
async fn auth_session_reports_anonymous_request_without_cookie() {
    let (status, body) =
        send_request_with_cookie(server_config(), "GET", "/auth/session", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "authenticated": false,
            "auth_mode": "static_tokens",
        })
    );
}

#[tokio::test]
async fn auth_session_cookie_authorizes_admin_routes_without_bearer_token() {
    let config = server_config_with_sessions(vec![AuthSessionSeed {
        session_id: "admin-session".into(),
        principal: AuthPrincipal {
            subject: "user:admin@example.com".into(),
            display_name: Some("Admin User".into()),
            roles: vec![AuthRole::Admin],
            seller_actor_ids: vec![],
            buyer_actor_ids: vec![],
        },
    }]);

    let (status, body) = send_request_with_cookie(
        config,
        "GET",
        "/admin/config",
        Some("morpheus_session=admin-session"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["admin"]["auth_scheme"], "Bearer or Session");
}

#[tokio::test]
async fn auth_session_cookie_limits_seller_to_bound_actor_ids() {
    let config = server_config_with_sessions(vec![AuthSessionSeed {
        session_id: "seller-session".into(),
        principal: AuthPrincipal {
            subject: "user:seller@example.com".into(),
            display_name: Some("Seller User".into()),
            roles: vec![AuthRole::Seller],
            seller_actor_ids: vec!["seller:shop.example:01JSELLER".into()],
            buyer_actor_ids: vec![],
        },
    }]);

    let allowed = json!({
        "seller_id": "seller:shop.example:01JSELLER",
        "display_name": "Fixture Seller",
        "legal_profile_ref": "https://shop.example/legal",
        "terms_ref": "https://shop.example/terms",
        "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "supported_payment_adapters": ["mock"],
        "supported_entitlement_types": ["external_entitlement"]
    });
    let (allowed_status, _) = send_request_with_cookie(
        config.clone(),
        "POST",
        "/api/v1/seller/announce",
        Some("morpheus_session=seller-session"),
        Some(allowed),
    )
    .await;
    assert_eq!(allowed_status, StatusCode::ACCEPTED);

    let forbidden = json!({
        "seller_id": "seller:shop.example:02JOTHER",
        "display_name": "Other Seller",
        "legal_profile_ref": "https://shop.example/legal",
        "terms_ref": "https://shop.example/terms",
        "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "supported_payment_adapters": ["mock"],
        "supported_entitlement_types": ["external_entitlement"]
    });
    let (forbidden_status, body) = send_request_with_cookie(
        config,
        "POST",
        "/api/v1/seller/announce",
        Some("morpheus_session=seller-session"),
        Some(forbidden),
    )
    .await;
    assert_eq!(forbidden_status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "ACTOR_FORBIDDEN");
}

#[tokio::test]
async fn oidc_login_redirects_to_provider_with_state_nonce_and_pkce() {
    let config = server_config_with_oidc_test_token("unused", "unused");
    let (status, headers, _) =
        send_request_returning_headers(config, "GET", "/auth/login?return_to=/ui/admin", None)
            .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap();
    assert!(
        location.starts_with("https://idp.example/realms/morpheus/protocol/openid-connect/auth?")
    );
    assert_eq!(query_param(location, "client_id"), "morpheus");
    assert_eq!(query_param(location, "response_type"), "code");
    assert_eq!(query_param(location, "scope"), "openid+profile+email");
    assert!(!query_param(location, "state").is_empty());
    assert!(!query_param(location, "nonce").is_empty());
    assert_eq!(query_param(location, "code_challenge_method"), "S256");
    assert!(!query_param(location, "code_challenge").is_empty());
}

#[tokio::test]
async fn oidc_callback_creates_http_only_session_from_test_claims() {
    let mut config = server_config();
    config.auth = AuthServerConfig::oidc_test(
        "https://idp.example/realms/morpheus",
        "https://idp.example/realms/morpheus/protocol/openid-connect/auth",
        "https://idp.example/realms/morpheus/protocol/openid-connect/token",
        "morpheus",
        "secret",
        "http://127.0.0.1:8080/auth/callback",
        "test-session-secret",
        Vec::new(),
    );

    let (_, login_headers, _) = send_request_returning_headers(
        config.clone(),
        "GET",
        "/auth/login?return_to=/ui/seller",
        None,
    )
    .await;
    let location = login_headers
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap();
    let state = query_param(location, "state");
    let nonce = query_param(location, "nonce");
    config
        .auth
        .add_insecure_test_token("code-1", &insecure_test_id_token(&nonce));

    let (status, callback_headers, _) = send_request_returning_headers(
        config.clone(),
        "GET",
        &format!("/auth/callback?state={state}&code=code-1"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        callback_headers
            .get("location")
            .and_then(|value| value.to_str().ok()),
        Some("/ui/seller")
    );
    let cookie = callback_headers
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .unwrap();
    assert!(cookie.starts_with("morpheus_session="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));

    let (session_status, session_body) =
        send_request_with_cookie(config, "GET", "/auth/session", Some(cookie), None).await;
    assert_eq!(session_status, StatusCode::OK);
    assert_eq!(session_body["authenticated"], true);
    assert_eq!(session_body["principal"]["roles"], json!(["seller"]));
    assert_eq!(
        session_body["principal"]["seller_actor_ids"],
        json!(["seller:shop.example:01JSELLER"])
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
