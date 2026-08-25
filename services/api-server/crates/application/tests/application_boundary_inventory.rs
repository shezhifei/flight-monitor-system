use std::fs;
use std::path::{Path, PathBuf};

const DEBT_PATTERNS: &[&str] = &[
    "sqlx::query",
    "sqlx::query_as",
    "sqlx::query_scalar",
    "fms_infrastructure::repositories",
    "PgPool",
    "Transaction<",
    "Postgres",
    // `Sqlx` 前缀曾是 10 个别名 trait（Sqlx*TransactionalRepository）的专属标记，那些
    // trait 把 `Transaction<'tx, Postgres>` 从签名里藏起来，绕过 `Postgres` / `Transaction<`
    // 的扫描。别名 trait 已随生产 sqlx 依赖一并删除；模式保留，防止同类变通回流。
    "Sqlx",
];

#[test]
fn application_services_boundary_debt_inventory_matches_baseline() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let services_dir = manifest_dir.join("src").join("services");

    let mut actual = Vec::new();
    collect_debt_files(&services_dir, &services_dir, &mut actual);
    actual.sort();

    // The living inventory of application services that still reach past the
    // domain data ports. This list may only ever SHRINK: P3 deletes the
    // `application -> fms-infrastructure`/`sqlx` edge one service at a time, and
    // each such step must remove a line here. A new entry means new debt.
    let expected = [
        "ai_action_proposal_service/tests.rs",
        "ai_execution_readiness_service.rs",
        "ai_job_timeout_reaper_service.rs",
        "ai_runtime_service/ai_execution_control_service.rs",
        "ai_runtime_service/compensation_planner.rs",
        "ai_runtime_service/in_memory_repos.rs",
        "ai_runtime_service/recovery_orchestrator.rs",
        "ai_runtime_service/rollback_service.rs",
        "dispatch_chat_service.rs",
        "domain_action_executor/tests.rs",
        "flight_runtime_service/tests.rs",
        "in_memory_ai_proposal_repository.rs",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();

    assert_eq!(actual, expected, "application service boundary debt inventory changed");
}

#[test]
// P1 与 P2 都已完成，这个断言**仍然是红的**——因为 P3（application 层不再持有数据库类型）
// 还没做完。原来的 ignore 理由写的是「until P1 resolves boundary violations」，P1 早已落地
// 而红灯依旧，那条理由已经变成假话；让守门说真话是 P0 的全部内容，所以这里改成真实的阻塞项。
// 解除 ignore 的条件只有一个：下面的清单降到 0，而不是清单被改。
#[ignore = "P3 未完成：application 层仍有 20 个文件持有 sqlx 类型；清单降到 0 时解除"]
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
