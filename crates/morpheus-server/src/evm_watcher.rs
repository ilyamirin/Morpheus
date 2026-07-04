use async_trait::async_trait;
use morpheus_config::EvmEscrowConfig;
use morpheus_protocol::{ValidationCode, ValidationError};
use morpheus_store::{EventStore, EvmEscrowLogRecord, OrderProjectionRecord};
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;

use crate::evm_escrow::{ExpectedEscrowPayment, map_escrow_log_to_payment_event};
use crate::evm_rpc::{EvmRpcClient, RpcLog, RpcReceipt};
use crate::{MatrixPublisher, ServerConfig, watcher_payment_event};

#[async_trait]
pub trait EvmLogSource: Clone + Send + Sync + 'static {
    async fn block_number(&self) -> Result<i64, ValidationError>;

    async fn get_logs(
        &self,
        from_block: i64,
        to_block: i64,
        address: &str,
        topics: &[String],
    ) -> Result<Vec<RpcLog>, ValidationError>;

    async fn transaction_receipt(
        &self,
        tx_hash: &str,
    ) -> Result<Option<RpcReceipt>, ValidationError>;
}

#[async_trait]
impl EvmLogSource for EvmRpcClient {
    async fn block_number(&self) -> Result<i64, ValidationError> {
        EvmRpcClient::block_number(self).await
    }

    async fn get_logs(
        &self,
        from_block: i64,
        to_block: i64,
        address: &str,
        topics: &[String],
    ) -> Result<Vec<RpcLog>, ValidationError> {
        EvmRpcClient::get_logs(self, from_block, to_block, address, topics).await
    }

    async fn transaction_receipt(
        &self,
        tx_hash: &str,
    ) -> Result<Option<RpcReceipt>, ValidationError> {
        EvmRpcClient::transaction_receipt(self, tx_hash).await
    }
}

#[async_trait]
pub trait WatcherPublisher: Clone + Send + Sync + 'static {
    async fn publish_payment_event(
        &self,
        room_id: &str,
        event_type: &str,
        body: Value,
    ) -> Result<Value, ValidationError>;
}

#[derive(Debug, Clone)]
pub struct WatcherScanConfig {
    pub evm: EvmEscrowConfig,
    pub instance_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatcherScanResult {
    pub scanned: usize,
    pub accepted: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub from_block: i64,
    pub to_block: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EvmWatcherRuntimeStatus {
    pub last_scan: Option<EvmWatcherScanSnapshot>,
    pub last_error: Option<EvmWatcherErrorSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvmWatcherScanSnapshot {
    pub status: String,
    pub scanned: usize,
    pub accepted: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub from_block: i64,
    pub to_block: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvmWatcherErrorSnapshot {
    pub code: ValidationCode,
    pub message: String,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct SharedEvmWatcherStatus(Arc<RwLock<EvmWatcherRuntimeStatus>>);

impl SharedEvmWatcherStatus {
    pub async fn snapshot(&self) -> EvmWatcherRuntimeStatus {
        self.0.read().await.clone()
    }

    pub async fn record_success(&self, result: &WatcherScanResult) {
        let mut status = self.0.write().await;
        status.last_scan = Some(EvmWatcherScanSnapshot {
            status: "ok".into(),
            scanned: result.scanned,
            accepted: result.accepted,
            duplicates: result.duplicates,
            rejected: result.rejected,
            from_block: result.from_block,
            to_block: result.to_block,
            updated_at_unix_ms: unix_time_ms(),
        });
        status.last_error = None;
    }

    pub async fn record_error(&self, err: &ValidationError) {
        let mut status = self.0.write().await;
        status.last_error = Some(EvmWatcherErrorSnapshot {
            code: err.code,
            message: err.message.clone(),
            updated_at_unix_ms: unix_time_ms(),
        });
    }
}

struct ExpectedPaymentContext {
    order_id: String,
    room_id: String,
    payment_id: String,
    amount: String,
    currency: String,
    token_decimals: u8,
    expected: ExpectedEscrowPayment,
}

#[derive(Clone)]
pub struct MatrixWatcherPublisher<P> {
    server_config: ServerConfig,
    publisher: P,
}

impl<P> MatrixWatcherPublisher<P> {
    pub fn new(server_config: ServerConfig, publisher: P) -> Self {
        Self {
            server_config,
            publisher,
        }
    }
}

#[async_trait]
impl<P> WatcherPublisher for MatrixWatcherPublisher<P>
where
    P: MatrixPublisher,
{
    async fn publish_payment_event(
        &self,
        room_id: &str,
        event_type: &str,
        body: Value,
    ) -> Result<Value, ValidationError> {
        let event = watcher_payment_event(&self.server_config, room_id, event_type, body);
        let mut published = self.publisher.publish(vec![event]).await?;
        published
            .pop()
            .ok_or_else(|| watcher_error("watcher publisher returned no event"))
    }
}

pub fn spawn_evm_escrow_watcher<S, P>(
    store: S,
    publisher: P,
    server_config: ServerConfig,
    rpc_url: String,
    status: SharedEvmWatcherStatus,
) where
    S: EventStore,
    P: MatrixPublisher,
{
    tokio::spawn(async move {
        let source = EvmRpcClient::new(rpc_url);
        let watcher_publisher =
            MatrixWatcherPublisher::new(server_config.clone(), publisher.clone());
        let poll_interval_secs = server_config
            .evm_escrow
            .as_ref()
            .map(|evm| evm.poll_interval_secs)
            .unwrap_or(5);
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(poll_interval_secs));
        loop {
            interval.tick().await;
            let Some(evm) = server_config.evm_escrow.clone() else {
                continue;
            };
            let scan_config = WatcherScanConfig {
                evm,
                instance_id: server_config.instance_id.clone(),
            };
            match scan_once(&store, &source, &watcher_publisher, scan_config).await {
                Ok(result) => status.record_success(&result).await,
                Err(err) => status.record_error(&err).await,
            }
        }
    });
}

pub async fn scan_once<S, R, P>(
    store: &S,
    source: &R,
    publisher: &P,
    config: WatcherScanConfig,
) -> Result<WatcherScanResult, ValidationError>
where
    S: EventStore,
    R: EvmLogSource,
    P: WatcherPublisher,
{
    let chain_id = config.evm.chain_id as i64;
    let escrow_contract = config.evm.escrow_contract.to_lowercase();
    let checkpoint = store
        .evm_escrow_checkpoint(chain_id, &escrow_contract)
        .await?;
    let start_block = config.evm.start_block.unwrap_or(0) as i64;
    let latest_checkpoint = checkpoint.unwrap_or(start_block);
    let head = source.block_number().await?;
    let safe_to = head - config.evm.confirmations as i64;

    let max_scan = config.evm.max_scan_blocks.unwrap_or(100) as i64;
    let from_block =
        if let (Some(checkpoint), Some(rescan_depth)) = (checkpoint, config.evm.rescan_depth) {
            let overlap_start = checkpoint - rescan_depth as i64 + 1;
            std::cmp::max(start_block + 1, overlap_start)
        } else {
            latest_checkpoint + 1
        };
    if safe_to < from_block {
        return Ok(WatcherScanResult {
            from_block,
            to_block: safe_to,
            ..WatcherScanResult::default()
        });
    }

    let to_block = std::cmp::min(safe_to, from_block + max_scan - 1);
    let topic_values = crate::evm_escrow::escrow_event_topics().all();
    let logs = source
        .get_logs(
            from_block,
            to_block,
            &config.evm.escrow_contract,
            &topic_values,
        )
        .await?;
    let logs = logs
        .into_iter()
        .filter(|log| log.block_number >= from_block && log.block_number <= to_block)
        .collect::<Vec<_>>();
    let mut result = WatcherScanResult {
        scanned: logs.len(),
        from_block,
        to_block,
        ..WatcherScanResult::default()
    };

    for rpc_log in logs {
        process_rpc_log(store, source, publisher, &config, &mut result, rpc_log).await?;
    }

    store
        .set_evm_escrow_checkpoint(chain_id, &escrow_contract, to_block)
        .await?;
    Ok(result)
}

async fn process_rpc_log<S, R, P>(
    store: &S,
    source: &R,
    publisher: &P,
    config: &WatcherScanConfig,
    result: &mut WatcherScanResult,
    rpc_log: RpcLog,
) -> Result<(), ValidationError>
where
    S: EventStore,
    R: EvmLogSource,
    P: WatcherPublisher,
{
    let decoded = match crate::evm_escrow::decode_rpc_log(config.evm.chain_id as i64, &rpc_log) {
        Ok(decoded) => decoded,
        Err(_) => {
            result.rejected += 1;
            return Ok(());
        }
    };
    let receipt = source.transaction_receipt(&decoded.tx_hash).await?;
    if !receipt_confirms_log(receipt.as_ref(), &decoded) {
        result.rejected += 1;
        return Ok(());
    }
    let Some(expected) = expected_payment_by_order_hash(store, &decoded.order_hash).await? else {
        result.rejected += 1;
        return Ok(());
    };
    if crate::evm_escrow::verify_decoded_log(&expected.expected, &decoded).is_err() {
        result.rejected += 1;
        return Ok(());
    }

    let record = EvmEscrowLogRecord {
        chain_id: decoded.chain_id,
        tx_hash: decoded.tx_hash.clone(),
        log_index: decoded.log_index,
        block_number: decoded.block_number,
        block_hash: decoded.block_hash.clone(),
        escrow_contract: decoded.escrow_contract.clone(),
        order_hash: decoded.order_hash.clone(),
        event_name: decoded.event_name.clone(),
        payload: serde_json::to_value(&decoded)
            .map_err(|err| watcher_error(format!("failed to serialize evm escrow log: {err}")))?,
        emitted_marketplace_event_id: None,
    };
    if !store.record_evm_escrow_log(record).await? {
        result.duplicates += 1;
        return Ok(());
    }

    let payment = map_escrow_log_to_payment_event(
        &expected.order_id,
        &expected.payment_id,
        &expected.amount,
        &expected.currency,
        expected.token_decimals,
        &decoded,
    )?;
    publisher
        .publish_payment_event(&expected.room_id, &payment.event_type, payment.body)
        .await?;
    result.accepted += 1;
    Ok(())
}

async fn expected_payment_by_order_hash<S: EventStore>(
    store: &S,
    order_hash: &str,
) -> Result<Option<ExpectedPaymentContext>, ValidationError> {
    let orders = store.orders().await?;
    let payments = store.payments().await?;
    for payment in payments {
        let Some(confirmation) = payment.body.get("confirmation") else {
            continue;
        };
        if confirmation.get("method").and_then(Value::as_str) != Some("evm_escrow_deposit") {
            continue;
        }
        let Some(confirmed_order_hash) = confirmation.get("order_hash").and_then(Value::as_str)
        else {
            continue;
        };
        if !confirmed_order_hash.eq_ignore_ascii_case(order_hash) {
            continue;
        }
        let Some(order) = orders
            .iter()
            .find(|order| order.order_id == payment.order_id)
            .cloned()
        else {
            continue;
        };
        return expected_payment_context(order, payment.payment_id, &payment.body).map(Some);
    }
    Ok(None)
}

fn expected_payment_context(
    order: OrderProjectionRecord,
    payment_id: String,
    body: &Value,
) -> Result<ExpectedPaymentContext, ValidationError> {
    let confirmation = body
        .get("confirmation")
        .ok_or_else(|| watcher_error("evm escrow payment missing confirmation"))?;
    let amount = required_str(body, "amount")?;
    let amount_units = required_str(confirmation, "amount_units")?;
    let expected = ExpectedEscrowPayment {
        order_hash: required_str(confirmation, "order_hash")?.into(),
        chain_id: required_i64(confirmation, "chain_id")?,
        escrow_contract: required_str(confirmation, "escrow_contract")?.into(),
        token: required_str(confirmation, "token")?.into(),
        amount: amount_units.into(),
        buyer: required_str(confirmation, "buyer_evm_address")?.into(),
        seller: required_str(confirmation, "seller_evm_address")?.into(),
    };
    Ok(ExpectedPaymentContext {
        order_id: order.order_id,
        room_id: order.room_id,
        payment_id,
        amount: amount.into(),
        currency: required_str(body, "currency")?.into(),
        token_decimals: optional_u8(confirmation, "token_decimals")?
            .map_or_else(|| infer_token_decimals(amount, amount_units), Ok)?,
        expected,
    })
}

fn receipt_confirms_log(
    receipt: Option<&RpcReceipt>,
    log: &crate::evm_escrow::DecodedEscrowLog,
) -> bool {
    receipt.is_some_and(|receipt| {
        receipt.success
            && receipt.transaction_hash.eq_ignore_ascii_case(&log.tx_hash)
            && receipt.block_hash.eq_ignore_ascii_case(&log.block_hash)
            && receipt.block_number == log.block_number
    })
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, ValidationError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| watcher_error(format!("evm escrow payment missing {field}")))
}

fn required_i64(value: &Value, field: &str) -> Result<i64, ValidationError> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| watcher_error(format!("evm escrow payment missing {field}")))
}

fn optional_u8(value: &Value, field: &str) -> Result<Option<u8>, ValidationError> {
    let Some(raw) = value.get(field).and_then(Value::as_u64) else {
        return Ok(None);
    };
    u8::try_from(raw)
        .map(Some)
        .map_err(|_| watcher_error(format!("evm escrow payment invalid {field}")))
}

fn infer_token_decimals(amount: &str, amount_units: &str) -> Result<u8, ValidationError> {
    (0..=18)
        .find(|decimals| {
            decimal_amount_units(amount, *decimals).is_ok_and(|candidate| candidate == amount_units)
        })
        .ok_or_else(|| watcher_error("evm escrow payment missing token_decimals"))
}

fn decimal_amount_units(amount: &str, decimals: u8) -> Result<String, ValidationError> {
    let (whole, fraction) = amount
        .split_once('.')
        .map_or((amount, ""), |(whole, fraction)| (whole, fraction));
    if whole.is_empty()
        || !whole.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
        || fraction.len() > decimals as usize
    {
        return Err(watcher_error(
            "evm escrow amount cannot be represented by inferred token decimals",
        ));
    }

    let mut digits = String::with_capacity(whole.len() + decimals as usize);
    let trimmed_whole = whole.trim_start_matches('0');
    digits.push_str(if trimmed_whole.is_empty() {
        "0"
    } else {
        trimmed_whole
    });
    digits.push_str(fraction);
    for _ in fraction.len()..decimals as usize {
        digits.push('0');
    }
    let trimmed = digits.trim_start_matches('0');
    Ok(if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.into()
    })
}

fn watcher_error(message: impl Into<String>) -> ValidationError {
    ValidationError::new(ValidationCode::PolicyViolation, message.into())
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
