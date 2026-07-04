use morpheus_protocol::*;
use serde_json::{Value, json};

fn valid_event() -> Value {
    morpheus_protocol::fixtures::valid_order_created_event()
}

fn event_with(event_type: &str, body: Value) -> Value {
    let mut event = valid_event();
    event["type"] = json!(event_type);
    event["content"]["protocol_event_id"] = json!(format!(
        "evt:customer.example:{}",
        event_type
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_uppercase()
    ));
    event["content"]["body"] = body;
    event
}

fn assert_code(result: ValidationResult<impl Sized>, code: ValidationCode) {
    match result {
        Ok(_) => panic!("expected rejection"),
        Err(err) => assert_eq!(err.code, code),
    }
}

fn validated(mut event: Value) -> ValidationResult<ValidatedMarketplaceEvent> {
    validate_event_envelope(&event).inspect(|_| {
        event["content"]["critical"] = json!([]);
    })
}

#[test]
fn constants_match_v01_namespace() {
    assert_eq!(PROTOCOL_NAME, "io.marketplace");
    assert_eq!(PROTOCOL_VERSION, "0.1");
    assert!(CATALOG_EVENT_TYPES.contains(&"io.marketplace.offer.upserted"));
    assert!(ORDER_EVENT_TYPES.contains(&"io.marketplace.dispute.ruling.issued"));
    assert!(ENTITLEMENT_TYPES.contains(&"booking_slot"));
    assert!(PRODUCT_KINDS.contains(&"digital_service"));
    assert!(DISPUTE_RULINGS.contains(&"partial_refund_required"));
}

#[test]
fn validation_disposition_matches_reference() {
    assert_eq!(
        validation_disposition(ValidationCode::MissingRequiredField),
        ValidationDisposition::Retryable
    );
    assert_eq!(
        validation_disposition(ValidationCode::RoomProfileViolation),
        ValidationDisposition::Retryable
    );
    assert_eq!(
        validation_disposition(ValidationCode::UnauthorizedSender),
        ValidationDisposition::Terminal
    );
}

#[test]
fn parses_actor_ids_and_rejects_extra_segments() {
    let parsed = parse_actor_id("seller:shop.example:01JSELLER").unwrap();
    assert_eq!(parsed.kind, "seller");
    assert_eq!(parsed.instance_id, "shop.example");
    assert_code(
        parse_actor_id("seller:shop.example:01J:EXTRA"),
        ValidationCode::InvalidId,
    );
}

#[test]
fn parses_object_instances_and_rejects_bad_prefixes() {
    assert_eq!(
        parse_object_instance("offer:shop.example:01JOFFER").unwrap(),
        "shop.example"
    );
    assert!(is_protocol_object_id(
        "snap:shop.example:01JSNAP",
        Some("snap")
    ));
    assert!(!is_protocol_object_id("snap_01J", Some("snap")));
    assert_code(
        parse_object_instance("legacy:shop.example:01J"),
        ValidationCode::InvalidId,
    );
}

#[test]
fn rejects_non_dns_instance_ids() {
    assert!(is_valid_instance_id("shop.example"));
    assert!(!is_valid_instance_id("shop"));
    assert!(!is_valid_instance_id("Shop.example"));
    assert!(!is_protocol_object_id("offer:shop:01JOFFER", Some("offer")));
}

#[test]
fn canonical_json_sorts_keys_and_hashes() {
    let value = json!({"b": 1, "a": {"d": 2, "c": [3, {"f": 4, "e": 5}]}});
    assert_eq!(
        canonical_json(&value).unwrap(),
        r#"{"a":{"c":[3,{"e":5,"f":4}],"d":2},"b":1}"#
    );
    let hash = sha256_canonical(&value).unwrap();
    assert!(hash.starts_with("sha256:"));
    assert_eq!(hash.len(), 71);
}

#[test]
fn canonical_hash_mismatch_is_rejected() {
    assert_code(
        assert_sha256_matches(&json!({"a": 1}), "bad"),
        ValidationCode::HashMismatch,
    );
    assert_code(
        assert_sha256_matches(
            &json!({"a": 1}),
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        ValidationCode::HashMismatch,
    );
}

#[test]
fn accepts_valid_marketplace_envelope() {
    let event = validated(valid_event()).unwrap();
    assert_eq!(event.event_type, "io.marketplace.order.created");
    assert_eq!(event.marketplace_event_id, "evt:customer.example:01JORDER");
}

#[test]
fn rejects_unsupported_protocol_versions() {
    let mut event = valid_event();
    event["content"]["protocol_version"] = json!("0.0");
    assert_code(
        validate_event_envelope(&event),
        ValidationCode::UnsupportedProtocolVersion,
    );
}

#[test]
fn rejects_non_utc_timestamps() {
    let mut event = valid_event();
    event["content"]["created_at"] = json!("2026-05-04T10:00:00+03:00");
    assert_code(
        validate_event_envelope(&event),
        ValidationCode::MissingRequiredField,
    );
}

#[test]
fn rejects_sender_issuer_mismatch() {
    let mut event = valid_event();
    event["sender"] = json!("@other:customer.example");
    assert_code(
        validate_event_envelope(&event),
        ValidationCode::UnauthorizedSender,
    );
}

#[test]
fn rejects_missing_actor_bound_issuer_actor_id() {
    let mut event = valid_event();
    event["content"]["issuer"]
        .as_object_mut()
        .unwrap()
        .remove("actor_id");
    assert_code(
        validate_event_envelope(&event),
        ValidationCode::MissingRequiredField,
    );
}

#[test]
fn rejects_invalid_money_amounts_and_quantity_above_one() {
    let mut event = valid_event();
    event["content"]["body"]["price"]["amount"] = json!("10.123456789");
    assert_code(
        validate_event_envelope(&event),
        ValidationCode::MissingRequiredField,
    );

    let mut event = valid_event();
    event["content"]["body"]["quantity"] = json!(2);
    assert_code(
        validate_event_envelope(&event),
        ValidationCode::PaymentTermsMismatch,
    );
}

#[test]
fn accepts_uppercase_asset_currency_codes() {
    let mut event = valid_event();
    event["content"]["body"]["price"]["currency"] = json!("USDC");
    validate_event_envelope(&event).unwrap();
}

#[test]
fn rejects_room_profile_violations() {
    assert_event_allowed_in_room(RoomProfile::Catalog, "io.marketplace.offer.upserted").unwrap();
    assert_code(
        assert_event_allowed_in_room(RoomProfile::Catalog, "io.marketplace.order.created"),
        ValidationCode::RoomProfileViolation,
    );
}

#[test]
fn accepts_registered_critical_and_rejects_unknown_critical() {
    let mut context = MarketplaceEventValidationContext {
        room_profile: Some(RoomProfile::Order),
        supported_critical: ["com.example.supported".to_string()].into_iter().collect(),
        ..Default::default()
    };
    let mut event = valid_event();
    event["content"]["critical"] = json!(["com.example.supported"]);
    assert!(matches!(
        validate_marketplace_event(&event, &mut context).unwrap(),
        MarketplaceEventValidationResult::Accepted(_)
    ));

    let mut event = valid_event();
    event["content"]["critical"] = json!(["com.example.unknown"]);
    let mut context = MarketplaceEventValidationContext {
        room_profile: Some(RoomProfile::Order),
        ..Default::default()
    };
    assert_code(
        validate_marketplace_event(&event, &mut context).map(|_| ()),
        ValidationCode::UnknownCriticalExtension,
    );
}

#[test]
fn ignores_unknown_non_critical_marketplace_events() {
    let mut event = valid_event();
    event["type"] = json!("io.marketplace.future.event");
    let mut context = MarketplaceEventValidationContext::default();
    assert_eq!(
        validate_marketplace_event(&event, &mut context).unwrap(),
        MarketplaceEventValidationResult::IgnoredUnknownEventType
    );
}

#[test]
fn rejects_unknown_events_with_critical_extensions() {
    let mut event = valid_event();
    event["type"] = json!("io.marketplace.future.event");
    event["content"]["critical"] = json!(["com.example.critical"]);
    let mut context = MarketplaceEventValidationContext::default();
    assert_code(
        validate_marketplace_event(&event, &mut context).map(|_| ()),
        ValidationCode::UnknownCriticalExtension,
    );
}

#[test]
fn rejects_protocol_event_id_replay_with_different_body_hash() {
    let mut context = MarketplaceEventValidationContext {
        room_profile: Some(RoomProfile::Order),
        ..Default::default()
    };
    let first = valid_event();
    validate_marketplace_event(&first, &mut context).unwrap();
    let mut second = first.clone();
    second["event_id"] = json!("$other");
    second["content"]["body"]["quantity"] = json!(1);
    assert_code(
        validate_marketplace_event(&second, &mut context).map(|_| ()),
        ValidationCode::DuplicateEvent,
    );
}

#[test]
fn rejects_redacted_marketplace_events() {
    let mut event = valid_event();
    event["unsigned"] = json!({"redacted_because": {"event_id": "$r"}});
    assert_code(
        validate_event_envelope(&event),
        ValidationCode::RedactedEvent,
    );
}

#[test]
fn validates_schema_specific_required_fields() {
    for (event_type, body) in [
        (
            "io.marketplace.payment.intent.created",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "adapter": "stripe",
                "amount": "100.00",
                "currency": "USD",
                "capture_policy": "before_entitlement",
                "idempotency_key": "idem",
                "provider_ref": "pi",
                "confirmation": {"method": "redirect", "uri": "https://pay.example/confirm"},
                "expires_at": "2026-05-04T10:30:00Z"
            }),
        ),
        (
            "io.marketplace.payment.captured",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "adapter": "stripe",
                "amount": "100.00",
                "currency": "USD",
                "provider_ref": "ch",
                "evidence": {"kind": "provider_receipt", "uri": "https://pay.example/r", "sha256": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"}
            }),
        ),
        (
            "io.marketplace.dispute.evidence.submitted",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "dispute_id": "disp:arbiter.example:01JDISP",
                "evidence": {"kind": "statement", "uri": "mxc://customer.example/e", "sha256": "sha256:4444444444444444444444444444444444444444444444444444444444444444"}
            }),
        ),
        (
            "io.marketplace.entitlement.granted",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "entitlement_id": "ent:customer.example:01JENT",
                "type": "license_key",
                "external_ref": "license-01JENT",
                "evidence": {"kind": "delivery", "uri": "https://deliver.example/e", "sha256": "sha256:5555555555555555555555555555555555555555555555555555555555555555"}
            }),
        ),
        (
            "io.marketplace.entitlement.granted",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "entitlement_id": "ent:customer.example:01JENT",
                "type": "license_key",
                "external_ref": "license-01JENT"
            }),
        ),
    ] {
        let mut event = valid_event();
        event["type"] = json!(event_type);
        event["content"]["body"] = body;
        assert!(validate_event_envelope(&event).is_ok(), "{event_type}");
    }
}

#[test]
fn validates_every_known_protocol_event_body_schema() {
    let hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let cases = [
        (
            "io.marketplace.instance.profile",
            json!({
                "instance_id": "shop.example",
                "matrix_server_name": "shop.example",
                "application_service_id": "morpheus-shop",
                "catalog_room_id": "!catalog:shop.example",
                "protocol_versions": ["0.1"],
                "payment_adapters": ["mock"],
                "entitlement_types": ["external_entitlement"],
                "arbitration_policies": ["standard-digital-v1"]
            }),
        ),
        (
            "io.marketplace.catalog.profile",
            json!({"instance_id": "shop.example", "snapshot_required": true, "delta_required": true}),
        ),
        (
            "io.marketplace.actor.seller.announced",
            json!({
                "seller_id": "seller:shop.example:01JSELLER",
                "status": "active",
                "display_name": "Seller",
                "legal_profile_ref": "https://shop.example/legal",
                "terms_ref": "https://shop.example/terms",
                "terms_hash": hash,
                "supported_payment_adapters": ["mock"],
                "supported_entitlement_types": ["external_entitlement"]
            }),
        ),
        (
            "io.marketplace.actor.seller.suspended",
            json!({"seller_id": "seller:shop.example:01JSELLER", "status": "suspended"}),
        ),
        (
            "io.marketplace.actor.customer.bound",
            json!({
                "customer_id": "customer:customer.example:01JCUST",
                "status": "active",
                "display_name": "Customer",
                "instance_id": "customer.example",
                "authorized_representatives": ["@buyer:customer.example"],
                "accepted_payment_adapters": ["mock"],
                "accepted_arbitration_policies": ["standard-digital-v1"]
            }),
        ),
        (
            "io.marketplace.catalog.snapshot.published",
            json!({
                "snapshot_id": "snap:shop.example:01JSNAP",
                "sequence": 1,
                "format": "application/json+io.marketplace.catalog.v0",
                "uri": "mxc://shop.example/snapshot",
                "sha256": hash,
                "covers_events_until": "$event",
                "product_count": 1,
                "offer_count": 1,
                "created_at": "2026-05-04T10:00:00Z"
            }),
        ),
        (
            "io.marketplace.product.upserted",
            json!({
                "product_id": "prod:shop.example:01JPROD",
                "seller_id": "seller:shop.example:01JSELLER",
                "revision": 1,
                "status": "active",
                "kind": "digital_service",
                "title": "Consulting",
                "description": "Remote consulting",
                "categories": ["services"],
                "tags": ["remote"],
                "media": [],
                "terms_hash": hash
            }),
        ),
        (
            "io.marketplace.product.withdrawn",
            json!({"product_id": "prod:shop.example:01JPROD", "revision": 2}),
        ),
        (
            "io.marketplace.offer.upserted",
            json!({
                "offer_id": "offer:shop.example:01JOFFER",
                "product_id": "prod:shop.example:01JPROD",
                "seller_id": "seller:shop.example:01JSELLER",
                "revision": 1,
                "status": "active",
                "price": {"amount": "100.00", "currency": "USD"},
                "payment_terms": {"capture_policy": "before_entitlement", "adapter_policy": "seller_supported"},
                "entitlement": {"type": "external_entitlement", "delivery": "external"},
                "availability": {"mode": "unlimited"},
                "seller_terms_hash": hash,
                "offer_terms_hash": hash
            }),
        ),
        (
            "io.marketplace.offer.withdrawn",
            json!({"offer_id": "offer:shop.example:01JOFFER", "revision": 2}),
        ),
        (
            "io.marketplace.inventory.updated",
            json!({"offer_id": "offer:shop.example:01JOFFER", "revision": 2, "available_quantity": 10}),
        ),
        (
            "io.marketplace.order.created",
            valid_event()["content"]["body"].clone(),
        ),
        (
            "io.marketplace.order.accepted",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "offer_revision": 3,
                "seller_terms_hash": hash,
                "offer_terms_hash": hash,
                "payment_capture_policy": "before_entitlement",
                "arbitration_policy_version": "1"
            }),
        ),
        (
            "io.marketplace.order.cancelled",
            json!({"order_id": "ord:customer.example:01JORDER"}),
        ),
        (
            "io.marketplace.order.rejected",
            json!({"order_id": "ord:customer.example:01JORDER"}),
        ),
        (
            "io.marketplace.order.completed",
            json!({"order_id": "ord:customer.example:01JORDER"}),
        ),
        (
            "io.marketplace.payment.intent.created",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "adapter": "mock",
                "amount": "100.00",
                "currency": "USD",
                "capture_policy": "before_entitlement",
                "idempotency_key": "idem",
                "provider_ref": "mock_pi",
                "confirmation": {"method": "redirect", "uri": "https://pay.example/confirm"},
                "expires_at": "2026-05-04T10:30:00Z"
            }),
        ),
        (
            "io.marketplace.payment.authorized",
            json!({"order_id": "ord:customer.example:01JORDER", "payment_id": "pay:customer.example:01JPAY"}),
        ),
        (
            "io.marketplace.payment.failed",
            json!({"order_id": "ord:customer.example:01JORDER", "payment_id": "pay:customer.example:01JPAY"}),
        ),
        (
            "io.marketplace.payment.cancelled",
            json!({"order_id": "ord:customer.example:01JORDER", "payment_id": "pay:customer.example:01JPAY"}),
        ),
        (
            "io.marketplace.payment.captured",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "adapter": "mock",
                "amount": "100.00",
                "currency": "USD",
                "provider_ref": "mock_ch",
                "evidence": {"kind": "receipt", "uri": "https://pay.example/r", "sha256": hash}
            }),
        ),
        (
            "io.marketplace.payment.refund.requested",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "refund_id": "refund:customer.example:01JREFUND",
                "amount": "25.00",
                "currency": "USD",
                "provider_ref": "mock_rf",
                "evidence": {"kind": "receipt", "uri": "https://pay.example/rf", "sha256": hash}
            }),
        ),
        (
            "io.marketplace.payment.refunded",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "refund_id": "refund:customer.example:01JREFUND",
                "amount": "25.00",
                "currency": "USD",
                "provider_ref": "mock_rf",
                "evidence": {"kind": "receipt", "uri": "mxc://shop.example/rf", "sha256": hash}
            }),
        ),
        (
            "io.marketplace.payment.chargeback.opened",
            json!({"order_id": "ord:customer.example:01JORDER", "payment_id": "pay:customer.example:01JPAY"}),
        ),
        (
            "io.marketplace.entitlement.granted",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "entitlement_id": "ent:customer.example:01JENT",
                "type": "external_entitlement",
                "external_ref": "external",
                "evidence": {"kind": "delivery", "uri": "https://deliver.example/e", "sha256": hash}
            }),
        ),
        (
            "io.marketplace.entitlement.activated",
            json!({"order_id": "ord:customer.example:01JORDER", "entitlement_id": "ent:customer.example:01JENT"}),
        ),
        (
            "io.marketplace.entitlement.completed",
            json!({"order_id": "ord:customer.example:01JORDER", "entitlement_id": "ent:customer.example:01JENT"}),
        ),
        (
            "io.marketplace.entitlement.revoked",
            json!({"order_id": "ord:customer.example:01JORDER", "entitlement_id": "ent:customer.example:01JENT"}),
        ),
        (
            "io.marketplace.entitlement.expired",
            json!({"order_id": "ord:customer.example:01JORDER", "entitlement_id": "ent:customer.example:01JENT"}),
        ),
        (
            "io.marketplace.dispute.opened",
            json!({"order_id": "ord:customer.example:01JORDER", "dispute_id": "disp:arbiter.example:01JDISP"}),
        ),
        (
            "io.marketplace.dispute.evidence.submitted",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "dispute_id": "disp:arbiter.example:01JDISP",
                "evidence": {"kind": "statement", "uri": "http://arbiter.example/e", "sha256": hash}
            }),
        ),
        (
            "io.marketplace.dispute.ruling.issued",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "dispute_id": "disp:arbiter.example:01JDISP",
                "ruling": "partial_refund_required",
                "reason_code": "not_as_described",
                "remedy": {"amount": "25.00", "currency": "USD"},
                "evidence_refs": ["$evidence"],
                "binding": true
            }),
        ),
        (
            "io.marketplace.dispute.closed",
            json!({"order_id": "ord:customer.example:01JORDER", "dispute_id": "disp:arbiter.example:01JDISP"}),
        ),
    ];

    for (event_type, body) in cases {
        assert!(
            validate_event_envelope(&event_with(event_type, body)).is_ok(),
            "{event_type}"
        );
    }
}

#[test]
fn rejects_strict_evidence_hashes_and_entitlement_requirements() {
    let mut event = valid_event();
    event["type"] = json!("io.marketplace.payment.captured");
    event["content"]["body"] = json!({
        "order_id": "ord:customer.example:01JORDER",
        "payment_id": "pay:customer.example:01JPAY",
        "adapter": "stripe",
        "amount": "100.00",
        "currency": "USD",
        "provider_ref": "ch",
        "evidence": {"kind": "provider_receipt", "uri": "https://pay.example/r", "sha256": "sha256:receipt"}
    });
    assert_code(
        validate_event_envelope(&event),
        ValidationCode::MissingRequiredField,
    );

    let mut event = valid_event();
    event["type"] = json!("io.marketplace.entitlement.granted");
    event["content"]["body"] = json!({
        "order_id": "ord:customer.example:01JORDER",
        "entitlement_id": "ent:customer.example:01JENT",
        "type": "booking_slot",
        "external_ref": "booking"
    });
    assert_code(
        validate_event_envelope(&event),
        ValidationCode::MissingRequiredField,
    );
}

#[test]
fn validates_policy_helpers() {
    let err = ValidationError::new(ValidationCode::UnauthorizedSender, "no");
    assert_eq!(err.disposition(), ValidationDisposition::Terminal);
    assert!(validate_extension_name("com.example.feature").is_ok());
    assert_code(
        validate_extension_name("io.marketplace.private"),
        ValidationCode::PolicyViolation,
    );
    assert_code(
        validate_extension_name("feature"),
        ValidationCode::PolicyViolation,
    );
    assert!(
        canonical_json_sha256(&json!({"ok": true}))
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(validate_min_consumer_version("0.1", "0.1").is_ok());
    assert_code(
        validate_min_consumer_version("0.2", "0.1"),
        ValidationCode::UnsupportedProtocolVersion,
    );
    assert!(validate_sender_issuer_server("@market:shop.example", "shop.example").is_ok());
    assert_code(
        validate_sender_issuer_server("@market:other.example", "shop.example"),
        ValidationCode::UnauthorizedSender,
    );
    assert!(validate_retention_policy(true, true).is_ok());
    assert_code(
        validate_retention_policy(false, true),
        ValidationCode::PolicyViolation,
    );
}

#[test]
fn validates_operational_and_appservice_helpers() {
    assert!(validate_backfill_page_event_ids(&["$a".into(), "$b".into()]).is_ok());
    assert_code(
        validate_backfill_page_event_ids(&["$a".into(), "$a".into()]),
        ValidationCode::DuplicateEvent,
    );
    assert!(validate_snapshot_cache_entry(Some("sha256:a"), "sha256:a").is_ok());
    assert_code(
        validate_snapshot_cache_entry(Some("sha256:a"), "sha256:b"),
        ValidationCode::HashMismatch,
    );
    assert!(
        validate_appservice_sender_namespace("@market_bot:shop.example", "shop.example", "market_")
            .is_ok()
    );
    assert_code(
        validate_appservice_sender_namespace("@bot:shop.example", "shop.example", "market_"),
        ValidationCode::UnauthorizedSender,
    );
    assert!(validate_appservice_transaction(Some(&["$a".into()]), &["$a".into()]).is_ok());
    assert_code(
        validate_appservice_transaction(Some(&["$a".into()]), &["$b".into()]),
        ValidationCode::DuplicateEvent,
    );
}

#[test]
fn validates_privacy_rules() {
    assert_code(
        validate_marketplace_privacy(
            "io.marketplace.offer.upserted",
            &json!({"customer_id": "customer:customer.example:01JCUST"}),
        ),
        ValidationCode::PrivacyViolation,
    );
    assert_code(
        validate_marketplace_privacy(
            "io.marketplace.entitlement.granted",
            &json!({"uri": "https://files.example/download?token=secret"}),
        ),
        ValidationCode::PrivacyViolation,
    );
    assert!(
        validate_marketplace_privacy(
            "io.marketplace.offer.upserted",
            &json!({"offer_id": "offer:shop.example:01JOFFER"})
        )
        .is_ok()
    );
}

#[test]
fn rejects_invalid_generic_event_shapes_and_replays_same_event_idempotently() {
    for (pointer, value, code) in [
        (
            "/room_id",
            json!("bad"),
            ValidationCode::MissingRequiredField,
        ),
        (
            "/content/protocol",
            json!("other"),
            ValidationCode::MissingRequiredField,
        ),
        (
            "/content/protocol_event_id",
            json!("bad"),
            ValidationCode::InvalidId,
        ),
        (
            "/content/issuer/instance_id",
            json!("bad"),
            ValidationCode::InvalidId,
        ),
        (
            "/content/issuer/matrix_user_id",
            json!("market"),
            ValidationCode::MissingRequiredField,
        ),
        (
            "/content/created_at",
            json!("not a date"),
            ValidationCode::MissingRequiredField,
        ),
    ] {
        let mut event = valid_event();
        *event.pointer_mut(pointer).unwrap() = value;
        assert_code(validate_event_envelope(&event), code);
    }

    assert_code(
        validate_event_envelope(&json!({"type": "io.marketplace.order.created"})),
        ValidationCode::MissingRequiredField,
    );

    let mut context = MarketplaceEventValidationContext::default();
    let event = valid_event();
    validate_marketplace_event(&event, &mut context).unwrap();
    assert!(validate_marketplace_event(&event, &mut context).is_ok());

    let mut unknown = valid_event();
    unknown["type"] = json!("io.marketplace.future.event");
    assert_code(
        validate_event_envelope(&unknown),
        ValidationCode::UnknownEventType,
    );
}

#[test]
fn rejects_invalid_schema_helper_branches() {
    let cases = [
        (
            "io.marketplace.product.upserted",
            json!({
                "product_id": "bad",
                "seller_id": "seller:shop.example:01JSELLER",
                "revision": 1,
                "status": "active",
                "kind": "digital_service",
                "title": "Consulting",
                "description": "Remote consulting",
                "categories": ["services"],
                "tags": ["remote"],
                "media": [],
                "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }),
            ValidationCode::InvalidId,
        ),
        (
            "io.marketplace.catalog.profile",
            json!({"instance_id": "shop.example", "snapshot_required": "yes", "delta_required": true}),
            ValidationCode::MissingRequiredField,
        ),
        (
            "io.marketplace.product.withdrawn",
            json!({"product_id": "prod:shop.example:01JPROD", "revision": 0}),
            ValidationCode::MissingRequiredField,
        ),
        (
            "io.marketplace.offer.upserted",
            json!({
                "offer_id": "offer:shop.example:01JOFFER",
                "product_id": "prod:shop.example:01JPROD",
                "seller_id": "seller:shop.example:01JSELLER",
                "revision": 1,
                "status": "paused",
                "price": {"amount": "100.00", "currency": "USD"},
                "payment_terms": {"capture_policy": "bad", "adapter_policy": "seller_supported"},
                "entitlement": {"type": "external_entitlement", "delivery": "external"},
                "availability": {"mode": "unlimited"},
                "seller_terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "offer_terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }),
            ValidationCode::MissingRequiredField,
        ),
        (
            "io.marketplace.payment.intent.created",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "payment_id": "pay:customer.example:01JPAY",
                "adapter": "mock",
                "amount": "100",
                "currency": "US",
                "capture_policy": "before_entitlement",
                "idempotency_key": "idem",
                "provider_ref": "mock_pi",
                "confirmation": {"method": "redirect", "uri": "ftp://pay.example/confirm"},
                "expires_at": "2026-05-04T10:30:00+03:00"
            }),
            ValidationCode::MissingRequiredField,
        ),
        (
            "io.marketplace.dispute.evidence.submitted",
            json!({
                "order_id": "ord:customer.example:01JORDER",
                "dispute_id": "disp:arbiter.example:01JDISP",
                "evidence": {"kind": "statement", "uri": "file:///tmp/e", "sha256": "bad"}
            }),
            ValidationCode::MissingRequiredField,
        ),
    ];

    for (event_type, body, code) in cases {
        assert_code(validate_event_envelope(&event_with(event_type, body)), code);
    }
}
