use async_trait::async_trait;
use morpheus_config::{EvmEscrowConfig, EvmEscrowTokenConfig};
use morpheus_protocol::{ValidationCode, ValidationError};
use morpheus_server::evm_rpc::{RpcLog, RpcReceipt};
use morpheus_server::evm_watcher::{
    EvmLogSource, SharedEvmWatcherStatus, WatcherPublisher, WatcherScanConfig, WatcherScanResult,
    scan_once,
};
use morpheus_store::{EventStore, InMemoryEventStore, OrderProjectionRecord};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeLogSource {
    head: i64,
    logs: Vec<RpcLog>,
    receipts: Vec<RpcReceipt>,
    queries: Arc<Mutex<Vec<(i64, i64)>>>,
}

impl FakeLogSource {
    fn new(head: i64, logs: Vec<RpcLog>, receipts: Vec<RpcReceipt>) -> Self {
        Self {
            head,
            logs,
            receipts,
            queries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn queries(&self) -> Vec<(i64, i64)> {
        self.queries.lock().unwrap().clone()
    }
}

#[async_trait]
impl EvmLogSource for FakeLogSource {
    async fn block_number(&self) -> Result<i64, morpheus_protocol::ValidationError> {
        Ok(self.head)
    }

    async fn get_logs(
        &self,
        from: i64,
        to: i64,
        _address: &str,
        _topics: &[String],
    ) -> Result<Vec<RpcLog>, morpheus_protocol::ValidationError> {
        self.queries.lock().unwrap().push((from, to));
        Ok(self.logs.clone())
    }

    async fn transaction_receipt(
        &self,
        tx_hash: &str,
    ) -> Result<Option<RpcReceipt>, morpheus_protocol::ValidationError> {
        Ok(self
            .receipts
            .iter()
            .find(|receipt| receipt.transaction_hash == tx_hash)
            .cloned())
    }
}

#[derive(Clone, Default)]
struct FakeWatcherPublisher {
    events: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl WatcherPublisher for FakeWatcherPublisher {
    async fn publish_payment_event(
        &self,
        room_id: &str,
        event_type: &str,
        body: Value,
    ) -> Result<Value, morpheus_protocol::ValidationError> {
        let event = json!({
            "room_id": room_id,
            "type": event_type,
            "content": { "body": body },
            "event_id": "$watcher"
        });
        self.events.lock().unwrap().push(event.clone());
        Ok(event)
    }
}

#[tokio::test]
async fn watcher_publishes_authorized_for_verified_deposit_log() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment(&store).await;
    let source = FakeLogSource::new(20, vec![deposit_rpc_log()], vec![success_receipt()]);
    let publisher = FakeWatcherPublisher::default();

    let result = scan_once(&store, &source, &publisher, watcher_config())
        .await
        .unwrap();

    assert_eq!(result.accepted, 1);
    assert_eq!(
        publisher.events.lock().unwrap()[0]["type"],
        "io.marketplace.payment.authorized"
    );
}

#[tokio::test]
async fn watcher_waits_for_confirmations() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment(&store).await;
    let source = FakeLogSource::new(
        10,
        vec![deposit_rpc_log_at_block(10)],
        vec![success_receipt_at_block(10)],
    );
    let publisher = FakeWatcherPublisher::default();

    let result = scan_once(
        &store,
        &source,
        &publisher,
        watcher_config_with_confirmations(2),
    )
    .await
    .unwrap();

    assert_eq!(result.scanned, 0);
    assert_eq!(result.accepted, 0);
    assert!(publisher.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn watcher_rejects_failed_receipt() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment(&store).await;
    let source = FakeLogSource::new(20, vec![deposit_rpc_log()], vec![failed_receipt()]);
    let publisher = FakeWatcherPublisher::default();

    let result = scan_once(&store, &source, &publisher, watcher_config())
        .await
        .unwrap();

    assert_eq!(result.accepted, 0);
    assert_eq!(result.rejected, 1);
    assert!(publisher.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn watcher_rejects_amount_mismatch_without_publish() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment_with_amount(&store, "26000000").await;
    let source = FakeLogSource::new(
        20,
        vec![deposit_rpc_log_at_block(18)],
        vec![success_receipt_at_block(18)],
    );
    let publisher = FakeWatcherPublisher::default();

    let result = scan_once(&store, &source, &publisher, watcher_config())
        .await
        .unwrap();

    assert_eq!(result.accepted, 0);
    assert_eq!(result.rejected, 1);
    assert!(publisher.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn watcher_deduplicates_processed_logs() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment(&store).await;
    let source = FakeLogSource::new(
        20,
        vec![deposit_rpc_log_at_block(18)],
        vec![success_receipt_at_block(18)],
    );
    let publisher = FakeWatcherPublisher::default();

    scan_once(&store, &source, &publisher, watcher_config())
        .await
        .unwrap();
    store
        .set_evm_escrow_checkpoint(31337, "0x0000000000000000000000000000000000000001", 0)
        .await
        .unwrap();
    let duplicate = scan_once(&store, &source, &publisher, watcher_config())
        .await
        .unwrap();

    assert_eq!(duplicate.duplicates, 1);
    assert_eq!(publisher.events.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn watcher_rescans_overlap_without_republishing_duplicates() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment(&store).await;
    let source = FakeLogSource::new(
        20,
        vec![deposit_rpc_log_at_block(18)],
        vec![success_receipt_at_block(18)],
    );
    let publisher = FakeWatcherPublisher::default();

    let first = scan_once(
        &store,
        &source,
        &publisher,
        watcher_config_with_rescan_depth(3),
    )
    .await
    .unwrap();
    let second = scan_once(
        &store,
        &source,
        &publisher,
        watcher_config_with_rescan_depth(3),
    )
    .await
    .unwrap();

    assert_eq!(first.accepted, 1);
    assert_eq!(second.duplicates, 1);
    assert_eq!(publisher.events.lock().unwrap().len(), 1);
    assert_eq!(source.queries(), vec![(1, 19), (17, 19)]);
}

#[tokio::test]
async fn watcher_runtime_status_tracks_success_and_error() {
    let status = SharedEvmWatcherStatus::default();
    status
        .record_success(&WatcherScanResult {
            scanned: 2,
            accepted: 1,
            duplicates: 1,
            rejected: 0,
            from_block: 17,
            to_block: 19,
        })
        .await;

    let snapshot = status.snapshot().await;
    let last_scan = snapshot.last_scan.unwrap();
    assert_eq!(last_scan.status, "ok");
    assert_eq!(last_scan.accepted, 1);
    assert!(last_scan.updated_at_unix_ms > 0);
    assert!(snapshot.last_error.is_none());

    status
        .record_error(&ValidationError::new(
            ValidationCode::PolicyViolation,
            "rpc scan failed",
        ))
        .await;

    let snapshot = status.snapshot().await;
    assert_eq!(snapshot.last_scan.unwrap().duplicates, 1);
    let last_error = snapshot.last_error.unwrap();
    assert_eq!(last_error.code, ValidationCode::PolicyViolation);
    assert_eq!(last_error.message, "rpc scan failed");
    assert!(last_error.updated_at_unix_ms > 0);
}

async fn seed_evm_order_and_payment(store: &InMemoryEventStore) {
    seed_evm_order_and_payment_with_amount(store, "25000000").await;
}

async fn seed_evm_order_and_payment_with_amount(store: &InMemoryEventStore, amount_units: &str) {
    let order_id = "ord:shop.example:01JEVMORDER";
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
    store
        .upsert_payment(
            "pay:shop.example:01JPAYEVM",
            order_id,
            "pending",
            json!({
                "order_id": order_id,
                "payment_id": "pay:shop.example:01JPAYEVM",
                "adapter": "evm_escrow",
                "amount": "25.00",
                "currency": "USDC",
                "confirmation": {
                    "method": "evm_escrow_deposit",
                    "adapter": "evm_escrow",
                    "chain_id": 31337,
                    "token": "0x0000000000000000000000000000000000000002",
                    "token_decimals": 6,
                    "amount_units": amount_units,
                    "escrow_contract": "0x0000000000000000000000000000000000000001",
                    "order_hash": "0x1111111111111111111111111111111111111111111111111111111111111111",
                    "buyer_evm_address": "0x0000000000000000000000000000000000000004",
                    "seller_evm_address": "0x0000000000000000000000000000000000000003"
                }
            }),
        )
        .await
        .unwrap();
}

fn deposit_rpc_log() -> RpcLog {
    deposit_rpc_log_at_block(10)
}

fn deposit_rpc_log_at_block(block_number: i64) -> RpcLog {
    RpcLog {
        address: "0x0000000000000000000000000000000000000001".into(),
        block_hash: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        block_number,
        transaction_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .into(),
        log_index: 0,
        topics: vec![
            morpheus_server::evm_escrow::escrow_event_topics().deposited,
            "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
            "0x0000000000000000000000000000000000000000000000000000000000000004".into(),
            "0x0000000000000000000000000000000000000000000000000000000000000003".into(),
        ],
        data: concat!(
            "0x",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "00000000000000000000000000000000000000000000000000000000017d7840",
        )
        .into(),
    }
}

fn success_receipt() -> RpcReceipt {
    success_receipt_at_block(10)
}

fn success_receipt_at_block(block_number: i64) -> RpcReceipt {
    RpcReceipt {
        transaction_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .into(),
        block_hash: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        block_number,
        success: true,
    }
}

fn failed_receipt() -> RpcReceipt {
    RpcReceipt {
        success: false,
        ..success_receipt()
    }
}

fn watcher_config() -> WatcherScanConfig {
    watcher_config_with_confirmations(1)
}

fn watcher_config_with_confirmations(confirmations: u64) -> WatcherScanConfig {
    WatcherScanConfig {
        instance_id: "shop.example".into(),
        evm: EvmEscrowConfig {
            enabled: true,
            chain_id: 31337,
            rpc_url_env: "MORPHEUS_EVM_RPC_URL".into(),
            escrow_contract: "0x0000000000000000000000000000000000000001".into(),
            default_token: "0x0000000000000000000000000000000000000002".into(),
            confirmations,
            poll_interval_secs: 1,
            start_block: Some(0),
            max_scan_blocks: Some(100),
            rescan_depth: None,
            deployments_path: None,
            tokens: vec![EvmEscrowTokenConfig {
                symbol: "USDC".into(),
                contract: "0x0000000000000000000000000000000000000002".into(),
                decimals: 6,
                currency: "USDC".into(),
            }],
        },
    }
}

fn watcher_config_with_rescan_depth(rescan_depth: u64) -> WatcherScanConfig {
    let mut config = watcher_config();
    config.evm.rescan_depth = Some(rescan_depth);
    config
}
