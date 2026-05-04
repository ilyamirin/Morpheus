use morpheus_protocol::{ValidationCode, validate_event_envelope};
use serde_json::json;

#[test]
fn validates_order_created_envelope() {
    let event = json!({
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
    });

    let validated = validate_event_envelope(&event).expect("valid order.created");
    assert_eq!(validated.event_type, "io.marketplace.order.created");
    assert_eq!(
        validated.marketplace_event_id,
        "evt:customer.example:01JORDER"
    );
}

#[test]
fn rejects_unknown_critical_fields() {
    let mut event = morpheus_protocol::fixtures::valid_order_created_event();
    event["content"]["critical"] = json!(["com.example.unknown"]);

    let err = validate_event_envelope(&event).expect_err("critical fields are rejected");
    assert_eq!(err.code, ValidationCode::UnknownCritical);
}

#[test]
fn rejects_order_room_replay() {
    let mut event = morpheus_protocol::fixtures::valid_order_created_event();
    event["room_id"] = json!("!other:customer.example");

    let err = validate_event_envelope(&event).expect_err("room mismatch is rejected");
    assert_eq!(err.code, ValidationCode::CatalogReferenceMismatch);
}
