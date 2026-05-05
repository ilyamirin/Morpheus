use morpheus_protocol::ValidationCode;
use morpheus_store::{
    AppServiceTransactionRecord, CatalogOfferProjectionRecord, EventStore, InMemoryEventStore,
    MarketplaceEventRecord, OrderProjectionRecord, ProjectionErrorRecord, RawMatrixEventRecord,
    migrations,
};

fn raw_event(event_id: &str, room_id: &str) -> RawMatrixEventRecord {
    RawMatrixEventRecord {
        event_id: event_id.into(),
        room_id: room_id.into(),
        sender: "@market:shop.example".into(),
        event_type: "io.marketplace.offer.upserted".into(),
        origin_server_ts: 1,
        raw_json: serde_json::json!({ "event_id": event_id }),
        validation_status: "accepted".into(),
        validation_code: None,
    }
}

#[tokio::test]
async fn store_records_and_reads_raw_events() {
    let store = InMemoryEventStore::default();
    let raw = raw_event("$e1", "!catalog:shop.example");

    store.record_raw_event(raw).await.unwrap();

    assert_eq!(
        store.raw_event("$e1").await.unwrap().unwrap().event_id,
        "$e1"
    );
    assert!(store.raw_event("$missing").await.unwrap().is_none());
}

#[tokio::test]
async fn store_records_marketplace_events_by_room() {
    let store = InMemoryEventStore::default();
    store
        .record_raw_event(raw_event("$e1", "!catalog:shop.example"))
        .await
        .unwrap();
    store
        .record_raw_event(raw_event("$e2", "!other:shop.example"))
        .await
        .unwrap();

    store
        .record_marketplace_event(MarketplaceEventRecord {
            marketplace_event_id: "evt:shop.example:01JMARKET1".into(),
            matrix_event_id: "$e1".into(),
            protocol_version: "0.1".into(),
            issuer_instance: "shop.example".into(),
            actor_id: Some("seller:shop.example:01JSELLER".into()),
            event_type: "io.marketplace.offer.upserted".into(),
            body: serde_json::json!({ "offer_id": "offer:shop.example:01JOFFER" }),
            created_at: "2026-05-05T00:00:00Z".into(),
        })
        .await
        .unwrap();
    store
        .record_marketplace_event(MarketplaceEventRecord {
            marketplace_event_id: "evt:shop.example:01JMARKET2".into(),
            matrix_event_id: "$e2".into(),
            protocol_version: "0.1".into(),
            issuer_instance: "shop.example".into(),
            actor_id: None,
            event_type: "io.marketplace.offer.upserted".into(),
            body: serde_json::json!({ "offer_id": "offer:shop.example:01JOTHER" }),
            created_at: "2026-05-05T00:00:01Z".into(),
        })
        .await
        .unwrap();

    let events = store
        .marketplace_events_by_room("!catalog:shop.example")
        .await
        .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].marketplace_event_id,
        "evt:shop.example:01JMARKET1"
    );
}

#[tokio::test]
async fn store_records_projection_errors() {
    let store = InMemoryEventStore::default();
    store
        .record_raw_event(raw_event("$e1", "!catalog:shop.example"))
        .await
        .unwrap();

    store
        .record_projection_error(ProjectionErrorRecord {
            matrix_event_id: Some("$e1".into()),
            code: ValidationCode::CatalogReferenceMismatch,
            message: "bad catalog".into(),
            details: serde_json::json!({ "field": "offer_id" }),
        })
        .await
        .unwrap();

    let errors = store.projection_errors().await.unwrap();

    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].matrix_event_id.as_deref(), Some("$e1"));
    assert_eq!(errors[0].code, ValidationCode::CatalogReferenceMismatch);
}

#[tokio::test]
async fn appservice_transactions_remain_idempotent() {
    let store = InMemoryEventStore::default();
    let tx = AppServiceTransactionRecord {
        txn_id: "txn-1".into(),
        event_ids: vec!["$a".into(), "$b".into()],
    };

    store
        .record_appservice_transaction(tx.clone())
        .await
        .unwrap();
    store.record_appservice_transaction(tx).await.unwrap();

    let err = store
        .record_appservice_transaction(AppServiceTransactionRecord {
            txn_id: "txn-1".into(),
            event_ids: vec!["$other".into()],
        })
        .await
        .expect_err("conflicting transaction rejected");

    assert_eq!(err.code, ValidationCode::DuplicateEvent);
}

#[tokio::test]
async fn store_upserts_catalog_and_order_projection_records() {
    let store = InMemoryEventStore::default();

    store
        .upsert_catalog_seller(
            "seller:shop.example:01JSELLER",
            "shop.example",
            "active",
            serde_json::json!({ "status": "active" }),
        )
        .await
        .unwrap();
    store
        .upsert_catalog_product(
            "prod:shop.example:01JPROD",
            "seller:shop.example:01JSELLER",
            1,
            serde_json::json!({ "revision": 1 }),
        )
        .await
        .unwrap();
    store
        .upsert_catalog_offer(CatalogOfferProjectionRecord {
            offer_id: "offer:shop.example:01JOFFER".into(),
            product_id: "prod:shop.example:01JPROD".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            revision: 1,
            price: serde_json::json!({ "amount": "100.00" }),
            inventory_kind: "booking_slot".into(),
            body: serde_json::json!({ "revision": 1 }),
        })
        .await
        .unwrap();
    store
        .tombstone_catalog_object(
            "offer:shop.example:01JOLD",
            "offer",
            serde_json::json!({ "reason": "removed" }),
        )
        .await
        .unwrap();

    let sellers = store.catalog_sellers().await.unwrap();
    let products = store.catalog_products().await.unwrap();
    let offers = store.catalog_offers().await.unwrap();
    let tombstones = store.catalog_tombstones().await.unwrap();

    assert_eq!(sellers[0].seller_id, "seller:shop.example:01JSELLER");
    assert_eq!(products[0].product_id, "prod:shop.example:01JPROD");
    assert_eq!(offers[0].offer_id, "offer:shop.example:01JOFFER");
    assert_eq!(offers[0].price, serde_json::json!({ "amount": "100.00" }));
    assert_eq!(tombstones[0].object_id, "offer:shop.example:01JOLD");

    store
        .upsert_order(OrderProjectionRecord {
            order_id: "ord:customer.example:01JORDER".into(),
            room_id: "!order:customer.example".into(),
            customer_id: "customer:customer.example:01JCUST".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            offer_id: "offer:shop.example:01JOFFER".into(),
            status: "created".into(),
            body: serde_json::json!({ "order_id": "ord:customer.example:01JORDER" }),
        })
        .await
        .unwrap();
    store
        .record_order_event(
            "ord:customer.example:01JORDER",
            "evt:shop.example:01JMARKET3",
            "io.marketplace.order.created",
            serde_json::json!({ "status": "created" }),
        )
        .await
        .unwrap();

    assert_eq!(
        store
            .order("ord:customer.example:01JORDER")
            .await
            .unwrap()
            .unwrap()
            .status,
        "created"
    );
    assert_eq!(store.orders().await.unwrap().len(), 1);
    assert_eq!(
        store
            .order_events("ord:customer.example:01JORDER")
            .await
            .unwrap()[0]
            .event_type,
        "io.marketplace.order.created"
    );
}

#[tokio::test]
async fn store_upserts_payment_entitlement_dispute_and_ruling_projection_records() {
    let store = InMemoryEventStore::default();

    store
        .upsert_payment(
            "pay:customer.example:01JPAYB",
            "ord:customer.example:01JORDER",
            "authorized",
            serde_json::json!({ "provider": "mock" }),
        )
        .await
        .unwrap();
    store
        .upsert_payment(
            "pay:customer.example:01JPAYA",
            "ord:customer.example:01JORDER",
            "captured",
            serde_json::json!({ "provider": "mock" }),
        )
        .await
        .unwrap();
    store
        .upsert_entitlement(
            "ent:customer.example:01JENT",
            "ord:customer.example:01JORDER",
            "active",
            serde_json::json!({ "scope": "download" }),
        )
        .await
        .unwrap();
    store
        .upsert_dispute(
            "disp:customer.example:01JDISP",
            "ord:customer.example:01JORDER",
            "opened",
            serde_json::json!({ "reason": "not_as_described" }),
        )
        .await
        .unwrap();
    store
        .upsert_arbitration_ruling(
            "ruling:arb.example:01JRULE",
            "disp:customer.example:01JDISP",
            "accepted",
            serde_json::json!({ "winner": "customer" }),
        )
        .await
        .unwrap();

    let payments = store.payments().await.unwrap();
    let entitlements = store.entitlements().await.unwrap();
    let disputes = store.disputes().await.unwrap();
    let rulings = store.arbitration_rulings().await.unwrap();

    assert_eq!(payments[0].payment_id, "pay:customer.example:01JPAYA");
    assert_eq!(payments[1].payment_id, "pay:customer.example:01JPAYB");
    assert_eq!(entitlements[0].status, "active");
    assert_eq!(disputes[0].status, "opened");
    assert_eq!(rulings[0].status, "accepted");
}

#[test]
fn postgres_migration_matches_event_store_projection_contract() {
    let sql = migrations::POSTGRES_0001;
    for required in [
        "issuer_instance TEXT NOT NULL",
        "inventory_kind TEXT NOT NULL",
        "object_type TEXT NOT NULL",
        "marketplace_event_id TEXT PRIMARY KEY",
        "ruling_id TEXT PRIMARY KEY",
        "status TEXT NOT NULL",
        "body JSONB NOT NULL",
    ] {
        assert!(sql.contains(required), "missing {required}");
    }
}
