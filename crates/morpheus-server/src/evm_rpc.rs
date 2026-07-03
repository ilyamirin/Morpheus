use morpheus_protocol::{ValidationCode, ValidationError};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcLog {
    pub address: String,
    pub block_hash: String,
    pub block_number: i64,
    pub transaction_hash: String,
    pub log_index: i64,
    pub topics: Vec<String>,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcReceipt {
    pub transaction_hash: String,
    pub block_hash: String,
    pub block_number: i64,
    pub success: bool,
}

pub fn parse_hex_quantity(value: &str) -> Result<i64, ValidationError> {
    let hex = value
        .strip_prefix("0x")
        .ok_or_else(|| rpc_error("hex quantity missing 0x prefix"))?;
    if hex.is_empty() || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(rpc_error("invalid hex quantity"));
    }
    i64::from_str_radix(hex, 16)
        .map_err(|err| rpc_error(format!("hex quantity out of range: {err}")))
}

pub fn rpc_log_from_value(value: Value) -> Result<RpcLog, ValidationError> {
    let field = |name: &str| -> Result<String, ValidationError> {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| rpc_error(format!("rpc log missing {name}")))
    };
    let topics = value
        .get("topics")
        .and_then(Value::as_array)
        .ok_or_else(|| rpc_error("rpc log missing topics"))?
        .iter()
        .map(|topic| {
            topic
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| rpc_error("rpc log topic must be string"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(RpcLog {
        address: field("address")?,
        block_hash: field("blockHash")?,
        block_number: parse_hex_quantity(&field("blockNumber")?)?,
        transaction_hash: field("transactionHash")?,
        log_index: parse_hex_quantity(&field("logIndex")?)?,
        topics,
        data: field("data")?,
    })
}

pub fn rpc_receipt_from_value(value: Value) -> Result<RpcReceipt, ValidationError> {
    let field = |name: &str| -> Result<String, ValidationError> {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| rpc_error(format!("rpc receipt missing {name}")))
    };
    Ok(RpcReceipt {
        transaction_hash: field("transactionHash")?,
        block_hash: field("blockHash")?,
        block_number: parse_hex_quantity(&field("blockNumber")?)?,
        success: parse_hex_quantity(&field("status")?)? == 1,
    })
}

#[derive(Debug, Clone)]
pub struct EvmRpcClient {
    url: String,
    client: reqwest::Client,
}

impl EvmRpcClient {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
        }
    }

    pub async fn block_number(&self) -> Result<i64, ValidationError> {
        let value = self.call("eth_blockNumber", serde_json::json!([])).await?;
        value
            .as_str()
            .ok_or_else(|| rpc_error("eth_blockNumber result must be string"))
            .and_then(parse_hex_quantity)
    }

    pub async fn get_logs(
        &self,
        from_block: i64,
        to_block: i64,
        address: &str,
        topics: &[String],
    ) -> Result<Vec<RpcLog>, ValidationError> {
        let value = self
            .call(
                "eth_getLogs",
                serde_json::json!([{
                    "fromBlock": hex_quantity(from_block)?,
                    "toBlock": hex_quantity(to_block)?,
                    "address": address,
                    "topics": [topics],
                }]),
            )
            .await?;
        value
            .as_array()
            .ok_or_else(|| rpc_error("eth_getLogs result must be array"))?
            .iter()
            .cloned()
            .map(rpc_log_from_value)
            .collect()
    }

    pub async fn transaction_receipt(
        &self,
        tx_hash: &str,
    ) -> Result<Option<RpcReceipt>, ValidationError> {
        let value = self
            .call("eth_getTransactionReceipt", serde_json::json!([tx_hash]))
            .await?;
        if value.is_null() {
            return Ok(None);
        }
        rpc_receipt_from_value(value).map(Some)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, ValidationError> {
        let response = self
            .client
            .post(&self.url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params
            }))
            .send()
            .await
            .map_err(|err| rpc_error(format!("evm rpc {method} request failed: {err}")))?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .map_err(|err| rpc_error(format!("evm rpc {method} response was not json: {err}")))?;
        if !status.is_success() {
            return Err(rpc_error(format!(
                "evm rpc {method} returned http {status}: {body}"
            )));
        }
        if let Some(error) = body.get("error") {
            return Err(rpc_error(format!(
                "evm rpc {method} returned error: {error}"
            )));
        }
        body.get("result")
            .cloned()
            .ok_or_else(|| rpc_error(format!("evm rpc {method} missing result")))
    }
}

fn hex_quantity(value: i64) -> Result<String, ValidationError> {
    if value < 0 {
        return Err(rpc_error("hex quantity cannot be negative"));
    }
    Ok(format!("0x{value:x}"))
}

pub fn rpc_error(message: impl Into<String>) -> ValidationError {
    ValidationError::new(ValidationCode::PolicyViolation, message.into())
}
