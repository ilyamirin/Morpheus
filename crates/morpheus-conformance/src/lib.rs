use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorResult {
    pub id: String,
    pub group: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub struct ConformanceVector {
    pub id: String,
    pub group: String,
    pub run: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
}

#[derive(Default)]
pub struct ConformanceRunner {
    vectors: Vec<ConformanceVector>,
}

impl ConformanceRunner {
    pub fn new(vectors: Vec<ConformanceVector>) -> Self {
        Self { vectors }
    }

    pub fn run_all(&self) -> Vec<VectorResult> {
        self.vectors
            .iter()
            .map(|vector| match (vector.run)() {
                Ok(()) => VectorResult {
                    id: vector.id.clone(),
                    group: vector.group.clone(),
                    status: "passed".into(),
                    message: None,
                },
                Err(message) => VectorResult {
                    id: vector.id.clone(),
                    group: vector.group.clone(),
                    status: "failed".into(),
                    message: Some(message),
                },
            })
            .collect()
    }
}

pub fn required_vectors() -> ConformanceRunner {
    ConformanceRunner::new(vec![
        vector("required.valid_catalog_snapshot", || {
            let mut catalog = morpheus_core::CatalogIndex::new("shop.example");
            catalog
                .apply_snapshot(morpheus_core::SnapshotRecord {
                    snapshot_id: "snap_01J".into(),
                    sequence: 1,
                    sha256: "sha256:abc".into(),
                    covers_events_until: "$a".into(),
                })
                .map_err(|err| err.to_string())
        }),
        vector("required.valid_product_offer_delta", || {
            morpheus_core::fixtures::valid_catalog();
            Ok(())
        }),
        vector("required.unknown_instance_catalog_rejected", || {
            let allowlist = morpheus_core::AllowlistPolicy::default();
            if allowlist.can("unknown.example", "catalog") {
                Err("unknown instance accepted".into())
            } else {
                Ok(())
            }
        }),
        vector("required.suspended_seller_offer_rejected", || {
            let mut catalog = morpheus_core::CatalogIndex::new("shop.example");
            catalog
                .apply_snapshot(morpheus_core::SnapshotRecord {
                    snapshot_id: "snap_01J".into(),
                    sequence: 1,
                    sha256: "sha256:abc".into(),
                    covers_events_until: "$a".into(),
                })
                .unwrap();
            catalog
                .upsert_seller(morpheus_core::SellerRecord {
                    seller_id: "seller:shop.example:01JSELLER".into(),
                    status: "suspended".into(),
                })
                .unwrap();
            expect_rejected(catalog.upsert_offer(morpheus_core::OfferRecord {
                offer_id: "offer:shop.example:01JOFFER".into(),
                product_id: "prod:shop.example:01JPROD".into(),
                seller_id: "seller:shop.example:01JSELLER".into(),
                revision: 1,
                price: morpheus_core::Money {
                    amount: "100.00".into(),
                    currency: "USD".into(),
                },
                entitlement_type: "booking_slot".into(),
            }))
        }),
        vector("required.stale_offer_revision_rejected", || {
            let catalog = morpheus_core::fixtures::valid_catalog();
            let mut order = morpheus_core::fixtures::valid_order_created();
            order.offer_revision = 1;
            let allowlist = order_allowlist();
            let customer = valid_customer();
            expect_rejected(morpheus_core::validate_order_created(
                &order, &catalog, &allowlist, &customer,
            ))
        }),
        vector("required.price_mismatch_rejected", || {
            let catalog = morpheus_core::fixtures::valid_catalog();
            let mut order = morpheus_core::fixtures::valid_order_created();
            order.price.amount = "1.00".into();
            let allowlist = order_allowlist();
            let customer = valid_customer();
            expect_rejected(morpheus_core::validate_order_created(
                &order, &catalog, &allowlist, &customer,
            ))
        }),
        vector("required.valid_order_lifecycle_completed", || {
            morpheus_core::validate_order_sequence(&morpheus_core::fixtures::valid_order_flow())
                .and_then(|decision| {
                    if decision.final_state == morpheus_core::OrderState::Completed {
                        Ok(())
                    } else {
                        Err(morpheus_protocol::ValidationError::new(
                            morpheus_protocol::ValidationCode::InvalidStateTransition,
                            "order did not complete",
                        ))
                    }
                })
                .map_err(|err| err.to_string())
        }),
        vector("required.unauthorized_payment_capture_rejected", || {
            expect_rejected(morpheus_core::assert_event_authority(
                "io.marketplace.payment.captured",
                "@market:customer.example",
                &morpheus_core::OrderAuthorities {
                    seller_as_user: "@market:shop.example".into(),
                    customer_as_user: "@market:customer.example".into(),
                    arbiter_as_user: "@market:arbiter.example".into(),
                    payment_as_users: vec![],
                },
            ))
        }),
        vector("required.entitlement_before_capture_rejected", || {
            let mut flow = morpheus_core::fixtures::valid_order_flow();
            flow.remove(5);
            expect_rejected(morpheus_core::validate_order_sequence(&flow).map(|_| ()))
        }),
        vector("required.non_allowlisted_arbiter_rejected", || {
            let catalog = morpheus_core::fixtures::valid_catalog();
            let allowlist = morpheus_core::AllowlistPolicy::new([(
                "shop.example".to_string(),
                vec!["orders".to_string()],
            )]);
            let customer = valid_customer();
            expect_rejected(morpheus_core::validate_order_created(
                &morpheus_core::fixtures::valid_order_created(),
                &catalog,
                &allowlist,
                &customer,
            ))
        }),
        vector("required.non_arbiter_ruling_rejected", || {
            expect_rejected(morpheus_core::assert_event_authority(
                "io.marketplace.dispute.ruling.issued",
                "@market:shop.example",
                &morpheus_core::OrderAuthorities {
                    seller_as_user: "@market:shop.example".into(),
                    customer_as_user: "@market:customer.example".into(),
                    arbiter_as_user: "@market:arbiter.example".into(),
                    payment_as_users: vec![],
                },
            ))
        }),
        vector("required.unknown_critical_extension_rejected", || {
            let mut event = morpheus_protocol::fixtures::valid_order_created_event();
            event["content"]["critical"] = serde_json::json!(["com.example.unknown"]);
            expect_rejected(morpheus_protocol::validate_event_envelope(&event).map(|_| ()))
        }),
        vector("required.order_room_replay_rejected", || {
            let mut event = morpheus_protocol::fixtures::valid_order_created_event();
            event["room_id"] = serde_json::json!("!other:customer.example");
            expect_rejected(morpheus_protocol::validate_event_envelope(&event).map(|_| ()))
        }),
        vector("required.snapshot_hash_mismatch_rejected", || {
            let mut catalog = morpheus_core::CatalogIndex::new("shop.example");
            catalog
                .apply_snapshot(morpheus_core::SnapshotRecord {
                    snapshot_id: "snap_01J".into(),
                    sequence: 2,
                    sha256: "sha256:abc".into(),
                    covers_events_until: "$a".into(),
                })
                .unwrap();
            expect_rejected(catalog.apply_snapshot(morpheus_core::SnapshotRecord {
                snapshot_id: "snap_01J".into(),
                sequence: 2,
                sha256: "sha256:def".into(),
                covers_events_until: "$a".into(),
            }))
        }),
        vector("required.revision_rollback_rejected", || {
            let mut catalog = morpheus_core::fixtures::valid_catalog();
            expect_rejected(catalog.upsert_offer(morpheus_core::OfferRecord {
                offer_id: "offer:shop.example:01JOFFER".into(),
                product_id: "prod:shop.example:01JPROD".into(),
                seller_id: "seller:shop.example:01JSELLER".into(),
                revision: 2,
                price: morpheus_core::Money {
                    amount: "100.00".into(),
                    currency: "USD".into(),
                },
                entitlement_type: "booking_slot".into(),
            }))
        }),
    ])
}

fn vector(
    id: &str,
    run: impl Fn() -> Result<(), String> + Send + Sync + 'static,
) -> ConformanceVector {
    ConformanceVector {
        id: id.into(),
        group: "required".into(),
        run: Box::new(run),
    }
}

fn order_allowlist() -> morpheus_core::AllowlistPolicy {
    morpheus_core::AllowlistPolicy::new([
        ("shop.example".to_string(), vec!["orders".to_string()]),
        (
            "arbiter.example".to_string(),
            vec!["arbitration".to_string()],
        ),
    ])
}

fn valid_customer() -> morpheus_core::CustomerBinding {
    morpheus_core::CustomerBinding {
        customer_id: "customer:customer.example:01JCUST".into(),
        status: "active".into(),
        accepted_payment_adapters: vec!["mock".into()],
        accepted_arbitration_policies: vec!["standard-digital-v1".into()],
    }
}

fn expect_rejected<T>(result: Result<T, morpheus_protocol::ValidationError>) -> Result<(), String> {
    match result {
        Ok(_) => Err("expected rejection".into()),
        Err(_) => Ok(()),
    }
}
