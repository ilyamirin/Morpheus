use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use morpheus_protocol::validate_event_envelope;
use morpheus_server::{ServerConfig, build_router};
use morpheus_store::{EventStore, InMemoryEventStore};
use serde_json::{Value, json};
use tower::ServiceExt;

const HASH: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SELLER_TERMS_HASH: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const OFFER_TERMS_HASH: &str =
    "sha256:2222222222222222222222222222222222222222222222222222222222222222";

fn catalog_event(event_type: &str, event_suffix: &str, body: Value) -> Value {
    json!({
        "type": event_type,
        "room_id": "!catalog:shop.example",
        "event_id": format!("$matrix-catalog-{event_suffix}"),
        "sender": "@market:shop.example",
        "origin_server_ts": 1_777_888_000_000i64,
        "content": {
            "protocol": "io.marketplace",
            "protocol_version": "0.1",
            "protocol_event_id": format!("evt:shop.example:{event_suffix}"),
            "created_at": "2026-05-04T10:00:00Z",
            "issuer": {
                "instance_id": "shop.example",
                "actor_id": "seller:shop.example:01JSELLER",
                "matrix_user_id": "@market:shop.example"
            },
            "critical": [],
            "body": body
        }
    })
}

fn order_event(event_type: &str, event_suffix: &str, body: Value) -> Value {
    let mut event = morpheus_protocol::fixtures::valid_order_created_event();
    event["type"] = json!(event_type);
    event["event_id"] = json!(format!("$matrix-order-{event_suffix}"));
    event["content"]["protocol_event_id"] = json!(format!("evt:customer.example:{event_suffix}"));
    event["content"]["body"] = body;
    event
}

fn customer_bound_event() -> Value {
    order_event(
        "io.marketplace.actor.customer.bound",
        "01JCUSTBOUND",
        json!({
            "customer_id": "customer:customer.example:01JCUST",
            "status": "active",
            "display_name": "Fixture Customer",
            "instance_id": "customer.example",
            "authorized_representatives": ["@market:customer.example"],
            "accepted_payment_adapters": ["mock"],
            "accepted_arbitration_policies": ["standard-digital-v1"]
        }),
    )
}

fn order_created_event() -> Value {
    let mut event = morpheus_protocol::fixtures::valid_order_created_event();
    event["content"]["body"]["offer_revision"] = json!(1);
    event["content"]["body"]["entitlement_type"] = json!("external_entitlement");
    event
}

fn catalog_trio() -> Vec<Value> {
    vec![
        catalog_event(
            "io.marketplace.actor.seller.announced",
            "01JSELLER",
            json!({
                "seller_id": "seller:shop.example:01JSELLER",
                "status": "active",
                "display_name": "Fixture Seller",
                "legal_profile_ref": "https://shop.example/legal",
                "terms_ref": "https://shop.example/terms",
                "terms_hash": HASH,
                "supported_payment_adapters": ["mock"],
                "supported_entitlement_types": ["external_entitlement"]
            }),
        ),
        catalog_event(
            "io.marketplace.product.upserted",
            "01JPRODUCT",
            json!({
                "product_id": "prod:shop.example:01JPROD",
                "seller_id": "seller:shop.example:01JSELLER",
                "revision": 1,
                "status": "active",
                "kind": "digital_service",
                "title": "Remote consulting",
                "description": "One hour remote consulting session",
                "categories": ["services"],
                "tags": ["remote"],
                "media": [],
                "terms_hash": HASH
            }),
        ),
        catalog_event(
            "io.marketplace.offer.upserted",
            "01JOFFER",
            json!({
                "offer_id": "offer:shop.example:01JOFFER",
                "product_id": "prod:shop.example:01JPROD",
                "seller_id": "seller:shop.example:01JSELLER",
                "revision": 1,
                "status": "active",
                "price": {"amount": "100.00", "currency": "USD"},
                "payment_terms": {
                    "capture_policy": "before_entitlement",
                    "adapter_policy": "seller_supported"
                },
                "entitlement": {
                    "type": "external_entitlement",
                    "delivery": "external"
                },
                "availability": {"mode": "unlimited"},
                "seller_terms_hash": SELLER_TERMS_HASH,
                "offer_terms_hash": OFFER_TERMS_HASH
            }),
        ),
    ]
}

fn order_lifecycle() -> Vec<Value> {
    vec![
        customer_bound_event(),
        order_created_event(),
        order_event(
            "io.marketplace.order.accepted",
            "01JACCEPTED",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "offer_revision": 1,
                "seller_terms_hash": SELLER_TERMS_HASH,
                "offer_terms_hash": OFFER_TERMS_HASH,
                "payment_capture_policy": "before_entitlement",
                "arbitration_policy_version": "1"
            }),
        ),
        order_event(
            "io.marketplace.payment.intent.created",
            "01JPAYINTENT",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "adapter": "mock",
                "amount": "100.00",
                "currency": "USD",
                "capture_policy": "before_entitlement",
                "idempotency_key": "idem-01JORDER",
                "provider_ref": "mock_pi_01JORDER",
                "confirmation": {
                    "method": "redirect",
                    "uri": "https://pay.example/confirm"
                },
                "expires_at": "2026-05-04T10:30:00Z"
            }),
        ),
        order_event(
            "io.marketplace.payment.authorized",
            "01JPAYAUTH",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY"
            }),
        ),
        order_event(
            "io.marketplace.payment.captured",
            "01JPAYCAPTURED",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "adapter": "mock",
                "amount": "100.00",
                "currency": "USD",
                "provider_ref": "mock_ch_01JORDER",
                "evidence": {
                    "kind": "receipt",
                    "uri": "https://pay.example/receipts/01JORDER",
                    "sha256": HASH
                }
            }),
        ),
        order_event(
            "io.marketplace.entitlement.granted",
            "01JENTGRANTED",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "entitlement_id": "ent:customer.example:01JENT",
                "type": "external_entitlement",
                "external_ref": "delivery-01JORDER",
                "evidence": {
                    "kind": "delivery",
                    "uri": "https://deliver.example/entitlements/01JORDER",
                    "sha256": HASH
                }
            }),
        ),
        order_event(
            "io.marketplace.entitlement.completed",
            "01JENTDONE",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "entitlement_id": "ent:customer.example:01JENT"
            }),
        ),
        order_event(
            "io.marketplace.order.completed",
            "01JORDERDONE",
            json!({
                "order_id": "ord:customer.example:01JORDER"
            }),
        ),
    ]
}

async fn send_transaction(txn_id: &str, events: Vec<Value>) -> StatusCode {
    let store = InMemoryEventStore::default();
    send_transaction_to_store(store, txn_id, events).await.0
}

async fn send_transaction_to_store(
    store: InMemoryEventStore,
    txn_id: &str,
    events: Vec<Value>,
) -> (StatusCode, Value) {
    let app = build_router(
        ServerConfig {
            instance_id: "shop.example".into(),
            matrix_server_name: "customer.example".into(),
            catalog_room_id: "!catalog:shop.example".into(),
            appservice_sender_localpart: "market".into(),
            homeserver_token: "hs-token".into(),
            admin_token: "admin-token".into(),
            seller_token: "seller-token".into(),
            buyer_token: "buyer-token".into(),
        },
        store,
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/_matrix/app/v1/transactions/{txn_id}?access_token=hs-token"
                ))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "events": events }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, body)
}

#[test]
fn catalog_trio_fixtures_are_protocol_valid_wire_events() {
    for event in catalog_trio() {
        validate_event_envelope(&event).unwrap();
    }
}

#[test]
fn order_lifecycle_fixtures_are_protocol_valid_wire_events() {
    for event in order_lifecycle() {
        validate_event_envelope(&event).unwrap();
    }
}

#[tokio::test]
async fn send_transaction_accepts_behavior_fixtures() {
    let mut events = catalog_trio();
    events.extend(order_lifecycle());

    assert_eq!(
        send_transaction("txn-behavior-fixtures", events).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn send_transaction_projects_catalog_order_and_payment_state() {
    let store = InMemoryEventStore::default();
    let mut events = catalog_trio();
    events.extend(order_lifecycle());
    let (status, body) =
        send_transaction_to_store(store.clone(), "txn-project-behavior-fixtures", events).await;
    let projection_errors = store.projection_errors().await.unwrap();

    assert_eq!(status, StatusCode::OK, "{body} {projection_errors:?}");
    assert!(projection_errors.is_empty());

    let sellers = store.catalog_sellers().await.unwrap();
    let products = store.catalog_products().await.unwrap();
    let offers = store.catalog_offers().await.unwrap();
    let order = store
        .order("ord:customer.example:01JORDER")
        .await
        .unwrap()
        .unwrap();
    let payments = store.payments().await.unwrap();
    let entitlements = store.entitlements().await.unwrap();
    let order_events = store
        .order_events("ord:customer.example:01JORDER")
        .await
        .unwrap();
    let marketplace_events = store
        .marketplace_events_by_room("!order:customer.example")
        .await
        .unwrap();

    assert_eq!(sellers[0].status, "active");
    assert_eq!(products[0].revision, 1);
    assert_eq!(offers[0].inventory_kind, "unlimited");
    assert_eq!(order.status, "completed");
    assert_eq!(payments[0].status, "captured");
    assert_eq!(entitlements[0].status, "completed");
    assert_eq!(order_events.len(), 8);
    assert_eq!(marketplace_events.len(), 9);
}

#[tokio::test]
async fn duplicate_appservice_transaction_is_noop_after_success() {
    let store = InMemoryEventStore::default();
    let mut events = catalog_trio();
    events.extend(order_lifecycle());

    let (first_status, first_body) =
        send_transaction_to_store(store.clone(), "txn-idempotent-order", events.clone()).await;
    let (second_status, second_body) =
        send_transaction_to_store(store.clone(), "txn-idempotent-order", events).await;

    assert_eq!(first_status, StatusCode::OK, "{first_body}");
    assert_eq!(second_status, StatusCode::OK, "{second_body}");
    assert!(store.projection_errors().await.unwrap().is_empty());
    assert_eq!(
        store
            .marketplace_events_by_room("!order:customer.example")
            .await
            .unwrap()
            .len(),
        9
    );
}

#[tokio::test]
async fn conflicting_appservice_transaction_id_is_rejected_before_processing() {
    let store = InMemoryEventStore::default();
    let (first_status, first_body) =
        send_transaction_to_store(store.clone(), "txn-conflicting-order", catalog_trio()).await;
    let (second_status, second_body) =
        send_transaction_to_store(store.clone(), "txn-conflicting-order", order_lifecycle()).await;

    assert_eq!(first_status, StatusCode::OK, "{first_body}");
    assert_eq!(second_status, StatusCode::CONFLICT, "{second_body}");
    assert_eq!(second_body["code"], "DUPLICATE_EVENT");
    assert!(store.projection_errors().await.unwrap().is_empty());
}

#[tokio::test]
async fn send_transaction_rejects_order_created_when_catalog_terms_do_not_match() {
    let store = InMemoryEventStore::default();
    let mut invalid_order = order_created_event();
    invalid_order["content"]["body"]["offer_revision"] = json!(2);
    let mut events = catalog_trio();
    events.push(customer_bound_event());
    events.push(invalid_order);

    let (status, body) =
        send_transaction_to_store(store.clone(), "txn-invalid-catalog-terms", events).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "CATALOG_REFERENCE_MISMATCH");
    assert!(
        store
            .order("ord:customer.example:01JORDER")
            .await
            .unwrap()
            .is_none()
    );
    let raw = store
        .raw_event("$matrix-order-created")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(raw.validation_status, "rejected");
    assert_eq!(store.projection_errors().await.unwrap().len(), 1);
}

#[tokio::test]
async fn send_transaction_rejects_payment_capture_before_payment_intent() {
    let store = InMemoryEventStore::default();
    let mut events = catalog_trio();
    events.push(customer_bound_event());
    events.push(order_created_event());
    events.push(order_event(
        "io.marketplace.order.accepted",
        "01JACCEPTED",
        json!({
            "order_id": "ord:customer.example:01JORDER",
            "offer_revision": 1,
            "seller_terms_hash": SELLER_TERMS_HASH,
            "offer_terms_hash": OFFER_TERMS_HASH,
            "payment_capture_policy": "before_entitlement",
            "arbitration_policy_version": "1"
        }),
    ));
    events.push(order_event(
        "io.marketplace.payment.captured",
        "01JPAYCAPTURED",
        json!({
            "order_id": "ord:customer.example:01JORDER",
            "payment_id": "pay:customer.example:01JPAY",
            "adapter": "mock",
            "amount": "100.00",
            "currency": "USD",
            "provider_ref": "mock_ch_01JORDER",
            "evidence": {
                "kind": "receipt",
                "uri": "https://pay.example/receipts/01JORDER",
                "sha256": HASH
            }
        }),
    ));

    let (status, body) =
        send_transaction_to_store(store.clone(), "txn-invalid-payment-order", events).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "INVALID_STATE_TRANSITION");
    assert!(store.payments().await.unwrap().is_empty());
    let raw = store
        .raw_event("$matrix-order-01JPAYCAPTURED")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(raw.validation_status, "rejected");
}
