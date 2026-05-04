#[test]
fn exposes_all_required_v01_vectors() {
    let results = morpheus_conformance::required_vectors().run_all();
    assert_eq!(results.len(), 15);
    assert!(
        results.iter().all(|result| result.status == "passed"),
        "{results:#?}"
    );
    assert_eq!(results[0].id, "required.valid_catalog_snapshot");
    assert_eq!(results[14].id, "required.revision_rollback_rejected");
}
