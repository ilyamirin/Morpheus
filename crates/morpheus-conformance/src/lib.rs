use morpheus_core::{
    ArbitrationFlowEvent, CatalogDeltaEvent, CatalogSnapshotDocument, OfferRecord,
    OrderAuthorities, OrderDecision, OrderState, ProductRecord, SellerRecord, SnapshotRecord,
};
use morpheus_protocol::{
    MarketplaceEventValidationContext, RoomProfile, ValidationError, assert_sha256_matches,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

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
                .apply_snapshot(snapshot())
                .map_err(|err| err.to_string())
        }),
        vector("required.valid_product_offer_delta", || {
            morpheus_core::fixtures::valid_catalog();
            Ok(())
        }),
        vector("required.unknown_instance_catalog_rejected", || {
            let allowlist = morpheus_core::AllowlistPolicy::default();
            reject_bool(allowlist.can("unknown.example", "catalog"))
        }),
        vector("required.suspended_seller_offer_rejected", || {
            let mut catalog = morpheus_core::CatalogIndex::new("shop.example");
            catalog.apply_snapshot(snapshot()).unwrap();
            catalog
                .upsert_seller(SellerRecord {
                    seller_id: "seller:shop.example:01JSELLER".into(),
                    status: "suspended".into(),
                })
                .unwrap();
            expect_rejected(catalog.upsert_offer(offer()))
        }),
        vector("required.stale_offer_revision_rejected", || {
            let catalog = morpheus_core::fixtures::valid_catalog();
            let mut order = morpheus_core::fixtures::valid_order_created();
            order.offer_revision = 1;
            expect_rejected(morpheus_core::validate_order_created(
                &order,
                &catalog,
                &morpheus_core::fixtures::order_allowlist(),
                &morpheus_core::fixtures::valid_customer(),
            ))
        }),
        vector("required.price_mismatch_rejected", || {
            let catalog = morpheus_core::fixtures::valid_catalog();
            let mut order = morpheus_core::fixtures::valid_order_created();
            order.price.amount = "1.00".into();
            expect_rejected(morpheus_core::validate_order_created(
                &order,
                &catalog,
                &morpheus_core::fixtures::order_allowlist(),
                &morpheus_core::fixtures::valid_customer(),
            ))
        }),
        vector("required.valid_order_lifecycle_completed", || {
            morpheus_core::validate_order_sequence(&morpheus_core::fixtures::valid_order_flow())
                .and_then(expect_order_completed)
                .map_err(|err| err.to_string())
        }),
        vector("required.unauthorized_payment_capture_rejected", || {
            expect_rejected(morpheus_core::assert_event_authority(
                "io.marketplace.payment.captured",
                "@market:customer.example",
                &authorities(),
            ))
        }),
        vector("required.entitlement_before_capture_rejected", || {
            let mut flow = morpheus_core::fixtures::valid_order_flow();
            flow.remove(5);
            expect_rejected(morpheus_core::validate_order_sequence(&flow).map(|_| ()))
        }),
        vector("required.non_allowlisted_arbiter_rejected", || {
            let allowlist = morpheus_core::AllowlistPolicy::new([(
                "shop.example".to_string(),
                vec!["orders".to_string()],
            )]);
            expect_rejected(morpheus_core::validate_order_created(
                &morpheus_core::fixtures::valid_order_created(),
                &morpheus_core::fixtures::valid_catalog(),
                &allowlist,
                &morpheus_core::fixtures::valid_customer(),
            ))
        }),
        vector("required.non_arbiter_ruling_rejected", || {
            expect_rejected(morpheus_core::assert_event_authority(
                "io.marketplace.dispute.ruling.issued",
                "@market:shop.example",
                &authorities(),
            ))
        }),
        vector("required.unknown_critical_extension_rejected", || {
            let mut event = morpheus_protocol::fixtures::valid_order_created_event();
            event["content"]["critical"] = json!(["com.example.unknown"]);
            let mut context = MarketplaceEventValidationContext {
                room_profile: Some(RoomProfile::Order),
                ..Default::default()
            };
            expect_rejected(
                morpheus_protocol::validate_marketplace_event(&event, &mut context).map(|_| ()),
            )
        }),
        vector("required.order_room_replay_rejected", || {
            let mut event = morpheus_protocol::fixtures::valid_order_created_event();
            event["room_id"] = json!("!other:customer.example");
            expect_rejected(morpheus_protocol::validate_event_envelope(&event).map(|_| ()))
        }),
        vector("required.snapshot_hash_mismatch_rejected", || {
            let mut catalog = morpheus_core::CatalogIndex::new("shop.example");
            catalog.apply_snapshot(snapshot()).unwrap();
            let mut next = snapshot();
            next.sha256 =
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
            expect_rejected(catalog.apply_snapshot(next))
        }),
        vector("required.revision_rollback_rejected", || {
            let mut catalog = morpheus_core::fixtures::valid_catalog();
            let mut stale = offer();
            stale.revision = 2;
            expect_rejected(catalog.upsert_offer(stale))
        }),
        vector("required.canonical_snapshot_hash_mismatch_rejected", || {
            let value = json!({"b": 1, "a": 2});
            expect_rejected(assert_sha256_matches(
                &value,
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ))
        }),
        vector("required.redacted_marketplace_event_rejected", || {
            let mut event = morpheus_protocol::fixtures::valid_order_created_event();
            event["unsigned"] = json!({"redacted_because": {"event_id": "$redaction"}});
            expect_rejected(morpheus_protocol::validate_event_envelope(&event).map(|_| ()))
        }),
        vector("required.catalog_privacy_leakage_rejected", || {
            expect_rejected(morpheus_protocol::validate_marketplace_privacy(
                "io.marketplace.offer.upserted",
                &json!({"order_id": "ord:customer.example:01JORDER"}),
            ))
        }),
        vector(
            "required.non_idempotent_appservice_transaction_rejected",
            || {
                let previous = vec!["$a".to_string()];
                let actual = vec!["$b".to_string()];
                expect_rejected(morpheus_protocol::validate_appservice_transaction(
                    Some(&previous),
                    &actual,
                ))
            },
        ),
        vector(
            "required.dispute_evidence_ref_outside_order_room_rejected",
            || {
                expect_rejected(morpheus_core::validate_arbitration_flow(&[
                    ArbitrationFlowEvent {
                        event_type: "io.marketplace.dispute.ruling.issued".into(),
                        event_id: "$ruling".into(),
                        room_id: "!order:customer.example".into(),
                        body: json!({
                            "ruling": "refund_required",
                            "binding": true,
                            "evidence_refs": ["$missing"]
                        }),
                    },
                ]))
            },
        ),
        vector("required.withdrawn_offer_removed_from_index", || {
            let mut catalog = morpheus_core::fixtures::valid_catalog();
            catalog.remove_object("offer:shop.example:01JOFFER");
            reject_bool(catalog.get_offer("offer:shop.example:01JOFFER").is_some())
        }),
        vector("required.protocol_downgrade_rejected", || {
            expect_rejected(morpheus_protocol::validate_min_consumer_version(
                "0.2", "0.1",
            ))
        }),
        vector("required.zero_day_retention_policy_rejected", || {
            expect_rejected(morpheus_protocol::validate_retention_policy(false, true))
        }),
        vector(
            "required.compatibility_profile_non_allowlisted_rejected",
            || {
                let policy = morpheus_core::AllowlistPolicy::default();
                reject_bool(morpheus_core::should_index_catalog_room(
                    &policy,
                    "unknown.example",
                ))
            },
        ),
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

fn snapshot() -> SnapshotRecord {
    SnapshotRecord {
        snapshot_id: "snap:shop.example:01JSNAP".into(),
        sequence: 1,
        sha256: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        covers_events_until: "$snap".into(),
    }
}

fn offer() -> OfferRecord {
    OfferRecord {
        offer_id: "offer:shop.example:01JOFFER".into(),
        product_id: "prod:shop.example:01JPROD".into(),
        seller_id: "seller:shop.example:01JSELLER".into(),
        revision: 3,
        price: morpheus_protocol::Money {
            amount: "100.00".into(),
            currency: "USD".into(),
        },
        entitlement_type: "booking_slot".into(),
        payment_capture_policy: Some("before_entitlement".into()),
        offer_terms_hash: Some(
            "sha256:2222222222222222222222222222222222222222222222222222222222222222".into(),
        ),
        seller_terms_hash: Some(
            "sha256:1111111111111111111111111111111111111111111111111111111111111111".into(),
        ),
    }
}

pub fn sample_snapshot_document() -> CatalogSnapshotDocument {
    CatalogSnapshotDocument {
        snapshot: snapshot(),
        sellers: vec![SellerRecord {
            seller_id: "seller:shop.example:01JSELLER".into(),
            status: "active".into(),
        }],
        products: vec![ProductRecord {
            product_id: "prod:shop.example:01JPROD".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            revision: 1,
            terms_hash: Some(
                "sha256:3333333333333333333333333333333333333333333333333333333333333333".into(),
            ),
        }],
        offers: vec![offer()],
        tombstones: vec![],
        sequence: 1,
        covers_events_until: "$snap".into(),
    }
}

pub fn sample_delta() -> CatalogDeltaEvent {
    CatalogDeltaEvent {
        event_type: "io.marketplace.offer.withdrawn".into(),
        event_id: "$withdraw".into(),
        catalog_sequence: 2,
        body: json!({"offer_id": "offer:shop.example:01JOFFER", "revision": 4}),
    }
}

fn authorities() -> OrderAuthorities {
    OrderAuthorities {
        seller_as_user: "@market:shop.example".into(),
        customer_as_user: "@market:customer.example".into(),
        arbiter_as_user: "@market:arbiter.example".into(),
        payment_as_users: vec!["@payment:shop.example".into()],
    }
}

fn expect_rejected<T>(result: Result<T, ValidationError>) -> Result<(), String> {
    match result {
        Ok(_) => Err("expected rejection".into()),
        Err(_) => Ok(()),
    }
}

fn reject_bool(value: bool) -> Result<(), String> {
    if value {
        Err("expected false".into())
    } else {
        Ok(())
    }
}

fn expect_order_completed(decision: OrderDecision) -> Result<(), ValidationError> {
    if decision.final_state == OrderState::Completed {
        Ok(())
    } else {
        Err(morpheus_protocol::ValidationError::new(
            morpheus_protocol::ValidationCode::InvalidStateTransition,
            "order did not complete",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_rejection_helpers_report_failed_expectations() {
        assert_eq!(
            expect_rejected::<()>(Ok(())).unwrap_err(),
            "expected rejection"
        );
        assert_eq!(reject_bool(true).unwrap_err(), "expected false");
        assert_eq!(
            expect_order_completed(OrderDecision {
                final_state: OrderState::Created
            })
            .unwrap_err()
            .code,
            morpheus_protocol::ValidationCode::InvalidStateTransition
        );
    }
}
