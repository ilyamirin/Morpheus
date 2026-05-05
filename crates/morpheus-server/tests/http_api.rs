use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use morpheus_server::{ServerConfig, build_router};
use morpheus_store::{
    CatalogOfferProjectionRecord, EventStore, InMemoryEventStore, OrderProjectionRecord,
};
use serde_json::{Value, json};
use tower::ServiceExt;

fn server_config() -> ServerConfig {
    ServerConfig {
        homeserver_token: "hs-token".into(),
        admin_token: "admin-token".into(),
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
            price: json!({ "amount": "100.00" }),
            inventory_kind: "booking_slot".into(),
            body: json!({ "revision": 1 }),
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
