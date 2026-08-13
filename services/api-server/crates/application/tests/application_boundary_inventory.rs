use std::fs;
use std::path::{Path, PathBuf};

const DEBT_PATTERNS: &[&str] = &[
    "sqlx::query",
    "sqlx::query_as",
    "sqlx::query_scalar",
    "fms_infrastructure::repositories",
];

#[test]
fn application_services_boundary_debt_inventory_matches_baseline() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let services_dir = manifest_dir.join("src").join("services");

    let mut actual = Vec::new();
    collect_debt_files(&services_dir, &services_dir, &mut actual);
    actual.sort();

    // Database setup/assertion code remains in DB-backed tests. Production
    // application services must not issue SQL or import concrete repositories.
    let expected = [
        "ai_action_proposal_service/tests.rs",
        "domain_action_executor/tests.rs",
        "flight_runtime_service/tests.rs",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();

    assert_eq!(actual, expected, "application service boundary debt inventory changed");
}

#[test]
fn production_application_source_does_not_bypass_domain_data_ports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let application_src = manifest_dir.join("src");

    let mut violations = Vec::new();
    collect_production_debt_files(&application_src, &application_src, &mut violations);
    violations.sort();

    assert!(
        violations.is_empty(),
        "production application source must use fms_domain data ports: {violations:?}"
    );
}

fn collect_debt_files(root: &Path, current: &Path, actual: &mut Vec<String>) {
    let mut entries = fs::read_dir(current)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", current.display()))
        .map(|entry| entry.unwrap_or_else(|err| panic!("failed to read directory entry: {err}")))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_debt_files(root, &path, actual);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }

        let source = fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        if DEBT_PATTERNS.iter().any(|pattern| source.contains(pattern)) {
            actual.push(relative_path(root, &path));
        }
    }
}

fn collect_production_debt_files(root: &Path, current: &Path, actual: &mut Vec<String>) {
    let mut entries = fs::read_dir(current)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", current.display()))
        .map(|entry| entry.unwrap_or_else(|err| panic!("failed to read directory entry: {err}")))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) != Some("tests") {
                collect_production_debt_files(root, &path, actual);
            }
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some("rs")
            || path.file_name().and_then(|name| name.to_str()) == Some("tests.rs")
        {
            continue;
        }

        let source = fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        if DEBT_PATTERNS.iter().any(|pattern| source.contains(pattern)) {
            actual.push(relative_path(root, &path));
        }
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or_else(|err| panic!("failed to strip root {} from {}: {err}", root.display(), path.display()))
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
