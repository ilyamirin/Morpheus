use morpheus_protocol::ValidationCode;
use morpheus_store::{
    AppServiceTransactionRecord, CatalogOfferProjectionRecord, EventStore, MarketplaceEventRecord,
    OrderProjectionRecord, RawMatrixEventRecord, SqliteEventStore, migrations,
};
use sqlx::SqlitePool;

async fn sqlite_store() -> SqliteEventStore {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(migrations::SQLITE_0001)
        .execute(&pool)
        .await
        .unwrap();
    SqliteEventStore::new(pool)
}

fn raw_event(event_id: &str, room_id: &str) -> RawMatrixEventRecord {
    RawMatrixEventRecord {
        event_id: event_id.into(),
        room_id: room_id.into(),
        sender: "@market:shop.example".into(),
        event_type: "io.marketplace.offer.upserted".into(),
        origin_server_ts: 1,
        raw_json: serde_json::json!({ "event_id": event_id, "nested": { "ok": true } }),
        validation_status: "accepted".into(),
        validation_code: None,
    }
}

#[tokio::test]
async fn sqlite_appservice_transactions_are_idempotent() {
    let store = sqlite_store().await;
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
async fn sqlite_persists_raw_and_marketplace_events_by_room() {
    let store = sqlite_store().await;
    let raw = raw_event("$e1", "!catalog:shop.example");
    store.record_raw_event(raw.clone()).await.unwrap();
    store
        .record_raw_event(raw_event("$e2", "!other:shop.example"))
        .await
        .unwrap();

    assert_eq!(
        store.raw_event("$e1").await.unwrap().unwrap().raw_json,
        raw.raw_json
    );

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
    assert_eq!(
        events[0].body,
        serde_json::json!({ "offer_id": "offer:shop.example:01JOFFER" })
    );
}

#[tokio::test]
async fn sqlite_persists_catalog_and_order_projections() {
    let store = sqlite_store().await;

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
        .upsert_catalog_offer(CatalogOfferProjectionRecord {
            offer_id: "offer:shop.example:01JOFFER".into(),
            product_id: "prod:shop.example:01JPROD".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            revision: 1,
            price: serde_json::json!({ "amount": "100.00", "currency": "USD" }),
            inventory_kind: "booking_slot".into(),
            body: serde_json::json!({ "revision": 1 }),
        })
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
            body: serde_json::json!({ "order_id": "ord:customer.example:01JORDER" }),
        })
        .await
        .unwrap();

    assert_eq!(
        store.catalog_sellers().await.unwrap()[0].issuer_instance,
        "shop.example"
    );
    assert_eq!(
        store.catalog_offers().await.unwrap()[0].price,
        serde_json::json!({ "amount": "100.00", "currency": "USD" })
    );
    assert_eq!(
        store
            .order("ord:customer.example:01JORDER")
            .await
            .unwrap()
            .unwrap()
            .status,
        "created"
    );
}
