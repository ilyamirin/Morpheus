use morpheus_protocol::{ValidationCode, ValidationError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

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
pub struct PaymentEventDraft {
    pub event_type: String,
    pub body: Value,
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
    log: &DecodedEscrowLog,
) -> Result<PaymentEventDraft, ValidationError> {
    let evidence = json!({
        "kind": "evm_escrow_log",
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
                "amount": log.amount,
                "currency": "USDC",
                "provider_ref": format!("evm:{}:{}:{}", log.chain_id, log.tx_hash, log.log_index),
                "evidence": evidence,
            }),
        }),
        "EscrowRefunded" | "EscrowPartiallyRefunded" => Ok(PaymentEventDraft {
            event_type: "io.marketplace.payment.refunded".into(),
            body: json!({
                "order_id": order_id,
                "payment_id": payment_id,
                "refund_id": format!("refund:local:{}", &log.tx_hash.trim_start_matches("0x")[..16]),
                "amount": log.buyer_amount.as_deref().unwrap_or(log.amount.as_str()),
                "currency": "USDC",
                "provider_ref": format!("evm:{}:{}:{}", log.chain_id, log.tx_hash, log.log_index),
                "evidence": evidence,
            }),
        }),
        _ => Err(ValidationError::new(
            ValidationCode::UnknownEventType,
            format!("unsupported evm escrow event {}", log.event_name),
        )),
    }
}
