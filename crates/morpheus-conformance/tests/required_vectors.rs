#[test]
fn exposes_all_required_v01_vectors() {
    let results = morpheus_conformance::required_vectors().run_all();
    assert_eq!(results.len(), 24);
    assert!(
        results.iter().all(|result| result.status == "passed"),
        "{results:#?}"
    );
    assert_eq!(results[0].id, "required.valid_catalog_snapshot");
    assert_eq!(
        results[23].id,
        "required.compatibility_profile_non_allowlisted_rejected"
    );
}

#[test]
fn runner_reports_failed_vectors_and_fixture_helpers_are_stable() {
    let runner = morpheus_conformance::ConformanceRunner::new(vec![
        morpheus_conformance::ConformanceVector {
            id: "failing.vector".into(),
            group: "contract".into(),
            run: Box::new(|| Err("boom".into())),
        },
    ]);
    let result = runner.run_all().remove(0);
    assert_eq!(result.id, "failing.vector");
    assert_eq!(result.group, "contract");
    assert_eq!(result.status, "failed");
    assert_eq!(result.message.as_deref(), Some("boom"));

    let snapshot = morpheus_conformance::sample_snapshot_document();
    assert_eq!(snapshot.products.len(), 1);
    assert_eq!(
        morpheus_conformance::sample_delta().event_type,
        "io.marketplace.offer.withdrawn"
    );
}
