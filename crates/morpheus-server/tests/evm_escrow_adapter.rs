use morpheus_server::evm_escrow::{compute_order_hash, EvmEscrowIntentInput};
use serde_json::json;

fn locked_terms_input() -> EvmEscrowIntentInput {
    EvmEscrowIntentInput {
        protocol: "io.marketplace".into(),
        protocol_version: "0.1".into(),
        instance_id: "shop.example".into(),
        order_id: "ord:shop.example:01JORDER".into(),
        customer_id: "customer:shop.example:01JCUSTOMER".into(),
        seller_id: "seller:shop.example:01JSELLER".into(),
        offer_id: "offer:shop.example:01JOFFER".into(),
        offer_revision: 1,
        price: json!({"amount": "25.00", "currency": "USDC"}),
        payment_adapter: "evm_escrow".into(),
        payment_capture_policy: "before_entitlement".into(),
        chain_id: 31337,
        token_contract: "0x0000000000000000000000000000000000000002".into(),
        amount_units: "25000000".into(),
        escrow_contract: "0x0000000000000000000000000000000000000001".into(),
        seller_evm_address: "0x0000000000000000000000000000000000000003".into(),
        buyer_evm_address: "0x0000000000000000000000000000000000000004".into(),
        arbiter_actor: "arbiter:shop.example:01JARBITER".into(),
        arbiter_evm_address: "0x0000000000000000000000000000000000000005".into(),
    }
}

#[test]
fn order_hash_is_deterministic_for_locked_terms() {
    let input = locked_terms_input();

    let left = compute_order_hash(&input).unwrap();
    let right = compute_order_hash(&input).unwrap();

    assert_eq!(left, right);
    assert!(left.starts_with("0x"));
    assert_eq!(left.len(), 66);
}

#[test]
fn order_hash_pins_canonical_fields_and_lowercases_evm_addresses() {
    let mut lower_case = locked_terms_input();
    lower_case.token_contract = "0x000000000000000000000000000000000000000a".into();
    lower_case.escrow_contract = "0x000000000000000000000000000000000000000b".into();
    lower_case.seller_evm_address = "0x000000000000000000000000000000000000000c".into();
    lower_case.buyer_evm_address = "0x000000000000000000000000000000000000000d".into();
    lower_case.arbiter_evm_address = "0x000000000000000000000000000000000000000e".into();
    let lower_hash = compute_order_hash(&lower_case).unwrap();

    let mut mixed_case = lower_case.clone();
    mixed_case.token_contract = "0x000000000000000000000000000000000000000A".into();
    mixed_case.escrow_contract = "0x000000000000000000000000000000000000000B".into();
    mixed_case.seller_evm_address = "0x000000000000000000000000000000000000000C".into();
    mixed_case.buyer_evm_address = "0x000000000000000000000000000000000000000D".into();
    mixed_case.arbiter_evm_address = "0x000000000000000000000000000000000000000E".into();

    assert_eq!(compute_order_hash(&mixed_case).unwrap(), lower_hash);
    assert_eq!(
        lower_hash,
        "0x5978eba3efc797dfeb1b78b080b4adc0e664ad37d3df3ad2190fa8dd579b82fd"
    );
}
