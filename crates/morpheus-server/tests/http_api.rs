use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use morpheus_protocol::ValidationError;
use morpheus_server::{MatrixPublisher, ServerConfig, build_router, build_router_with_publisher};
use morpheus_store::{
    CatalogOfferProjectionRecord, EventStore, InMemoryEventStore, OrderProjectionRecord,
};
use serde_json::{Value, json};
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
    }
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

    (status, content_type)
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

#[tokio::test]
async fn ui_html_routes_return_ok_without_auth() {
    for uri in ["/ui/admin", "/ui/seller", "/ui/buyer"] {
        let (status, _) = send_ui_request(uri).await;

        assert_eq!(status, StatusCode::OK, "{uri}");
    }
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
            "room_id": "!order2:shop.example",
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
    let events = store
        .marketplace_events_by_room("!order2:shop.example")
        .await
        .unwrap();
    assert_eq!(events[0].event_type, "io.marketplace.actor.customer.bound");
    assert_eq!(events[1].event_type, "io.marketplace.order.created");
}
