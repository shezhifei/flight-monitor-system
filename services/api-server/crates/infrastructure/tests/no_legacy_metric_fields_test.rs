#[test]
fn no_legacy_metric_fields_remain() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_path = manifest_dir.join("src/repositories/pg_todo_agent_context_repository.rs");
    let source = std::fs::read_to_string(&source_path).expect("read source file");
    let test_marker = "#[cfg(test)]";
    let production_source = &source[..source.find(test_marker).unwrap_or(source.len())];
    assert!(
        !production_source.contains("fn get_legacy_hits"),
        "get_legacy_hits should not exist"
    );
    assert!(
        !production_source.contains("fn batch_get_legacy_hits"),
        "batch_get_legacy_hits should not exist"
    );
    assert!(
        !production_source.contains("find_todo_ids_legacy_preferred_calls"),
        "find_todo_ids_legacy_preferred_calls should not exist"
    );
    assert!(
        !production_source.contains("legacy_retired"),
        "legacy_retired should not exist"
    );
}
