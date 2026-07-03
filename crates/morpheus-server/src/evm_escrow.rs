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
