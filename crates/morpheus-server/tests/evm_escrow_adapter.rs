use morpheus_protocol::{ValidationCode, validate_event_envelope};
use morpheus_server::evm_escrow::{
    DecodedEscrowLog, EvmEscrowIntentInput, ExpectedEscrowPayment, compute_order_hash,
    decode_rpc_log, escrow_event_topics, map_escrow_log_to_payment_event, verify_decoded_log,
};
use morpheus_server::evm_rpc::RpcLog;
use serde_json::{Value, json};

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

fn deposited_log_fixture() -> DecodedEscrowLog {
    DecodedEscrowLog {
        event_name: "EscrowDeposited".into(),
        order_hash: "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
        tx_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        log_index: 0,
        block_number: 10,
        block_hash: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        chain_id: 31337,
        escrow_contract: "0x0000000000000000000000000000000000000001".into(),
        token: "0x0000000000000000000000000000000000000002".into(),
        amount: "25000000".into(),
        buyer: Some("0x0000000000000000000000000000000000000004".into()),
        seller: Some("0x0000000000000000000000000000000000000003".into()),
        buyer_amount: None,
        seller_amount: None,
    }
}

fn expected_payment_fixture() -> ExpectedEscrowPayment {
    ExpectedEscrowPayment {
        order_hash: "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
        chain_id: 31337,
        escrow_contract: "0x0000000000000000000000000000000000000001".into(),
        token: "0x0000000000000000000000000000000000000002".into(),
        amount: "25000000".into(),
        buyer: "0x0000000000000000000000000000000000000004".into(),
        seller: "0x0000000000000000000000000000000000000003".into(),
    }
}

fn protocol_event(event_type: &str, body: Value) -> Value {
    json!({
        "type": event_type,
        "room_id": "!order:shop.example",
        "event_id": format!(
            "$matrix-{}",
            event_type
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .collect::<String>()
        ),
        "sender": "@market:shop.example",
        "origin_server_ts": 1_777_888_000_000i64,
        "content": {
            "protocol": "io.marketplace",
            "protocol_version": "0.1",
            "protocol_event_id": format!(
                "evt:shop.example:{}",
                event_type
                    .chars()
                    .filter(|ch| ch.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_uppercase()
            ),
            "created_at": "2026-05-04T10:00:00Z",
            "issuer": {
                "instance_id": "shop.example",
                "actor_id": "seller:shop.example:01JSELLER",
                "matrix_user_id": "@market:shop.example"
            },
            "critical": [],
            "body": body
        }
    })
}

fn assert_protocol_valid(event_type: &str, body: Value) {
    validate_event_envelope(&protocol_event(event_type, body)).unwrap();
}

fn assert_evm_log_evidence(body: &Value, event_name: &str) {
    let evidence = &body["evidence"];
    assert_eq!(evidence["kind"], "evm_escrow_log");
    assert_eq!(evidence["log"]["event_name"], event_name);
    assert_eq!(
        evidence["log"]["tx_hash"],
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert!(
        evidence["uri"]
            .as_str()
            .unwrap()
            .starts_with("https://evidence.morpheus.local/evm/31337/tx/")
    );
    let sha256 = evidence["sha256"].as_str().unwrap();
    assert!(sha256.starts_with("sha256:"));
    assert_eq!(sha256.len(), 71);
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

#[test]
fn deposited_log_maps_to_payment_authorized() {
    let log = deposited_log_fixture();

    let mapped = map_escrow_log_to_payment_event(
        "ord:shop.example:01JORDER",
        "pay:shop.example:01JPAY",
        "25.00",
        "USDC",
        &log,
    )
    .unwrap();

    assert_eq!(mapped.event_type, "io.marketplace.payment.authorized");
    assert_eq!(mapped.body["order_id"], "ord:shop.example:01JORDER");
    assert_eq!(mapped.body["payment_id"], "pay:shop.example:01JPAY");
}

#[test]
fn released_log_maps_to_protocol_valid_payment_captured() {
    let mut log = deposited_log_fixture();
    log.event_name = "EscrowReleased".into();
    log.amount = "25.00".into();

    let mapped = map_escrow_log_to_payment_event(
        "ord:shop.example:01JORDER",
        "pay:shop.example:01JPAY",
        "25.00",
        "USDC",
        &log,
    )
    .unwrap();

    assert_eq!(mapped.event_type, "io.marketplace.payment.captured");
    assert_eq!(mapped.body["adapter"], "evm_escrow");
    assert_eq!(mapped.body["amount"], "25.00");
    assert_eq!(mapped.body["currency"], "USDC");
    assert_evm_log_evidence(&mapped.body, "EscrowReleased");
    assert_protocol_valid(&mapped.event_type, mapped.body);
}

#[test]
fn refunded_log_maps_to_protocol_valid_payment_refunded() {
    let mut log = deposited_log_fixture();
    log.event_name = "EscrowRefunded".into();
    log.amount = "25.00".into();

    let mapped = map_escrow_log_to_payment_event(
        "ord:shop.example:01JORDER",
        "pay:shop.example:01JPAY",
        "25.00",
        "USDC",
        &log,
    )
    .unwrap();

    assert_eq!(mapped.event_type, "io.marketplace.payment.refunded");
    assert_eq!(mapped.body["refund_id"].as_str().unwrap().len(), 81);
    assert!(
        mapped.body["refund_id"]
            .as_str()
            .unwrap()
            .starts_with("refund:evm.local:")
    );
    assert_eq!(mapped.body["amount"], "25.00");
    assert_eq!(mapped.body["currency"], "USDC");
    assert_evm_log_evidence(&mapped.body, "EscrowRefunded");
    assert_protocol_valid(&mapped.event_type, mapped.body);
}

#[test]
fn refund_id_changes_with_log_index() {
    let mut first_log = deposited_log_fixture();
    first_log.event_name = "EscrowRefunded".into();
    first_log.amount = "25.00".into();

    let mut second_log = first_log.clone();
    second_log.log_index = 1;

    let first = map_escrow_log_to_payment_event(
        "ord:shop.example:01JORDER",
        "pay:shop.example:01JPAY",
        "25.00",
        "USDC",
        &first_log,
    )
    .unwrap();
    let second = map_escrow_log_to_payment_event(
        "ord:shop.example:01JORDER",
        "pay:shop.example:01JPAY",
        "25.00",
        "USDC",
        &second_log,
    )
    .unwrap();

    assert_ne!(first.body["refund_id"], second.body["refund_id"]);
    assert_ne!(first.body["provider_ref"], second.body["provider_ref"]);
}

#[test]
fn partial_refund_uses_buyer_amount_and_protocol_valid_evidence() {
    let mut log = deposited_log_fixture();
    log.event_name = "EscrowPartiallyRefunded".into();
    log.amount = "25.00".into();
    log.buyer_amount = Some("10.00".into());
    log.seller_amount = Some("15.00".into());

    let mapped = map_escrow_log_to_payment_event(
        "ord:shop.example:01JORDER",
        "pay:shop.example:01JPAY",
        "25.00",
        "USDC",
        &log,
    )
    .unwrap();

    assert_eq!(mapped.event_type, "io.marketplace.payment.refunded");
    assert_eq!(mapped.body["amount"], "10.00");
    assert_eq!(mapped.body["evidence"]["log"]["seller_amount"], "15.00");
    assert_evm_log_evidence(&mapped.body, "EscrowPartiallyRefunded");
    assert_protocol_valid(&mapped.event_type, mapped.body);
}

#[test]
fn watcher_accepts_matching_deposit_log() {
    let expected = expected_payment_fixture();
    let log = deposited_log_fixture();

    verify_decoded_log(&expected, &log).unwrap();
}

#[test]
fn watcher_accepts_case_insensitive_evm_identifiers() {
    let mut expected = expected_payment_fixture();
    expected.order_hash = expected.order_hash.to_ascii_uppercase();
    expected.escrow_contract = "0x000000000000000000000000000000000000000A".into();
    expected.token = "0x000000000000000000000000000000000000000B".into();
    expected.buyer = "0x000000000000000000000000000000000000000C".into();
    expected.seller = "0x000000000000000000000000000000000000000D".into();
    let mut log = deposited_log_fixture();
    log.escrow_contract = "0x000000000000000000000000000000000000000a".into();
    log.token = "0x000000000000000000000000000000000000000b".into();
    log.buyer = Some("0x000000000000000000000000000000000000000c".into());
    log.seller = Some("0x000000000000000000000000000000000000000d".into());

    verify_decoded_log(&expected, &log).unwrap();
}

#[test]
fn watcher_rejects_amount_mismatch() {
    let expected = expected_payment_fixture();
    let mut log = deposited_log_fixture();
    log.amount = "24000000".into();

    let err = verify_decoded_log(&expected, &log).unwrap_err();
    assert_eq!(err.code, ValidationCode::PaymentTermsMismatch);
}

#[test]
fn watcher_rejects_participant_mismatch() {
    let expected = expected_payment_fixture();
    let mut log = deposited_log_fixture();
    log.buyer = Some("0x0000000000000000000000000000000000000006".into());

    let err = verify_decoded_log(&expected, &log).unwrap_err();
    assert_eq!(err.code, ValidationCode::PaymentTermsMismatch);
}

#[test]
fn watcher_requires_event_participants() {
    let expected = expected_payment_fixture();

    let mut deposited = deposited_log_fixture();
    deposited.buyer = None;
    let err = verify_decoded_log(&expected, &deposited).unwrap_err();
    assert_eq!(err.code, ValidationCode::PaymentTermsMismatch);

    let mut released = deposited_log_fixture();
    released.event_name = "EscrowReleased".into();
    released.buyer = None;
    verify_decoded_log(&expected, &released).unwrap();
    released.seller = None;
    let err = verify_decoded_log(&expected, &released).unwrap_err();
    assert_eq!(err.code, ValidationCode::PaymentTermsMismatch);

    let mut refunded = deposited_log_fixture();
    refunded.event_name = "EscrowRefunded".into();
    refunded.seller = None;
    verify_decoded_log(&expected, &refunded).unwrap();
    refunded.buyer = None;
    let err = verify_decoded_log(&expected, &refunded).unwrap_err();
    assert_eq!(err.code, ValidationCode::PaymentTermsMismatch);
}

#[test]
fn unsupported_escrow_log_event_is_rejected() {
    let mut log = deposited_log_fixture();
    log.event_name = "EscrowCancelled".into();

    let err = map_escrow_log_to_payment_event(
        "ord:shop.example:01JORDER",
        "pay:shop.example:01JPAY",
        "25.00",
        "USDC",
        &log,
    )
    .unwrap_err();

    assert_eq!(err.code, ValidationCode::UnknownEventType);
}

#[test]
fn refunded_log_rejects_short_tx_hash_without_panicking() {
    let mut log = deposited_log_fixture();
    log.event_name = "EscrowRefunded".into();
    log.tx_hash = "0xabc".into();

    let err = map_escrow_log_to_payment_event(
        "ord:shop.example:01JORDER",
        "pay:shop.example:01JPAY",
        "25.00",
        "USDC",
        &log,
    )
    .unwrap_err();

    assert_eq!(err.code, ValidationCode::InvalidId);
}

#[test]
fn decodes_deposited_rpc_log() {
    let topics = escrow_event_topics();
    let log = RpcLog {
        address: "0x0000000000000000000000000000000000000001".into(),
        block_hash: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        block_number: 10,
        transaction_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .into(),
        log_index: 0,
        topics: vec![
            topics.deposited.clone(),
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
    };

    let decoded = decode_rpc_log(31337, &log).unwrap();

    assert_eq!(decoded.event_name, "EscrowDeposited");
    assert_eq!(
        decoded.order_hash,
        "0x1111111111111111111111111111111111111111111111111111111111111111"
    );
    assert_eq!(decoded.token, "0x0000000000000000000000000000000000000002");
    assert_eq!(decoded.amount, "25000000");
    assert_eq!(
        decoded.buyer.as_deref(),
        Some("0x0000000000000000000000000000000000000004")
    );
    assert_eq!(
        decoded.seller.as_deref(),
        Some("0x0000000000000000000000000000000000000003")
    );
}
