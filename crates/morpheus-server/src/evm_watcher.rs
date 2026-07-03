use async_trait::async_trait;
use morpheus_config::EvmEscrowConfig;
use morpheus_protocol::{ValidationCode, ValidationError};
use morpheus_store::{EventStore, EvmEscrowLogRecord, OrderProjectionRecord};
use serde_json::Value;

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

struct ExpectedPaymentContext {
    order_id: String,
    room_id: String,
    payment_id: String,
    amount: String,
    currency: String,
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
            let _ = scan_once(&store, &source, &watcher_publisher, scan_config).await;
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
        .await?
        .unwrap_or(config.evm.start_block.unwrap_or(0) as i64);
    let head = source.block_number().await?;
    let safe_to = head - config.evm.confirmations as i64;
    if safe_to <= checkpoint {
        return Ok(WatcherScanResult {
            from_block: checkpoint + 1,
            to_block: safe_to,
            ..WatcherScanResult::default()
        });
    }

    let max_scan = config.evm.max_scan_blocks.unwrap_or(100) as i64;
    let from_block = checkpoint + 1;
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
    let expected = ExpectedEscrowPayment {
        order_hash: required_str(confirmation, "order_hash")?.into(),
        chain_id: required_i64(confirmation, "chain_id")?,
        escrow_contract: required_str(confirmation, "escrow_contract")?.into(),
        token: required_str(confirmation, "token")?.into(),
        amount: required_str(confirmation, "amount_units")?.into(),
        buyer: required_str(confirmation, "buyer_evm_address")?.into(),
        seller: required_str(confirmation, "seller_evm_address")?.into(),
    };
    Ok(ExpectedPaymentContext {
        order_id: order.order_id,
        room_id: order.room_id,
        payment_id,
        amount: required_str(body, "amount")?.into(),
        currency: required_str(body, "currency")?.into(),
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

fn watcher_error(message: impl Into<String>) -> ValidationError {
    ValidationError::new(ValidationCode::PolicyViolation, message.into())
}
