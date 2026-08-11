//! Regression tests for `PgBusinessCaseRepository::find_by_id` workflow_receipt behaviour.
//!
//! These tests guard against a previous design where `workflow_receipt` was stored
//! as a JSON snapshot inside `flight_business_cases.context`. The snapshot
//! frequently diverged from the live `notifications` table — e.g. it surfaced
//! raw ULIDs as recipient identifiers because the snapshot only had
//! `user_id` (no joined `users.username`).
//!
//! After the refactor, `find_by_id` reads the live data via LATERAL JOINs
//! from `business_case_workflow_runs` → `notifications` (and snapshots
//! like `recipient_username_snapshot`). These tests assert the contract:
//!
//!   1. `case.workflow_receipt` is populated from `notifications`, not from
//!      stale `context.workflow_receipt` data.
//!   2. A stale snapshot inside `context` is *ignored* — even if it contains
//!      different data, the live notification data wins.
//!   3. Summary counts (pending/acknowledged/rejected/total) are derived
//!      from `notifications.ack_status`.
//!   4. `recipient_username` falls back through
//!      `recipient_username_snapshot` → `recipient_display_name_snapshot`
//!      → "未知账号".
//!   5. A case without a `business_case_workflow_runs` row has
//!      `workflow_receipt = None`.

use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_infrastructure::repositories::pg_business_case_repository::PgBusinessCaseRepository;
use serde_json::json;
use sqlx::{PgPool, Row};

async fn insert_user(pool: &PgPool, id: &str, username: &str, display_name: Option<&str>, department: Option<&str>) {
    sqlx::query(
        r#"
        INSERT INTO users (id, username, display_name, department, is_active, is_admin, created_at, updated_at)
        VALUES ($1, $2, $3, $4, TRUE, FALSE, NOW(), NOW())
        "#,
    )
    .bind(id)
    .bind(username)
    .bind(display_name)
    .bind(department)
    .execute(pool)
    .await
    .expect("insert user");
}

async fn insert_flight(pool: &PgPool, flight_id: &str, flight_number: &str) {
    sqlx::query(
        r#"
        INSERT INTO flights (flight_id, flight_number, created_at)
        VALUES ($1, $2, NOW())
        "#,
    )
    .bind(flight_id)
    .bind(flight_number)
    .execute(pool)
    .await
    .expect("insert flight");
}

async fn insert_business_case(pool: &PgPool, case_id: &str, flight_id: &str, context: serde_json::Value) {
    sqlx::query(
        r#"
        INSERT INTO flight_business_cases
            (case_id, flight_id, case_type, description, context, status,
             created_by, updated_by, created_at, log, visibility_scope)
        VALUES ($1, $2, 'gate_baggage_check', 'test', $3, 'PENDING',
                'tester', 'tester', NOW(), ARRAY[]::TEXT[], 'COMMON')
        "#,
    )
    .bind(case_id)
    .bind(flight_id)
    .bind(context)
    .execute(pool)
    .await
    .expect("insert business case");
}

async fn insert_workflow_run(pool: &PgPool, run_id: &str, case_id: &str, flight_id: &str, receipt_group_id: &str) {
    sqlx::query(
        r#"
        INSERT INTO business_case_workflow_runs
            (run_id, template_code, case_id, flight_id, process_definition_key, process_instance_id,
             receipt_group_id, status, started_by, created_at, updated_at)
        VALUES ($1, 'gate_baggage_check.v1', $2, $3, 'def-key', 'proc-' || $1,
                $4, 'pending', 'tester', NOW(), NOW())
        "#,
    )
    .bind(run_id)
    .bind(case_id)
    .bind(flight_id)
    .bind(receipt_group_id)
    .execute(pool)
    .await
    .expect("insert workflow run");
}

#[allow(clippy::too_many_arguments)]
async fn insert_notification(
    pool: &PgPool,
    notification_id: &str,
    user_id: &str,
    title: &str,
    severity: &str,
    receipt_group_id: &str,
    ack_status: &str,
    ack_at: Option<&str>,
    recipient_username_snapshot: Option<&str>,
    recipient_display_name_snapshot: Option<&str>,
) {
    let ack_at_value: Option<chrono::DateTime<chrono::Utc>> = ack_at
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&chrono::Utc));
    sqlx::query(
        r#"
        INSERT INTO notifications
            (notification_id, user_id, title, body, category, severity, is_read,
             origin_type, receipt_required, receipt_group_id,
             recipient_username_snapshot, recipient_display_name_snapshot,
             recipient_department_snapshot, recipient_job_title_snapshot,
             delivery_status, ack_status, ack_at, created_at)
        VALUES ($1, $2, $3, '', 'system', $4, FALSE,
                'workflow', TRUE, $5,
                $6, $7, '运行控制', '签派员',
                'delivered', $8, $9, NOW())
        "#,
    )
    .bind(notification_id)
    .bind(user_id)
    .bind(title)
    .bind(severity)
    .bind(receipt_group_id)
    .bind(recipient_username_snapshot)
    .bind(recipient_display_name_snapshot)
    .bind(ack_status)
    .bind(ack_at_value)
    .execute(pool)
    .await
    .expect("insert notification");
}

#[ignore = "requires DATABASE_URL with PostgreSQL"]
#[sqlx::test(migrations = "tests/migrations_business_case_receipt")]
async fn find_by_id_populates_workflow_receipt_from_notifications(pool: PgPool) {
    insert_user(&pool, "u-alice", "alice", Some("Alice"), Some("运行控制")).await;
    insert_user(&pool, "u-bob", "bob", Some("Bob"), Some("机务")).await;
    insert_flight(&pool, "FL001", "CZ1234").await;
    insert_business_case(&pool, "case-1", "FL001", json!({"some_other_key": "untouched"})).await;
    insert_workflow_run(&pool, "run-1", "case-1", "FL001", "rg-1").await;
    insert_notification(
        &pool,
        "n-1",
        "u-alice",
        "机位确认",
        "warning",
        "rg-1",
        "pending",
        None,
        Some("alice"),
        Some("Alice"),
    )
    .await;
    insert_notification(
        &pool,
        "n-2",
        "u-bob",
        "机位确认",
        "warning",
        "rg-1",
        "acknowledged",
        Some("2026-04-23T07:20:00Z"),
        Some("bob"),
        Some("Bob"),
    )
    .await;

    let repo = PgBusinessCaseRepository::new(pool);
    let case = repo.find_by_id("case-1").await.expect("query").expect("case exists");

    let receipt = case
        .workflow_receipt
        .as_ref()
        .expect("workflow_receipt populated from notifications");
    assert_eq!(receipt.receipt_group_id, "rg-1");
    assert_eq!(receipt.title.as_deref(), Some("机位确认"));
    assert_eq!(receipt.severity.as_deref(), Some("warning"));
    assert_eq!(receipt.summary.total_count, 2);
    assert_eq!(receipt.summary.pending_count, 1);
    assert_eq!(receipt.summary.acknowledged_count, 1);
    assert_eq!(receipt.summary.rejected_count, 0);
    assert_eq!(receipt.summary.overall_status, "pending");
    assert_eq!(receipt.items.len(), 2);

    let alice = receipt
        .items
        .iter()
        .find(|item| item.user_id == "u-alice")
        .expect("alice item present");
    assert_eq!(alice.recipient_username.as_deref(), Some("alice"));
    assert_eq!(alice.recipient_display_name.as_deref(), Some("Alice"));
    assert_eq!(alice.ack_status, "pending");
    assert!(alice.ack_at.is_none());

    let bob = receipt
        .items
        .iter()
        .find(|item| item.user_id == "u-bob")
        .expect("bob item present");
    assert_eq!(bob.recipient_username.as_deref(), Some("bob"));
    assert_eq!(bob.ack_status, "acknowledged");
    assert!(bob.ack_at.is_some());

    // Other context keys must be preserved untouched.
    assert_eq!(case.context.get("some_other_key"), Some(&json!("untouched")));
    // The legacy snapshot key MUST NOT have reappeared.
    assert!(case.context.get("workflow_receipt").is_none());
}

#[ignore = "requires DATABASE_URL with PostgreSQL"]
#[sqlx::test(migrations = "tests/migrations_business_case_receipt")]
async fn find_by_id_ignores_stale_context_workflow_receipt(pool: PgPool) {
    insert_user(&pool, "u-carol", "carol", Some("Carol"), Some("运行控制")).await;
    insert_flight(&pool, "FL002", "CZ5678").await;
    // Seed a stale workflow_receipt blob inside context — this simulates a
    // pre-migration row that still has the old snapshot in its JSONB.
    let stale_snapshot = json!({
        "receipt_group_id": "rg-stale",
        "title": "STALE TITLE — should be ignored",
        "severity": "critical",
        "origin_type": "workflow",
        "items": [
            {
                "user_id": "u-stale",
                "recipient_username": "STALE_RECIPIENT",
                "ack_status": "rejected"
            }
        ]
    });
    insert_business_case(&pool, "case-2", "FL002", json!({ "workflow_receipt": stale_snapshot })).await;
    insert_workflow_run(&pool, "run-2", "case-2", "FL002", "rg-live").await;
    insert_notification(
        &pool,
        "n-live-1",
        "u-carol",
        "LIVE TITLE",
        "info",
        "rg-live",
        "pending",
        None,
        Some("carol"),
        Some("Carol"),
    )
    .await;

    let repo = PgBusinessCaseRepository::new(pool);
    let case = repo.find_by_id("case-2").await.expect("query").expect("case exists");

    let receipt = case.workflow_receipt.as_ref().expect("workflow_receipt from live data");
    // Live data wins; the stale snapshot is fully ignored.
    assert_eq!(receipt.receipt_group_id, "rg-live");
    assert_eq!(receipt.title.as_deref(), Some("LIVE TITLE"));
    assert_eq!(receipt.severity.as_deref(), Some("info"));
    assert_eq!(receipt.items.len(), 1);
    assert_eq!(receipt.items[0].user_id, "u-carol");
    assert_eq!(receipt.items[0].recipient_username.as_deref(), Some("carol"));
    assert_eq!(receipt.items[0].ack_status, "pending");
}

#[ignore = "requires DATABASE_URL with PostgreSQL"]
#[sqlx::test(migrations = "tests/migrations_business_case_receipt")]
async fn find_by_id_falls_back_to_display_name_when_username_snapshot_empty(pool: PgPool) {
    insert_user(&pool, "u-dave", "dave", Some("Dave Chen"), Some("运行控制")).await;
    insert_flight(&pool, "FL003", "CZ9999").await;
    insert_business_case(&pool, "case-3", "FL003", json!({})).await;
    insert_workflow_run(&pool, "run-3", "case-3", "FL003", "rg-3").await;
    // Empty username snapshot, only display_name is set.
    insert_notification(
        &pool,
        "n-3",
        "u-dave",
        "Test",
        "info",
        "rg-3",
        "pending",
        None,
        Some(""),
        Some("Dave Chen"),
    )
    .await;

    let repo = PgBusinessCaseRepository::new(pool);
    let case = repo.find_by_id("case-3").await.expect("query").expect("case exists");

    let item = &case.workflow_receipt.as_ref().unwrap().items[0];
    assert_eq!(item.recipient_username.as_deref(), Some("Dave Chen"));
}

#[ignore = "requires DATABASE_URL with PostgreSQL"]
#[sqlx::test(migrations = "tests/migrations_business_case_receipt")]
async fn find_by_id_reports_unknown_account_when_no_username_or_display_name(pool: PgPool) {
    insert_user(&pool, "u-eve", "eve", None, None).await;
    insert_flight(&pool, "FL004", "CZ0001").await;
    insert_business_case(&pool, "case-4", "FL004", json!({})).await;
    insert_workflow_run(&pool, "run-4", "case-4", "FL004", "rg-4").await;
    insert_notification(
        &pool, "n-4", "u-eve", "Test", "info", "rg-4", "pending", None, None, None,
    )
    .await;

    let repo = PgBusinessCaseRepository::new(pool);
    let case = repo.find_by_id("case-4").await.expect("query").expect("case exists");

    let item = &case.workflow_receipt.as_ref().unwrap().items[0];
    // Hard guard: the historical bug surfaced the user_id ULID here.
    // We assert that the raw ULID is NOT leaked and that we fall back to
    // the localized "未知账号" sentinel instead.
    assert_ne!(item.recipient_username.as_deref(), Some("u-eve"));
    assert_eq!(item.recipient_username.as_deref(), Some("未知账号"));
}

#[ignore = "requires DATABASE_URL with PostgreSQL"]
#[sqlx::test(migrations = "tests/migrations_business_case_receipt")]
async fn find_by_id_returns_none_when_no_workflow_run(pool: PgPool) {
    insert_flight(&pool, "FL005", "CZ0002").await;
    insert_business_case(&pool, "case-5", "FL005", json!({})).await;
    // Note: no insert_workflow_run call.

    let repo = PgBusinessCaseRepository::new(pool);
    let case = repo.find_by_id("case-5").await.expect("query").expect("case exists");

    assert!(case.workflow_receipt.is_none());
    assert!(case.context.get("workflow_receipt").is_none());
}

#[ignore = "requires DATABASE_URL with PostgreSQL"]
#[sqlx::test(migrations = "tests/migrations_business_case_receipt")]
async fn find_by_id_summary_counts_rejected_status(pool: PgPool) {
    insert_user(&pool, "u-1", "user1", Some("User1"), Some("运行控制")).await;
    insert_user(&pool, "u-2", "user2", Some("User2"), Some("运行控制")).await;
    insert_user(&pool, "u-3", "user3", Some("User3"), Some("运行控制")).await;
    insert_flight(&pool, "FL006", "CZ0003").await;
    insert_business_case(&pool, "case-6", "FL006", json!({})).await;
    insert_workflow_run(&pool, "run-6", "case-6", "FL006", "rg-6").await;
    insert_notification(
        &pool,
        "n-6-1",
        "u-1",
        "T",
        "info",
        "rg-6",
        "pending",
        None,
        Some("u1"),
        Some("U1"),
    )
    .await;
    insert_notification(
        &pool,
        "n-6-2",
        "u-2",
        "T",
        "info",
        "rg-6",
        "rejected",
        Some("2026-04-23T07:00:00Z"),
        Some("u2"),
        Some("U2"),
    )
    .await;
    insert_notification(
        &pool,
        "n-6-3",
        "u-3",
        "T",
        "info",
        "rg-6",
        "acknowledged",
        Some("2026-04-23T07:10:00Z"),
        Some("u3"),
        Some("U3"),
    )
    .await;

    let repo = PgBusinessCaseRepository::new(pool);
    let case = repo.find_by_id("case-6").await.expect("query").expect("case exists");
    let summary = &case.workflow_receipt.as_ref().unwrap().summary;
    assert_eq!(summary.total_count, 3);
    assert_eq!(summary.pending_count, 1);
    assert_eq!(summary.acknowledged_count, 1);
    assert_eq!(summary.rejected_count, 1);
    // Any rejected recipient should mark the overall receipt as "rejected".
    assert_eq!(summary.overall_status, "rejected");
}

#[ignore = "requires DATABASE_URL with PostgreSQL"]
#[sqlx::test(migrations = "tests/migrations_business_case_receipt")]
async fn no_row_in_flight_business_cases_has_workflow_receipt_in_context_after_scrub(pool: PgPool) {
    // Defensive end-to-end check: after a scrub equivalent to migration 099
    // (removing `workflow_receipt` from the context JSONB), no row should
    // leak the legacy key.
    insert_user(&pool, "u-z", "z", Some("Z"), Some("x")).await;
    insert_flight(&pool, "FL-Z", "CZ-Z").await;
    insert_business_case(
        &pool,
        "case-z",
        "FL-Z",
        json!({
            "workflow_receipt": { "receipt_group_id": "rg-z", "stale": true },
            "other": "kept",
        }),
    )
    .await;
    insert_workflow_run(&pool, "run-z", "case-z", "FL-Z", "rg-z").await;
    insert_notification(
        &pool,
        "n-z",
        "u-z",
        "T",
        "info",
        "rg-z",
        "pending",
        None,
        Some("z"),
        Some("Z"),
    )
    .await;

    // Simulate the migration: scrub the legacy key.
    sqlx::query(
        r#"UPDATE flight_business_cases SET context = context - 'workflow_receipt'
           WHERE context ? 'workflow_receipt'"#,
    )
    .execute(&pool)
    .await
    .expect("scrub");

    // Legacy key gone from the persisted row (query before consuming the pool
    // by handing it to the repository).
    let row_count: i64 =
        sqlx::query("SELECT COUNT(*)::bigint AS c FROM flight_business_cases WHERE context ? 'workflow_receipt'")
            .fetch_one(&pool)
            .await
            .expect("count")
            .try_get("c")
            .expect("c column");
    assert_eq!(row_count, 0, "no row should have workflow_receipt in context");

    let repo = PgBusinessCaseRepository::new(pool);
    let case = repo.find_by_id("case-z").await.expect("query").expect("exists");
    // ...and the live read still works.
    assert!(case.workflow_receipt.is_some());
    assert_eq!(case.workflow_receipt.as_ref().unwrap().receipt_group_id, "rg-z");
    // Other context keys preserved.
    assert_eq!(case.context.get("other"), Some(&json!("kept")));
}
