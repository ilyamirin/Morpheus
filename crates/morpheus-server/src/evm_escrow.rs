use morpheus_protocol::{ValidationCode, ValidationError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscrowEventTopics {
    pub deposited: String,
    pub released: String,
    pub refunded: String,
    pub partially_refunded: String,
}

impl EscrowEventTopics {
    pub fn all(&self) -> Vec<String> {
        vec![
            self.deposited.clone(),
            self.released.clone(),
            self.refunded.clone(),
            self.partially_refunded.clone(),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmEscrowIntentInput {
    pub protocol: String,
    pub protocol_version: String,
    pub instance_id: String,
    pub order_id: String,
    pub customer_id: String,
    pub seller_id: String,
    pub offer_id: String,
    pub offer_revision: i64,
    pub price: Value,
    pub payment_adapter: String,
    pub payment_capture_policy: String,
    pub chain_id: u64,
    pub token_contract: String,
    pub amount_units: String,
    pub escrow_contract: String,
    pub seller_evm_address: String,
    pub buyer_evm_address: String,
    pub arbiter_actor: String,
    pub arbiter_evm_address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedEscrowLog {
    pub event_name: String,
    pub order_hash: String,
    pub tx_hash: String,
    pub log_index: i64,
    pub block_number: i64,
    pub block_hash: String,
    pub chain_id: i64,
    pub escrow_contract: String,
    pub token: String,
    pub amount: String,
    pub buyer: Option<String>,
    pub seller: Option<String>,
    pub buyer_amount: Option<String>,
    pub seller_amount: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedEscrowPayment {
    pub order_hash: String,
    pub chain_id: i64,
    pub escrow_contract: String,
    pub token: String,
    pub amount: String,
    pub buyer: String,
    pub seller: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentEventDraft {
    pub event_type: String,
    pub body: Value,
}

pub fn escrow_event_topics() -> EscrowEventTopics {
    EscrowEventTopics {
        deposited: event_topic("EscrowDeposited(bytes32,address,address,address,uint256)"),
        released: event_topic("EscrowReleased(bytes32,address,address,uint256)"),
        refunded: event_topic("EscrowRefunded(bytes32,address,address,uint256)"),
        partially_refunded: event_topic(
            "EscrowPartiallyRefunded(bytes32,address,address,address,uint256,uint256)",
        ),
    }
}

pub fn decode_rpc_log(
    chain_id: i64,
    log: &crate::evm_rpc::RpcLog,
) -> Result<DecodedEscrowLog, ValidationError> {
    let topics = escrow_event_topics();
    let topic0 = log
        .topics
        .first()
        .ok_or_else(|| evm_decode_error("evm escrow log missing topic0"))?;
    let words = data_words(&log.data)?;
    let order_hash = topic_bytes32(required_topic(log, 1)?)?;

    if topic0 == &topics.deposited {
        return Ok(DecodedEscrowLog {
            event_name: "EscrowDeposited".into(),
            order_hash,
            tx_hash: log.transaction_hash.clone(),
            log_index: log.log_index,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            chain_id,
            escrow_contract: log.address.to_lowercase(),
            token: word_address(required_word(&words, 0)?)?,
            amount: word_uint(required_word(&words, 1)?)?,
            buyer: Some(topic_address(required_topic(log, 2)?)?),
            seller: Some(topic_address(required_topic(log, 3)?)?),
            buyer_amount: None,
            seller_amount: None,
        });
    }

    if topic0 == &topics.released {
        return Ok(DecodedEscrowLog {
            event_name: "EscrowReleased".into(),
            order_hash,
            tx_hash: log.transaction_hash.clone(),
            log_index: log.log_index,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            chain_id,
            escrow_contract: log.address.to_lowercase(),
            token: word_address(required_word(&words, 0)?)?,
            amount: word_uint(required_word(&words, 1)?)?,
            buyer: None,
            seller: Some(topic_address(required_topic(log, 2)?)?),
            buyer_amount: None,
            seller_amount: None,
        });
    }

    if topic0 == &topics.refunded {
        return Ok(DecodedEscrowLog {
            event_name: "EscrowRefunded".into(),
            order_hash,
            tx_hash: log.transaction_hash.clone(),
            log_index: log.log_index,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            chain_id,
            escrow_contract: log.address.to_lowercase(),
            token: word_address(required_word(&words, 0)?)?,
            amount: word_uint(required_word(&words, 1)?)?,
            buyer: Some(topic_address(required_topic(log, 2)?)?),
            seller: None,
            buyer_amount: None,
            seller_amount: None,
        });
    }

    if topic0 == &topics.partially_refunded {
        let buyer_amount = word_uint(required_word(&words, 1)?)?;
        let seller_amount = word_uint(required_word(&words, 2)?)?;
        return Ok(DecodedEscrowLog {
            event_name: "EscrowPartiallyRefunded".into(),
            order_hash,
            tx_hash: log.transaction_hash.clone(),
            log_index: log.log_index,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            chain_id,
            escrow_contract: log.address.to_lowercase(),
            token: word_address(required_word(&words, 0)?)?,
            amount: sum_uint_strings(&buyer_amount, &seller_amount)?,
            buyer: Some(topic_address(required_topic(log, 2)?)?),
            seller: Some(topic_address(required_topic(log, 3)?)?),
            buyer_amount: Some(buyer_amount),
            seller_amount: Some(seller_amount),
        });
    }

    Err(evm_decode_error(format!(
        "unknown evm escrow topic {topic0}"
    )))
}

pub fn compute_order_hash(input: &EvmEscrowIntentInput) -> Result<String, ValidationError> {
    let canonical = json!({
        "protocol": input.protocol,
        "protocol_version": input.protocol_version,
        "instance_id": input.instance_id,
        "order_id": input.order_id,
        "customer_id": input.customer_id,
        "seller_id": input.seller_id,
        "offer_id": input.offer_id,
        "offer_revision": input.offer_revision,
        "price": input.price,
        "payment_adapter": input.payment_adapter,
        "payment_capture_policy": input.payment_capture_policy,
        "chain_id": input.chain_id,
        "token_contract": input.token_contract.to_lowercase(),
        "amount_units": input.amount_units,
        "escrow_contract": input.escrow_contract.to_lowercase(),
        "seller_evm_address": input.seller_evm_address.to_lowercase(),
        "buyer_evm_address": input.buyer_evm_address.to_lowercase(),
        "arbiter_actor": input.arbiter_actor,
        "arbiter_evm_address": input.arbiter_evm_address.to_lowercase(),
    });
    let bytes = serde_json::to_vec(&canonical).map_err(|err| {
        ValidationError::new(
            ValidationCode::PolicyViolation,
            format!("failed to serialize evm escrow order hash input: {err}"),
        )
    })?;
    let digest = Sha256::digest(bytes);
    Ok(format!("0x{}", hex::encode(digest)))
}

pub fn map_escrow_log_to_payment_event(
    order_id: &str,
    payment_id: &str,
    amount: &str,
    currency: &str,
    token_decimals: u8,
    log: &DecodedEscrowLog,
) -> Result<PaymentEventDraft, ValidationError> {
    let evidence = evm_log_evidence(log)?;
    let provider_ref = provider_ref(log);
    let buyer_refund_amount = match log.buyer_amount.as_deref() {
        Some(units) => decimal_units_amount(units, token_decimals)?,
        None => amount.to_string(),
    };

    match log.event_name.as_str() {
        "EscrowDeposited" => Ok(PaymentEventDraft {
            event_type: "io.marketplace.payment.authorized".into(),
            body: json!({ "order_id": order_id, "payment_id": payment_id }),
        }),
        "EscrowReleased" => Ok(PaymentEventDraft {
            event_type: "io.marketplace.payment.captured".into(),
            body: json!({
                "order_id": order_id,
                "payment_id": payment_id,
                "adapter": "evm_escrow",
                "amount": amount,
                "currency": currency,
                "provider_ref": provider_ref,
                "evidence": evidence,
            }),
        }),
        "EscrowRefunded" | "EscrowPartiallyRefunded" => Ok(PaymentEventDraft {
            event_type: "io.marketplace.payment.refunded".into(),
            body: json!({
                "order_id": order_id,
                "payment_id": payment_id,
                "refund_id": refund_id_from_log(log)?,
                "amount": buyer_refund_amount,
                "currency": currency,
                "provider_ref": provider_ref,
                "evidence": evidence,
            }),
        }),
        _ => Err(ValidationError::new(
            ValidationCode::UnknownEventType,
            format!("unsupported evm escrow event {}", log.event_name),
        )),
    }
}

pub fn verify_decoded_log(
    expected: &ExpectedEscrowPayment,
    log: &DecodedEscrowLog,
) -> Result<(), ValidationError> {
    let common_fields_match = expected.order_hash.eq_ignore_ascii_case(&log.order_hash)
        && expected.chain_id == log.chain_id
        && expected
            .escrow_contract
            .eq_ignore_ascii_case(&log.escrow_contract)
        && expected.token.eq_ignore_ascii_case(&log.token)
        && expected.amount == log.amount;
    let participants_match = match log.event_name.as_str() {
        "EscrowDeposited" | "EscrowPartiallyRefunded" => {
            participant_matches(log.buyer.as_deref(), &expected.buyer)
                && participant_matches(log.seller.as_deref(), &expected.seller)
        }
        "EscrowReleased" => participant_matches(log.seller.as_deref(), &expected.seller),
        "EscrowRefunded" => participant_matches(log.buyer.as_deref(), &expected.buyer),
        _ => false,
    };

    if common_fields_match && participants_match {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "evm escrow log does not match payment intent",
        ))
    }
}

fn participant_matches(actual: Option<&str>, expected: &str) -> bool {
    actual
        .map(|actual| expected.eq_ignore_ascii_case(actual))
        .unwrap_or(false)
}

fn evm_log_evidence(log: &DecodedEscrowLog) -> Result<Value, ValidationError> {
    let raw_log = json!({
        "chain_id": log.chain_id,
        "escrow_contract": log.escrow_contract,
        "tx_hash": log.tx_hash,
        "log_index": log.log_index,
        "block_number": log.block_number,
        "block_hash": log.block_hash,
        "order_hash": log.order_hash,
        "event_name": log.event_name,
        "token": log.token,
        "amount": log.amount,
        "buyer": log.buyer,
        "seller": log.seller,
        "buyer_amount": log.buyer_amount,
        "seller_amount": log.seller_amount,
    });
    let bytes = serde_json::to_vec(&raw_log).map_err(|err| {
        ValidationError::new(
            ValidationCode::PolicyViolation,
            format!("failed to serialize evm escrow evidence: {err}"),
        )
    })?;
    let digest = Sha256::digest(bytes);

    Ok(json!({
        "kind": "evm_escrow_log",
        "uri": format!(
            "https://evidence.morpheus.local/evm/{}/tx/{}/logs/{}",
            log.chain_id, log.tx_hash, log.log_index
        ),
        "sha256": format!("sha256:{}", hex::encode(digest)),
        "log": raw_log,
    }))
}

fn provider_ref(log: &DecodedEscrowLog) -> String {
    format!("evm:{}:{}:{}", log.chain_id, log.tx_hash, log.log_index)
}

fn refund_id_from_log(log: &DecodedEscrowLog) -> Result<String, ValidationError> {
    assert_full_tx_hash(&log.tx_hash)?;
    let digest = Sha256::digest(provider_ref(log).as_bytes());
    Ok(format!(
        "refund:evm.local:{}",
        hex::encode(digest).to_ascii_uppercase()
    ))
}

fn assert_full_tx_hash(tx_hash: &str) -> Result<(), ValidationError> {
    let hash = tx_hash.strip_prefix("0x").unwrap_or(tx_hash);
    if hash.len() != 64 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(ValidationError::new(
            ValidationCode::InvalidId,
            format!("invalid evm escrow transaction hash for refund id: {tx_hash}"),
        ));
    }

    Ok(())
}

fn event_topic(signature: &str) -> String {
    format!("{:#x}", alloy_primitives::keccak256(signature.as_bytes()))
}

fn topic_bytes32(topic: &str) -> Result<String, ValidationError> {
    let hex = topic_hex(topic)?;
    if hex.len() != 64 {
        return Err(evm_decode_error("evm escrow topic must be 32 bytes"));
    }
    Ok(format!("0x{}", hex.to_ascii_lowercase()))
}

fn topic_address(topic: &str) -> Result<String, ValidationError> {
    let hex = topic_hex(topic)?;
    if hex.len() != 64 {
        return Err(evm_decode_error(
            "evm escrow address topic must be 32 bytes",
        ));
    }
    Ok(format!("0x{}", hex[24..].to_ascii_lowercase()))
}

fn data_words(data: &str) -> Result<Vec<String>, ValidationError> {
    let hex = data
        .strip_prefix("0x")
        .ok_or_else(|| evm_decode_error("evm escrow data missing 0x prefix"))?;
    if hex.len() % 64 != 0 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(evm_decode_error(
            "evm escrow data must contain 32-byte words",
        ));
    }
    Ok(hex
        .as_bytes()
        .chunks(64)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect())
}

fn word_address(word: &str) -> Result<String, ValidationError> {
    if !is_hex_word(word) {
        return Err(evm_decode_error("evm escrow address word must be 32 bytes"));
    }
    Ok(format!("0x{}", word[24..].to_ascii_lowercase()))
}

fn word_uint(word: &str) -> Result<String, ValidationError> {
    if !is_hex_word(word) {
        return Err(evm_decode_error("evm escrow uint word must be 32 bytes"));
    }
    alloy_primitives::U256::from_str_radix(word, 16)
        .map(|value| value.to_string())
        .map_err(|err| evm_decode_error(format!("invalid evm escrow uint word: {err}")))
}

fn sum_uint_strings(left: &str, right: &str) -> Result<String, ValidationError> {
    let left = alloy_primitives::U256::from_str_radix(left, 10)
        .map_err(|err| evm_decode_error(format!("invalid evm escrow uint amount: {err}")))?;
    let right = alloy_primitives::U256::from_str_radix(right, 10)
        .map_err(|err| evm_decode_error(format!("invalid evm escrow uint amount: {err}")))?;
    left.checked_add(right)
        .map(|value| value.to_string())
        .ok_or_else(|| evm_decode_error("evm escrow uint amount overflow"))
}

fn decimal_units_amount(units: &str, decimals: u8) -> Result<String, ValidationError> {
    let value = units
        .parse::<u128>()
        .map_err(|err| evm_decode_error(format!("invalid evm escrow uint amount: {err}")))?;
    if decimals == 0 {
        return Ok(value.to_string());
    }
    let scale = 10u128
        .checked_pow(decimals as u32)
        .ok_or_else(|| evm_decode_error("evm escrow token decimals overflow"))?;
    let whole = value / scale;
    let fraction = value % scale;
    let mut fraction_text = format!("{:0width$}", fraction, width = decimals as usize);
    while fraction_text.len() > 2 && fraction_text.ends_with('0') {
        fraction_text.pop();
    }
    while fraction_text.len() < 2 {
        fraction_text.push('0');
    }
    Ok(format!("{whole}.{fraction_text}"))
}

fn required_topic(log: &crate::evm_rpc::RpcLog, index: usize) -> Result<&str, ValidationError> {
    log.topics
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| evm_decode_error(format!("evm escrow log missing topic {index}")))
}

fn required_word(words: &[String], index: usize) -> Result<&str, ValidationError> {
    words
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| evm_decode_error(format!("evm escrow log missing data word {index}")))
}

fn topic_hex(topic: &str) -> Result<&str, ValidationError> {
    let hex = topic
        .strip_prefix("0x")
        .ok_or_else(|| evm_decode_error("evm escrow topic missing 0x prefix"))?;
    if !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(evm_decode_error("evm escrow topic must be hex"));
    }
    Ok(hex)
}

fn is_hex_word(word: &str) -> bool {
    word.len() == 64 && word.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn evm_decode_error(message: impl Into<String>) -> ValidationError {
    ValidationError::new(ValidationCode::PolicyViolation, message.into())
}
