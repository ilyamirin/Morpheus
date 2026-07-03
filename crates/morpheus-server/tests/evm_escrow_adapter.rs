use morpheus_server::evm_escrow::{compute_order_hash, EvmEscrowIntentInput};
use serde_json::json;

#[test]
fn order_hash_is_deterministic_for_locked_terms() {
    let input = EvmEscrowIntentInput {
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
    };

    let left = compute_order_hash(&input).unwrap();
    let right = compute_order_hash(&input).unwrap();

    assert_eq!(left, right);
    assert!(left.starts_with("0x"));
    assert_eq!(left.len(), 66);
}
