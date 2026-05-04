use morpheus_store::{AppServiceTransactionRecord, EventStore, InMemoryEventStore};

#[tokio::test]
async fn appservice_transactions_are_idempotent() {
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
    assert!(err.to_string().contains("idempotent"));
}
