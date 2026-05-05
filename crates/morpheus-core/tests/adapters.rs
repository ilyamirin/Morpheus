use morpheus_core::{MockPaymentAdapter, PaymentAdapter, validate_entitlement_secret_safety};

#[test]
fn mock_payment_adapter_emits_deterministic_provider_refs() {
    let adapter = MockPaymentAdapter;
    let intent = adapter.create_intent("ord:customer.example:01JORDER", "100.00", "USD");
    assert_eq!(
        intent.payment_id,
        "pay:mock.example:ord_customer.example_01JORDER"
    );
    assert_eq!(intent.provider_ref, "mock_pi_ord_customer.example_01JORDER");

    let capture = adapter.capture(&intent.payment_id);
    assert_eq!(
        capture.provider_ref,
        "mock_ch_pay_mock.example_ord_customer.example_01JORDER"
    );
}

#[test]
fn entitlement_metadata_rejects_bearer_urls() {
    let err = validate_entitlement_secret_safety(
        "https://files.example/download?token=secret-bearer-value",
    )
    .expect_err("bearer URLs must not be recorded in marketplace events");
    assert!(err.message.contains("secret"));
}
