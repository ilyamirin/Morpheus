use morpheus_core::*;
use morpheus_protocol::{ValidationCode, ValidationResult};
use serde_json::{Value, json};

fn assert_code(result: ValidationResult<impl Sized>, code: ValidationCode) {
    match result {
        Ok(_) => panic!("expected rejection"),
        Err(err) => assert_eq!(err.code, code),
    }
}

fn catalog() -> CatalogIndex {
    morpheus_core::fixtures::valid_catalog()
}

fn snapshot_document() -> CatalogSnapshotDocument {
    CatalogSnapshotDocument {
        snapshot: SnapshotRecord {
            snapshot_id: "snap:shop.example:01JSNAP".into(),
            sequence: 1,
            sha256: "sha256:d8a36551d737917dac04aba6c89512fa6f2a019d641e565438d4f10867257add"
                .into(),
            covers_events_until: "$snap".into(),
        },
        sellers: vec![SellerRecord {
            seller_id: "seller:shop.example:01JSELLER".into(),
            status: "active".into(),
        }],
        products: vec![ProductRecord {
            product_id: "prod:shop.example:01JPROD".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            revision: 1,
            terms_hash: None,
        }],
        offers: vec![
            catalog()
                .get_offer("offer:shop.example:01JOFFER")
                .unwrap()
                .clone(),
        ],
        tombstones: vec![],
        sequence: 1,
        covers_events_until: "$snap".into(),
    }
}

fn order() -> OrderCreatedBody {
    morpheus_core::fixtures::valid_order_created()
}

fn customer() -> CustomerBinding {
    morpheus_core::fixtures::valid_customer()
}

fn allowlist() -> AllowlistPolicy {
    morpheus_core::fixtures::order_allowlist()
}

fn authorities() -> OrderAuthorities {
    OrderAuthorities {
        seller_as_user: "@market:shop.example".into(),
        customer_as_user: "@market:customer.example".into(),
        arbiter_as_user: "@market:arbiter.example".into(),
        payment_as_users: vec!["@payment:shop.example".into()],
    }
}

#[test]
fn catalog_index_accepts_and_retrieves_active_offer() {
    let catalog = catalog();
    assert_eq!(
        catalog
            .get_offer("offer:shop.example:01JOFFER")
            .unwrap()
            .revision,
        3
    );
}

#[test]
fn catalog_index_rejects_suspended_unknown_and_mismatched_sellers() {
    let mut local_catalog = CatalogIndex::new("shop.example");
    local_catalog
        .apply_snapshot(SnapshotRecord {
            snapshot_id: "snap:shop.example:01JSNAP".into(),
            sequence: 1,
            sha256: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            covers_events_until: "$snap".into(),
        })
        .unwrap();
    local_catalog
        .upsert_seller(SellerRecord {
            seller_id: "seller:shop.example:01JSELLER".into(),
            status: "suspended".into(),
        })
        .unwrap();
    assert_code(
        local_catalog.upsert_product(ProductRecord {
            product_id: "prod:shop.example:01JPROD".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            revision: 1,
            terms_hash: None,
        }),
        ValidationCode::ActorNotActive,
    );

    let mut active_catalog = catalog();
    let mut offer = active_catalog
        .get_offer("offer:shop.example:01JOFFER")
        .unwrap()
        .clone();
    offer.offer_id = "offer:shop.example:01JOTHER".into();
    offer.product_id = "prod:shop.example:01JMISSING".into();
    assert_code(
        active_catalog.upsert_offer(offer),
        ValidationCode::CatalogReferenceMismatch,
    );
}

#[test]
fn catalog_index_rejects_revision_rollbacks_and_hash_mismatch() {
    let mut catalog = catalog();
    let mut stale = catalog
        .get_offer("offer:shop.example:01JOFFER")
        .unwrap()
        .clone();
    stale.revision = 2;
    assert_code(
        catalog.upsert_offer(stale),
        ValidationCode::RevisionRollback,
    );

    assert_code(
        catalog.apply_snapshot(SnapshotRecord {
            snapshot_id: "snap:shop.example:01JSNAP".into(),
            sequence: 1,
            sha256: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .into(),
            covers_events_until: "$snap".into(),
        }),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut duplicate_snapshot_catalog = CatalogIndex::new("shop.example");
    let same_snapshot = SnapshotRecord {
        snapshot_id: "snap:shop.example:01JSNAP".into(),
        sequence: 1,
        sha256: "sha256:d8a36551d737917dac04aba6c89512fa6f2a019d641e565438d4f10867257add".into(),
        covers_events_until: "$snap".into(),
    };
    duplicate_snapshot_catalog
        .apply_snapshot(same_snapshot.clone())
        .unwrap();
    assert!(
        duplicate_snapshot_catalog
            .apply_snapshot(same_snapshot)
            .is_ok()
    );
    duplicate_snapshot_catalog
        .apply_snapshot(SnapshotRecord {
            snapshot_id: "snap:shop.example:01JSNAP2".into(),
            sequence: 2,
            sha256: "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                .into(),
            covers_events_until: "$snap2".into(),
        })
        .unwrap();
    assert_code(
        catalog.apply_snapshot(SnapshotRecord {
            snapshot_id: "snap:shop.example:01JOLD".into(),
            sequence: 0,
            sha256: "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                .into(),
            covers_events_until: "$old".into(),
        }),
        ValidationCode::RevisionRollback,
    );

    let mut product_stale = ProductRecord {
        product_id: "prod:shop.example:01JPROD".into(),
        seller_id: "seller:shop.example:01JSELLER".into(),
        revision: 1,
        terms_hash: None,
    };
    assert_code(
        catalog.upsert_product(product_stale.clone()),
        ValidationCode::RevisionRollback,
    );
    product_stale.product_id = "prod:other.example:01JPROD".into();
    product_stale.revision = 10;
    assert_code(
        catalog.upsert_product(product_stale),
        ValidationCode::CatalogReferenceMismatch,
    );
}

#[test]
fn catalog_remove_object_withdraws_offer_and_product_offers() {
    let mut offer_catalog = catalog();
    offer_catalog.remove_object("offer:shop.example:01JOFFER");
    assert!(
        offer_catalog
            .get_offer("offer:shop.example:01JOFFER")
            .is_none()
    );

    let mut product_catalog = catalog();
    product_catalog.remove_object("prod:shop.example:01JPROD");
    assert_eq!(product_catalog.offer_count(), 0);
}

#[test]
fn replay_catalog_timeline_deduplicates_and_checks_sequence() {
    let snapshot = snapshot_document();
    let delta = CatalogDeltaEvent {
        event_type: "io.marketplace.offer.withdrawn".into(),
        event_id: "$withdraw".into(),
        catalog_sequence: 2,
        body: json!({"offer_id": "offer:shop.example:01JOFFER", "revision": 4}),
    };
    let replayed = replay_catalog_timeline(
        "shop.example",
        snapshot.clone(),
        &[delta.clone(), delta.clone()],
    )
    .unwrap();
    assert!(replayed.get_offer("offer:shop.example:01JOFFER").is_none());

    let mut bad = delta;
    bad.catalog_sequence = 4;
    assert_code(
        replay_catalog_timeline("shop.example", snapshot, &[bad]),
        ValidationCode::CatalogReferenceMismatch,
    );
}

#[test]
fn replay_catalog_timeline_applies_all_delta_kinds_and_rejects_bad_snapshots() {
    let snapshot = snapshot_document();
    let product = CatalogDeltaEvent {
        event_type: "io.marketplace.product.upserted".into(),
        event_id: "$product2".into(),
        catalog_sequence: 2,
        body: json!({
            "product_id": "prod:shop.example:01JPROD2",
            "seller_id": "seller:shop.example:01JSELLER",
            "revision": 1,
            "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }),
    };
    let offer = CatalogDeltaEvent {
        event_type: "io.marketplace.offer.upserted".into(),
        event_id: "$offer2".into(),
        catalog_sequence: 3,
        body: json!({
            "offer_id": "offer:shop.example:01JOFFER2",
            "product_id": "prod:shop.example:01JPROD2",
            "seller_id": "seller:shop.example:01JSELLER",
            "revision": 1,
            "price": {"amount": "10.00", "currency": "USD"},
            "payment_terms": {"capture_policy": "before_entitlement"},
            "entitlement": {"type": "external_entitlement"},
            "offer_terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "seller_terms_hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }),
    };
    let product_withdrawn = CatalogDeltaEvent {
        event_type: "io.marketplace.product.withdrawn".into(),
        event_id: "$product-withdrawn".into(),
        catalog_sequence: 4,
        body: json!({"product_id": "prod:shop.example:01JPROD2"}),
    };
    let ignored = CatalogDeltaEvent {
        event_type: "io.marketplace.unknown".into(),
        event_id: "$ignored".into(),
        catalog_sequence: 5,
        body: json!({}),
    };
    let replayed = replay_catalog_timeline(
        "shop.example",
        snapshot.clone(),
        &[product, offer, product_withdrawn, ignored],
    )
    .unwrap();
    assert!(replayed.get_offer("offer:shop.example:01JOFFER2").is_none());

    let mut invalid_snapshot = snapshot;
    invalid_snapshot.snapshot.snapshot_id = "bad".into();
    assert_code(
        validate_catalog_snapshot(&invalid_snapshot, &invalid_snapshot.snapshot.sha256),
        ValidationCode::InvalidId,
    );
}

#[test]
fn allowlist_capabilities_status_expiry_and_audit_are_enforced() {
    let policy = AllowlistPolicy::from_entries([
        (
            "shop.example".into(),
            AllowlistEntry {
                capabilities: vec![AllowlistCapability::Catalog, AllowlistCapability::Indexing],
                status: "active".into(),
                valid_until_epoch_ms: Some(2_000),
                audit_reason: Some("vendor onboarding".into()),
                updated_by: Some("@admin:shop.example".into()),
                updated_at: Some("2026-05-04T10:00:00Z".into()),
            },
        ),
        (
            "old.example".into(),
            AllowlistEntry {
                capabilities: vec![AllowlistCapability::Orders],
                status: "revoked".into(),
                valid_until_epoch_ms: None,
                audit_reason: None,
                updated_by: None,
                updated_at: None,
            },
        ),
    ]);
    assert!(policy.can_at("shop.example", "catalog", 1_000));
    assert!(!policy.can_at("shop.example", "catalog", 2_000));
    assert!(policy.can_replay_existing_order("old.example"));
    validate_allowlist_policy(&policy, 1_000).unwrap();
}

#[test]
fn allowlist_rejects_expired_active_entries_and_bad_audit() {
    let expired = AllowlistPolicy::from_entries([(
        "shop.example".into(),
        AllowlistEntry {
            capabilities: vec![AllowlistCapability::Catalog],
            status: "active".into(),
            valid_until_epoch_ms: Some(1_000),
            audit_reason: Some("expired".into()),
            updated_by: Some("@admin:shop.example".into()),
            updated_at: None,
        },
    )]);
    assert_code(
        validate_allowlist_policy(&expired, 1_000),
        ValidationCode::PolicyViolation,
    );

    let bad = AllowlistPolicy::from_entries([(
        "shop.example".into(),
        AllowlistEntry {
            capabilities: vec![],
            status: "active".into(),
            valid_until_epoch_ms: None,
            audit_reason: Some("".into()),
            updated_by: Some("not-matrix".into()),
            updated_at: None,
        },
    )]);
    assert_code(
        validate_allowlist_policy(&bad, 0),
        ValidationCode::PolicyViolation,
    );

    let empty_instance = AllowlistPolicy::from_entries([(
        "".into(),
        AllowlistEntry {
            capabilities: vec![AllowlistCapability::Orders],
            status: "active".into(),
            valid_until_epoch_ms: None,
            audit_reason: Some("bad".into()),
            updated_by: Some("@admin:shop.example".into()),
            updated_at: None,
        },
    )]);
    assert_code(
        validate_allowlist_policy(&empty_instance, 0),
        ValidationCode::PolicyViolation,
    );
}

#[test]
fn order_created_validates_catalog_customer_allowlist_and_terms() {
    validate_order_created(&order(), &catalog(), &allowlist(), &customer()).unwrap();

    let mut stale = order();
    stale.offer_revision = 1;
    assert_code(
        validate_order_created(&stale, &catalog(), &allowlist(), &customer()),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut price = order();
    price.price.currency = "EUR".into();
    assert_code(
        validate_order_created(&price, &catalog(), &allowlist(), &customer()),
        ValidationCode::PaymentTermsMismatch,
    );

    let mut quantity = order();
    quantity.quantity = 2;
    assert_code(
        validate_order_created(&quantity, &catalog(), &allowlist(), &customer()),
        ValidationCode::PaymentTermsMismatch,
    );
}

#[test]
fn order_created_rejects_customer_and_arbiter_policy_mismatches() {
    let mut suspended_customer = customer();
    suspended_customer.status = "suspended".into();
    assert_code(
        validate_order_created(&order(), &catalog(), &allowlist(), &suspended_customer),
        ValidationCode::ActorNotActive,
    );

    let mut adapter_customer = customer();
    adapter_customer.accepted_payment_adapters = vec!["other".into()];
    assert_code(
        validate_order_created(&order(), &catalog(), &allowlist(), &adapter_customer),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut arbiter_mismatch = order();
    arbiter_mismatch.arbiter_actor = "arbiter:other.example:01JARB".into();
    assert_code(
        validate_order_created(&arbiter_mismatch, &catalog(), &allowlist(), &customer()),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut customer_mismatch = customer();
    customer_mismatch.customer_id = "customer:customer.example:01JOTHER".into();
    assert_code(
        validate_order_created(&order(), &catalog(), &allowlist(), &customer_mismatch),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut arbitration_customer = customer();
    arbitration_customer.accepted_arbitration_policies = vec!["other".into()];
    assert_code(
        validate_order_created(&order(), &catalog(), &allowlist(), &arbitration_customer),
        ValidationCode::CatalogReferenceMismatch,
    );

    let denied = AllowlistPolicy::default();
    assert_code(
        validate_order_created(&order(), &catalog(), &denied, &customer()),
        ValidationCode::InstanceNotAllowlisted,
    );

    let arbiter_denied = AllowlistPolicy::from_entries([(
        "shop.example".into(),
        AllowlistEntry {
            capabilities: vec![AllowlistCapability::Orders],
            status: "active".into(),
            valid_until_epoch_ms: None,
            audit_reason: Some("seller".into()),
            updated_by: Some("@admin:shop.example".into()),
            updated_at: None,
        },
    )]);
    assert_code(
        validate_order_created(&order(), &catalog(), &arbiter_denied, &customer()),
        ValidationCode::InstanceNotAllowlisted,
    );

    let mut missing_offer = order();
    missing_offer.offer_id = "offer:shop.example:01JMISSING".into();
    assert_code(
        validate_order_created(&missing_offer, &catalog(), &allowlist(), &customer()),
        ValidationCode::CatalogReferenceMismatch,
    );
}

#[test]
fn indexing_policy_requires_catalog_and_indexing_capabilities() {
    assert!(should_index_catalog_room(&allowlist(), "shop.example"));
    let policy = AllowlistPolicy::new([("shop.example".into(), vec!["catalog".into()])]);
    assert!(!should_index_catalog_room(&policy, "shop.example"));
}

#[test]
fn authority_allows_only_expected_order_parties() {
    assert_event_authority(
        "io.marketplace.order.created",
        "@market:customer.example",
        &authorities(),
    )
    .unwrap();
    assert_event_authority(
        "io.marketplace.order.cancelled",
        "@market:shop.example",
        &authorities(),
    )
    .unwrap();
    assert_event_authority(
        "io.marketplace.order.accepted",
        "@market:shop.example",
        &authorities(),
    )
    .unwrap();
    assert_event_authority(
        "io.marketplace.payment.captured",
        "@payment:shop.example",
        &authorities(),
    )
    .unwrap();
    assert_event_authority(
        "io.marketplace.entitlement.granted",
        "@market:shop.example",
        &authorities(),
    )
    .unwrap();
    assert_event_authority(
        "io.marketplace.dispute.opened",
        "@market:arbiter.example",
        &authorities(),
    )
    .unwrap();
    assert_event_authority(
        "io.marketplace.dispute.ruling.issued",
        "@market:arbiter.example",
        &authorities(),
    )
    .unwrap();

    assert_code(
        assert_event_authority(
            "io.marketplace.payment.captured",
            "@payment:pay.example",
            &authorities(),
        ),
        ValidationCode::UnauthorizedSender,
    );
    assert_code(
        assert_event_authority(
            "io.marketplace.order.accepted",
            "@market:customer.example",
            &authorities(),
        ),
        ValidationCode::UnauthorizedSender,
    );
    assert_code(
        assert_event_authority(
            "io.marketplace.dispute.closed",
            "@market:shop.example",
            &authorities(),
        ),
        ValidationCode::UnauthorizedSender,
    );
}

#[test]
fn transition_graph_accepts_happy_optional_and_dispute_paths() {
    let mut graph = OrderTransitionGraph::default();
    for event in [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.entitlement.granted",
        "io.marketplace.entitlement.activated",
        "io.marketplace.entitlement.completed",
        "io.marketplace.order.completed",
    ] {
        graph.apply(event).unwrap();
    }
    assert_eq!(graph.state, OrderState::Completed);

    let mut graph = OrderTransitionGraph::default();
    graph.apply("io.marketplace.order.created").unwrap();
    graph.apply("io.marketplace.order.accepted").unwrap();
    graph.apply("io.marketplace.dispute.opened").unwrap();
    graph
        .apply("io.marketplace.dispute.evidence.submitted")
        .unwrap();
    graph.apply("io.marketplace.dispute.ruling.issued").unwrap();
    graph.apply("io.marketplace.dispute.closed").unwrap();
    assert_eq!(graph.state, OrderState::DisputeResolved);
}

#[test]
fn transition_graph_rejects_invalid_lifecycle_edges() {
    let mut graph = OrderTransitionGraph::default();
    assert_code(
        graph.apply("io.marketplace.entitlement.granted"),
        ValidationCode::InvalidStateTransition,
    );
    graph.apply("io.marketplace.order.created").unwrap();
    graph.apply("io.marketplace.order.accepted").unwrap();
    graph
        .apply("io.marketplace.payment.intent.created")
        .unwrap();
    graph.apply("io.marketplace.payment.authorized").unwrap();
    graph.apply("io.marketplace.payment.captured").unwrap();
    graph.apply("io.marketplace.entitlement.granted").unwrap();
    graph.apply("io.marketplace.order.completed").unwrap();
    assert_code(
        graph.apply("io.marketplace.payment.refunded"),
        ValidationCode::InvalidStateTransition,
    );
}

#[test]
fn transition_graph_covers_all_valid_reference_edges() {
    fn reaches(start: &[&str], event: &str, expected: OrderState) {
        let mut graph = OrderTransitionGraph::default();
        for prior in start {
            graph.apply(prior).unwrap();
        }
        graph.apply(event).unwrap();
        assert_eq!(graph.state, expected, "{event}");
    }

    let created = ["io.marketplace.order.created"];
    reaches(
        &created,
        "io.marketplace.order.rejected",
        OrderState::Rejected,
    );
    reaches(
        &created,
        "io.marketplace.order.cancelled",
        OrderState::Cancelled,
    );

    let accepted = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
    ];
    reaches(
        &accepted,
        "io.marketplace.order.cancelled",
        OrderState::Cancelled,
    );
    reaches(
        &accepted,
        "io.marketplace.dispute.opened",
        OrderState::DisputeOpenedPrePayment,
    );

    let intent = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
    ];
    reaches(
        &intent,
        "io.marketplace.payment.failed",
        OrderState::Cancelled,
    );
    reaches(
        &intent,
        "io.marketplace.payment.cancelled",
        OrderState::Cancelled,
    );

    let authorized = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
    ];
    reaches(
        &authorized,
        "io.marketplace.entitlement.granted",
        OrderState::EntitlementGrantedBeforeCapture,
    );
    reaches(
        &authorized,
        "io.marketplace.payment.failed",
        OrderState::Cancelled,
    );

    let captured = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
    ];
    reaches(
        &captured,
        "io.marketplace.dispute.opened",
        OrderState::DisputeOpenedAfterCapture,
    );
    reaches(
        &captured,
        "io.marketplace.payment.refund.requested",
        OrderState::RefundRequested,
    );
    reaches(
        &captured,
        "io.marketplace.payment.refunded",
        OrderState::Refunded,
    );
    reaches(
        &captured,
        "io.marketplace.payment.chargeback.opened",
        OrderState::ChargebackOpened,
    );

    let refund_requested = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.payment.refund.requested",
    ];
    reaches(
        &refund_requested,
        "io.marketplace.payment.refund.requested",
        OrderState::RefundRequested,
    );
    reaches(
        &refund_requested,
        "io.marketplace.payment.refunded",
        OrderState::Refunded,
    );
    reaches(
        &refund_requested,
        "io.marketplace.payment.chargeback.opened",
        OrderState::ChargebackOpened,
    );

    let before_capture = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.entitlement.granted",
    ];
    reaches(
        &before_capture,
        "io.marketplace.dispute.opened",
        OrderState::DisputeOpenedAfterEntitlement,
    );
    reaches(
        &before_capture,
        "io.marketplace.payment.failed",
        OrderState::Cancelled,
    );
    reaches(
        &before_capture,
        "io.marketplace.entitlement.revoked",
        OrderState::Cancelled,
    );
    reaches(
        &before_capture,
        "io.marketplace.payment.chargeback.opened",
        OrderState::ChargebackOpened,
    );

    let granted = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.entitlement.granted",
    ];
    reaches(
        &granted,
        "io.marketplace.dispute.opened",
        OrderState::DisputeOpenedAfterEntitlement,
    );
    reaches(
        &granted,
        "io.marketplace.payment.refund.requested",
        OrderState::RefundRequested,
    );
    reaches(
        &granted,
        "io.marketplace.payment.refunded",
        OrderState::Refunded,
    );
    reaches(
        &granted,
        "io.marketplace.payment.chargeback.opened",
        OrderState::ChargebackOpened,
    );
    reaches(
        &granted,
        "io.marketplace.entitlement.completed",
        OrderState::EntitlementCompleted,
    );
    reaches(
        &granted,
        "io.marketplace.entitlement.expired",
        OrderState::Expired,
    );
    reaches(
        &granted,
        "io.marketplace.entitlement.revoked",
        OrderState::Cancelled,
    );

    let activated = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.entitlement.granted",
        "io.marketplace.entitlement.activated",
    ];
    reaches(
        &activated,
        "io.marketplace.order.completed",
        OrderState::Completed,
    );
    reaches(
        &activated,
        "io.marketplace.dispute.opened",
        OrderState::DisputeOpenedAfterEntitlement,
    );
    reaches(
        &activated,
        "io.marketplace.payment.refund.requested",
        OrderState::RefundRequested,
    );
    reaches(
        &activated,
        "io.marketplace.payment.refunded",
        OrderState::Refunded,
    );
    reaches(
        &activated,
        "io.marketplace.payment.chargeback.opened",
        OrderState::ChargebackOpened,
    );
    reaches(
        &activated,
        "io.marketplace.entitlement.completed",
        OrderState::EntitlementCompleted,
    );
    reaches(
        &activated,
        "io.marketplace.entitlement.expired",
        OrderState::Expired,
    );
    reaches(
        &activated,
        "io.marketplace.entitlement.revoked",
        OrderState::Cancelled,
    );

    let completed_entitlement = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.entitlement.granted",
        "io.marketplace.entitlement.completed",
    ];
    reaches(
        &completed_entitlement,
        "io.marketplace.order.completed",
        OrderState::Completed,
    );
    reaches(
        &completed_entitlement,
        "io.marketplace.dispute.opened",
        OrderState::DisputeOpenedAfterEntitlement,
    );
    reaches(
        &completed_entitlement,
        "io.marketplace.payment.refund.requested",
        OrderState::RefundRequested,
    );
    reaches(
        &completed_entitlement,
        "io.marketplace.payment.refunded",
        OrderState::Refunded,
    );
    reaches(
        &completed_entitlement,
        "io.marketplace.payment.chargeback.opened",
        OrderState::ChargebackOpened,
    );
    reaches(
        &completed_entitlement,
        "io.marketplace.entitlement.expired",
        OrderState::Expired,
    );
    reaches(
        &completed_entitlement,
        "io.marketplace.entitlement.revoked",
        OrderState::Cancelled,
    );

    let dispute_pre = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.dispute.opened",
    ];
    reaches(
        &dispute_pre,
        "io.marketplace.dispute.evidence.submitted",
        OrderState::DisputeOpenedPrePayment,
    );

    let dispute_capture = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.dispute.opened",
    ];
    reaches(
        &dispute_capture,
        "io.marketplace.dispute.evidence.submitted",
        OrderState::DisputeOpenedAfterCapture,
    );

    let dispute_entitlement = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.entitlement.granted",
        "io.marketplace.dispute.opened",
    ];
    reaches(
        &dispute_entitlement,
        "io.marketplace.dispute.evidence.submitted",
        OrderState::DisputeOpenedAfterEntitlement,
    );

    let ruling_pre = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.dispute.opened",
        "io.marketplace.dispute.ruling.issued",
    ];
    reaches(
        &ruling_pre,
        "io.marketplace.dispute.evidence.submitted",
        OrderState::RulingIssuedPrePayment,
    );

    let ruling_capture = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.dispute.opened",
        "io.marketplace.dispute.ruling.issued",
    ];
    reaches(
        &ruling_capture,
        "io.marketplace.dispute.evidence.submitted",
        OrderState::RulingIssuedAfterCapture,
    );
    reaches(
        &ruling_capture,
        "io.marketplace.payment.refund.requested",
        OrderState::RefundRequested,
    );
    reaches(
        &ruling_capture,
        "io.marketplace.payment.refunded",
        OrderState::Refunded,
    );
    reaches(
        &ruling_capture,
        "io.marketplace.payment.chargeback.opened",
        OrderState::ChargebackOpened,
    );
    reaches(
        &ruling_capture,
        "io.marketplace.entitlement.granted",
        OrderState::EntitlementGranted,
    );
    reaches(
        &ruling_capture,
        "io.marketplace.dispute.closed",
        OrderState::DisputeResolved,
    );

    let ruling_entitlement = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.entitlement.granted",
        "io.marketplace.dispute.opened",
        "io.marketplace.dispute.ruling.issued",
    ];
    reaches(
        &ruling_entitlement,
        "io.marketplace.dispute.evidence.submitted",
        OrderState::RulingIssuedAfterEntitlement,
    );
    reaches(
        &ruling_entitlement,
        "io.marketplace.payment.refund.requested",
        OrderState::RefundRequested,
    );
    reaches(
        &ruling_entitlement,
        "io.marketplace.payment.refunded",
        OrderState::Refunded,
    );
    reaches(
        &ruling_entitlement,
        "io.marketplace.payment.chargeback.opened",
        OrderState::ChargebackOpened,
    );
    reaches(
        &ruling_entitlement,
        "io.marketplace.dispute.closed",
        OrderState::DisputeResolved,
    );

    let refunded = [
        "io.marketplace.order.created",
        "io.marketplace.order.accepted",
        "io.marketplace.payment.intent.created",
        "io.marketplace.payment.authorized",
        "io.marketplace.payment.captured",
        "io.marketplace.payment.refunded",
    ];
    reaches(
        &refunded,
        "io.marketplace.payment.chargeback.opened",
        OrderState::ChargebackOpened,
    );
}

#[test]
fn order_flow_validates_payload_consistency() {
    let events = morpheus_core::fixtures::valid_order_flow();
    assert_eq!(
        validate_order_sequence(&events).unwrap().final_state,
        OrderState::Completed
    );

    let mut mismatched = events.clone();
    mismatched[3].body["amount"] = json!("1.00");
    assert_code(
        validate_order_sequence(&mismatched).map(|_| ()),
        ValidationCode::PaymentTermsMismatch,
    );

    let mut mismatched = events.clone();
    mismatched[2].body["offer_revision"] = json!(99);
    assert_code(
        validate_order_sequence(&mismatched).map(|_| ()),
        ValidationCode::PaymentTermsMismatch,
    );
}

#[test]
fn order_flow_requires_customer_binding_and_order_id_consistency() {
    let events = morpheus_core::fixtures::valid_order_flow();
    assert_code(
        validate_order_sequence(&events[1..]).map(|_| ()),
        ValidationCode::CatalogReferenceMismatch,
    );
    let mut mismatched = events.clone();
    mismatched[4].body["order_id"] = json!("ord:customer.example:01JOTHER");
    assert_code(
        validate_order_sequence(&mismatched).map(|_| ()),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut second_create = events.clone();
    let mut duplicate = second_create[1].clone();
    duplicate.body["order_id"] = json!("ord:customer.example:01JOTHER");
    second_create.insert(2, duplicate);
    assert_code(
        validate_order_sequence(&second_create).map(|_| ()),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut customer_after_order = events.clone();
    customer_after_order.insert(2, customer_after_order[0].clone());
    assert_code(
        validate_order_sequence(&customer_after_order).map(|_| ()),
        ValidationCode::InvalidStateTransition,
    );

    let mut inactive_customer = events.clone();
    inactive_customer[0].body["status"] = json!("suspended");
    assert_code(
        validate_order_sequence(&inactive_customer).map(|_| ()),
        ValidationCode::ActorNotActive,
    );

    assert_code(
        validate_order_sequence(&[events[0].clone(), events[4].clone()]).map(|_| ()),
        ValidationCode::InvalidStateTransition,
    );
}

#[test]
fn order_flow_rejects_payment_entitlement_and_dispute_payload_mismatches() {
    let events = morpheus_core::fixtures::valid_order_flow();

    let mut payment_not_accepted = events.clone();
    payment_not_accepted[0].body["accepted_payment_adapters"] = json!(["other"]);
    assert_code(
        validate_order_sequence(&payment_not_accepted).map(|_| ()),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut arbitration_not_accepted = events.clone();
    arbitration_not_accepted[0].body["accepted_arbitration_policies"] = json!(["other"]);
    assert_code(
        validate_order_sequence(&arbitration_not_accepted).map(|_| ()),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut duplicate_intent = events.clone();
    duplicate_intent.insert(4, duplicate_intent[3].clone());
    assert_code(
        validate_order_sequence(&duplicate_intent).map(|_| ()),
        ValidationCode::PaymentTermsMismatch,
    );

    let mut intent_adapter = events.clone();
    intent_adapter[3].body["adapter"] = json!("other");
    assert_code(
        validate_order_sequence(&intent_adapter).map(|_| ()),
        ValidationCode::PaymentTermsMismatch,
    );

    let mut capture_without_auth = events.clone();
    capture_without_auth.remove(4);
    assert_code(
        validate_order_sequence(&capture_without_auth).map(|_| ()),
        ValidationCode::InvalidStateTransition,
    );

    let mut capture_adapter = events.clone();
    capture_adapter[5].body["adapter"] = json!("other");
    assert_code(
        validate_order_sequence(&capture_adapter).map(|_| ()),
        ValidationCode::PaymentTermsMismatch,
    );

    let mut entitlement_payment = events.clone();
    entitlement_payment[6].body["payment_id"] = json!("pay:customer.example:01JOTHER");
    assert_code(
        validate_order_sequence(&entitlement_payment).map(|_| ()),
        ValidationCode::PaymentTermsMismatch,
    );

    let mut entitlement_lifecycle = events.clone();
    entitlement_lifecycle.insert(
        7,
        OrderFlowEvent {
            event_type: "io.marketplace.entitlement.activated".into(),
            body: json!({
                "order_id": "ord:customer.example:01JORDER",
                "entitlement_id": "ent:customer.example:01JOTHER"
            }),
        },
    );
    assert_code(
        validate_order_sequence(&entitlement_lifecycle).map(|_| ()),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut dispute = events.clone();
    dispute.pop();
    dispute.push(OrderFlowEvent {
        event_type: "io.marketplace.dispute.opened".into(),
        body: json!({"order_id": "ord:customer.example:01JORDER", "dispute_id": "disp:arbiter.example:01JDISP"}),
    });
    dispute.push(OrderFlowEvent {
        event_type: "io.marketplace.dispute.evidence.submitted".into(),
        body: json!({"order_id": "ord:customer.example:01JORDER", "dispute_id": "disp:arbiter.example:01JOTHER"}),
    });
    assert_code(
        validate_order_sequence(&dispute).map(|_| ()),
        ValidationCode::CatalogReferenceMismatch,
    );

    let refund_before_capture = vec![
        events[0].clone(),
        events[1].clone(),
        events[2].clone(),
        events[3].clone(),
        events[4].clone(),
        OrderFlowEvent {
            event_type: "io.marketplace.payment.refund.requested".into(),
            body: json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "refund_id": "refund:customer.example:01JREFUND",
                "amount": "100.00",
                "currency": "USD",
                "provider_ref": "mock_rf"
            }),
        },
    ];
    assert_code(
        validate_order_sequence(&refund_before_capture).map(|_| ()),
        ValidationCode::InvalidStateTransition,
    );
}

#[test]
fn order_flow_handles_after_entitlement_capture_policy() {
    let mut events = morpheus_core::fixtures::valid_order_flow();
    events[1].body["payment_capture_policy"] = json!("after_entitlement");
    events[2].body["payment_capture_policy"] = json!("after_entitlement");
    events[3].body["capture_policy"] = json!("after_entitlement");
    events.swap(5, 6);
    assert_eq!(
        validate_order_sequence(&events).unwrap().final_state,
        OrderState::Completed
    );

    let mut missing_entitlement = events.clone();
    missing_entitlement.remove(5);
    assert_code(
        validate_order_sequence(&missing_entitlement).map(|_| ()),
        ValidationCode::InvalidStateTransition,
    );
}

#[test]
fn order_flow_validates_refunds_and_partial_rulings() {
    let mut events = morpheus_core::fixtures::valid_order_flow();
    events.pop();
    events.push(OrderFlowEvent { event_type: "io.marketplace.dispute.opened".into(), body: json!({"order_id": "ord:customer.example:01JORDER", "dispute_id": "disp:arbiter.example:01JDISP"}) });
    events.push(OrderFlowEvent { event_type: "io.marketplace.dispute.evidence.submitted".into(), body: json!({"order_id": "ord:customer.example:01JORDER", "dispute_id": "disp:arbiter.example:01JDISP"}) });
    events.push(OrderFlowEvent { event_type: "io.marketplace.dispute.ruling.issued".into(), body: json!({"order_id": "ord:customer.example:01JORDER", "dispute_id": "disp:arbiter.example:01JDISP", "ruling": "partial_refund_required", "remedy": {"amount": "25.00", "currency": "USD"}}) });
    events.push(OrderFlowEvent { event_type: "io.marketplace.payment.refund.requested".into(), body: json!({"order_id": "ord:customer.example:01JORDER", "payment_id": "pay:customer.example:01JPAY", "refund_id": "refund:customer.example:01JREFUND", "amount": "25.00", "currency": "USD", "provider_ref": "re"}) });
    assert_eq!(
        validate_order_sequence(&events).unwrap().final_state,
        OrderState::RefundRequested
    );

    let mut bad = events.clone();
    bad.last_mut().unwrap().body["amount"] = json!("100.00");
    assert_code(
        validate_order_sequence(&bad).map(|_| ()),
        ValidationCode::PaymentTermsMismatch,
    );
}

fn envelope(event_type: &str, body: Value, sender: &str) -> Value {
    json!({
        "type": event_type,
        "room_id": "!order:customer.example",
        "event_id": format!("${}", event_type.replace('.', "_")),
        "sender": sender,
        "origin_server_ts": 1777898400000i64,
        "content": {
            "protocol": "io.marketplace",
            "protocol_version": "0.1",
            "protocol_event_id": format!("evt:customer.example:{}", event_type.replace('.', "").to_uppercase()),
            "created_at": "2026-05-04T10:00:00Z",
            "issuer": {
                "instance_id": sender.split(':').nth(1).unwrap(),
                "actor_id": if sender.contains("shop.example") { "seller:shop.example:01JSELLER" } else { "customer:customer.example:01JCUST" },
                "matrix_user_id": sender
            },
            "critical": [],
            "body": body
        }
    })
}

#[test]
fn order_room_timeline_validates_membership_authority_and_replay() {
    let events = morpheus_core::fixtures::valid_order_flow();
    let raw = vec![
        envelope(
            "io.marketplace.actor.customer.bound",
            json!({
                "customer_id": "customer:customer.example:01JCUST",
                "status": "active",
                "display_name": "Acme",
                "instance_id": "customer.example",
                "authorized_representatives": ["@buyer:customer.example"],
                "accepted_payment_adapters": ["stripe"],
                "accepted_arbitration_policies": ["standard-digital-v1"]
            }),
            "@market:customer.example",
        ),
        envelope(
            "io.marketplace.order.created",
            events[1].body.clone(),
            "@market:customer.example",
        ),
        envelope(
            "io.marketplace.order.accepted",
            events[2].body.clone(),
            "@market:shop.example",
        ),
    ];
    let context = OrderRoomTimelineContext {
        room_id: "!order:customer.example".into(),
        authorities: authorities(),
        required_members: vec![
            "@market:shop.example".into(),
            "@market:customer.example".into(),
            "@market:arbiter.example".into(),
        ],
        members: vec![
            "@market:shop.example".into(),
            "@market:customer.example".into(),
            "@market:arbiter.example".into(),
        ],
    };
    validate_order_room_timeline(&raw, &context).unwrap();
}

#[test]
fn order_room_timeline_rejects_missing_required_members_and_representatives() {
    let context = OrderRoomTimelineContext {
        room_id: "!order:customer.example".into(),
        authorities: authorities(),
        required_members: vec!["@market:shop.example".into()],
        members: vec![],
    };
    assert_code(
        validate_order_room_timeline(&[], &context),
        ValidationCode::RoomMembershipViolation,
    );

    let raw = vec![
        envelope(
            "io.marketplace.actor.customer.bound",
            json!({
                "customer_id": "customer:customer.example:01JCUST",
                "status": "active",
                "display_name": "Acme",
                "instance_id": "customer.example",
                "authorized_representatives": ["@buyer:customer.example"],
                "accepted_payment_adapters": ["stripe"],
                "accepted_arbitration_policies": ["standard-digital-v1"]
            }),
            "@market:customer.example",
        ),
        envelope(
            "io.marketplace.order.created",
            serde_json::to_value(order()).unwrap(),
            "@market:customer.example",
        ),
    ];
    let context = OrderRoomTimelineContext {
        room_id: "!order:customer.example".into(),
        authorities: authorities(),
        required_members: vec![
            "@market:shop.example".into(),
            "@market:customer.example".into(),
            "@market:arbiter.example".into(),
        ],
        members: vec![
            "@market:shop.example".into(),
            "@market:customer.example".into(),
            "@market:arbiter.example".into(),
        ],
    };
    assert_code(
        validate_order_room_timeline(&raw, &context),
        ValidationCode::RoomMembershipViolation,
    );

    let events = morpheus_core::fixtures::valid_order_flow();
    let wrong_room = vec![envelope(
        "io.marketplace.order.created",
        events[1].body.clone(),
        "@market:customer.example",
    )];
    let context = OrderRoomTimelineContext {
        room_id: "!other:customer.example".into(),
        authorities: authorities(),
        required_members: vec![],
        members: vec![
            "@market:shop.example".into(),
            "@market:customer.example".into(),
            "@market:arbiter.example".into(),
        ],
    };
    assert_code(
        validate_order_room_timeline(&wrong_room, &context),
        ValidationCode::CatalogReferenceMismatch,
    );

    let mut unknown = envelope(
        "io.marketplace.future.event",
        json!({}),
        "@market:customer.example",
    );
    unknown["content"]["protocol_event_id"] = json!("evt:customer.example:01JUNKNOWN");
    let context = OrderRoomTimelineContext {
        room_id: "!order:customer.example".into(),
        authorities: authorities(),
        required_members: vec![],
        members: vec![],
    };
    validate_order_room_timeline(&[unknown], &context).unwrap();

    let unjoined_representative = vec![envelope(
        "io.marketplace.actor.customer.bound",
        json!({
            "customer_id": "customer:customer.example:01JCUST",
            "status": "active",
            "display_name": "Acme",
            "instance_id": "customer.example",
            "authorized_representatives": ["@buyer:customer.example"],
            "accepted_payment_adapters": ["stripe"],
            "accepted_arbitration_policies": ["standard-digital-v1"]
        }),
        "@market:customer.example",
    )];
    let context = OrderRoomTimelineContext {
        room_id: "!order:customer.example".into(),
        authorities: authorities(),
        required_members: vec![],
        members: vec!["@market:customer.example".into()],
    };
    assert_code(
        validate_order_room_timeline(&unjoined_representative, &context),
        ValidationCode::RoomMembershipViolation,
    );
}

#[test]
fn arbitration_policy_and_flow_rules_match_reference() {
    validate_arbitration_policy(&ArbitrationPolicy {
        policy_id: "standard-digital-v1".into(),
        version: "1".into(),
        arbitration_window: "P14D".into(),
        accepted_remedies: vec!["full_refund".into()],
        binding: true,
    })
    .unwrap();
    assert_code(
        validate_arbitration_policy(&ArbitrationPolicy {
            policy_id: "".into(),
            version: "1".into(),
            arbitration_window: "P14D".into(),
            accepted_remedies: vec!["full_refund".into()],
            binding: true,
        }),
        ValidationCode::MissingRequiredField,
    );

    assert_code(
        validate_arbitration_flow(&[ArbitrationFlowEvent {
            event_type: "io.marketplace.dispute.ruling.issued".into(),
            event_id: "$ruling".into(),
            room_id: "!order:customer.example".into(),
            body: json!({"binding": true, "ruling": "refund_required", "evidence_refs": ["$missing"]}),
        }]),
        ValidationCode::CatalogReferenceMismatch,
    );
    assert_code(
        validate_arbitration_flow(&[
            ArbitrationFlowEvent {
                event_type: "io.marketplace.dispute.evidence.submitted".into(),
                event_id: "$evidence".into(),
                room_id: "!order:customer.example".into(),
                body: json!({}),
            },
            ArbitrationFlowEvent {
                event_type: "io.marketplace.dispute.ruling.issued".into(),
                event_id: "$ruling".into(),
                room_id: "!order:customer.example".into(),
                body: json!({"binding": true, "ruling": "refund_required", "evidence_refs": ["$evidence"]}),
            },
        ]),
        ValidationCode::PolicyViolation,
    );

    validate_arbitration_flow(&[
        ArbitrationFlowEvent {
            event_type: "io.marketplace.dispute.evidence.submitted".into(),
            event_id: "$evidence".into(),
            room_id: "!order:customer.example".into(),
            body: json!({}),
        },
        ArbitrationFlowEvent {
            event_type: "io.marketplace.dispute.ruling.issued".into(),
            event_id: "$ruling".into(),
            room_id: "!order:customer.example".into(),
            body: json!({"binding": true, "ruling": "refund_required", "evidence_refs": ["$evidence"]}),
        },
        ArbitrationFlowEvent {
            event_type: "io.marketplace.payment.refunded".into(),
            event_id: "$refund".into(),
            room_id: "!order:customer.example".into(),
            body: json!({}),
        },
    ])
    .unwrap();

    validate_arbitration_flow(&[
        ArbitrationFlowEvent {
            event_type: "io.marketplace.dispute.evidence.submitted".into(),
            event_id: "$evidence".into(),
            room_id: "!order:customer.example".into(),
            body: json!({}),
        },
        ArbitrationFlowEvent {
            event_type: "io.marketplace.dispute.ruling.issued".into(),
            event_id: "$ruling".into(),
            room_id: "!order:customer.example".into(),
            body: json!({"binding": true, "ruling": "no_fault", "evidence_refs": ["$evidence"]}),
        },
    ])
    .unwrap();
}

#[test]
fn mock_payment_and_entitlement_secret_safety_are_enforced() {
    let adapter = MockPaymentAdapter;
    let intent = adapter.create_intent("ord:customer.example:01JORDER", "100.00", "USD");
    assert!(intent.provider_ref.starts_with("mock_pi_"));
    assert!(adapter.verify_webhook(&adapter.capture(&intent.payment_id).provider_ref));
    assert!(
        adapter
            .authorize(&intent.payment_id)
            .starts_with("mock_auth_")
    );
    assert!(adapter.refund(&intent.payment_id).starts_with("mock_rf_"));
    assert!(!adapter.verify_webhook("stripe_pi"));
    validate_entitlement_secret_safety("external-delivery-ref").unwrap();
    assert_code(
        validate_entitlement_secret_safety("https://files.example/download?token=secret"),
        ValidationCode::PolicyViolation,
    );
}
