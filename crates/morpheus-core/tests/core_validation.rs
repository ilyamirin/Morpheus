use morpheus_core::{
    AllowlistPolicy, CatalogIndex, CustomerBinding, Money, OfferRecord, OrderAuthorities,
    OrderState, SellerRecord, SnapshotRecord, assert_event_authority, validate_order_created,
    validate_order_sequence,
};
use morpheus_protocol::ValidationCode;

#[test]
fn catalog_rejects_revision_rollback() {
    let mut catalog = CatalogIndex::new("shop.example");
    catalog
        .apply_snapshot(SnapshotRecord {
            snapshot_id: "snap_01J".into(),
            sequence: 1,
            sha256: "sha256:abc".into(),
            covers_events_until: "$a".into(),
        })
        .unwrap();
    catalog
        .upsert_seller(SellerRecord {
            seller_id: "seller:shop.example:01JSELLER".into(),
            status: "active".into(),
        })
        .unwrap();
    catalog
        .upsert_product(
            "prod:shop.example:01JPROD",
            "seller:shop.example:01JSELLER",
            7,
        )
        .unwrap();
    catalog
        .upsert_offer(OfferRecord {
            offer_id: "offer:shop.example:01JOFFER".into(),
            product_id: "prod:shop.example:01JPROD".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            revision: 3,
            price: Money {
                amount: "100.00".into(),
                currency: "USD".into(),
            },
            entitlement_type: "booking_slot".into(),
        })
        .unwrap();

    let err = catalog
        .upsert_offer(OfferRecord {
            offer_id: "offer:shop.example:01JOFFER".into(),
            product_id: "prod:shop.example:01JPROD".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            revision: 2,
            price: Money {
                amount: "100.00".into(),
                currency: "USD".into(),
            },
            entitlement_type: "booking_slot".into(),
        })
        .expect_err("rollback rejected");
    assert_eq!(err.code, ValidationCode::RevisionRollback);
}

#[test]
fn validates_complete_order_sequence() {
    let events = morpheus_core::fixtures::valid_order_flow();
    let decision = validate_order_sequence(&events).expect("happy path order");
    assert_eq!(decision.final_state, OrderState::Completed);
}

#[test]
fn rejects_unauthorized_payment_sender() {
    let err = assert_event_authority(
        "io.marketplace.payment.captured",
        "@market:customer.example",
        &OrderAuthorities {
            seller_as_user: "@market:shop.example".into(),
            customer_as_user: "@market:customer.example".into(),
            arbiter_as_user: "@market:arbiter.example".into(),
            payment_as_users: vec![],
        },
    )
    .expect_err("customer cannot capture payment");
    assert_eq!(err.code, ValidationCode::UnauthorizedSender);
}

#[test]
fn validates_order_terms_against_catalog_and_allowlist() {
    let catalog = morpheus_core::fixtures::valid_catalog();
    let allowlist = AllowlistPolicy::new([
        ("shop.example".to_string(), vec!["orders".to_string()]),
        (
            "arbiter.example".to_string(),
            vec!["arbitration".to_string()],
        ),
    ]);
    let customer = CustomerBinding {
        customer_id: "customer:customer.example:01JCUST".into(),
        status: "active".into(),
        accepted_payment_adapters: vec!["mock".into()],
        accepted_arbitration_policies: vec!["standard-digital-v1".into()],
    };

    validate_order_created(
        &morpheus_core::fixtures::valid_order_created(),
        &catalog,
        &allowlist,
        &customer,
    )
    .expect("order terms match catalog");
}
