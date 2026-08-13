use chrono::{Duration, Utc};
use fms_domain::models::dispatch::{dispatch_overrun_dedupe_key, AlertSeverity, DispatchAlert};
use fms_domain::ports::dispatch_repository::DispatchAlertRepository;
use fms_infrastructure::repositories::pg_dispatch_alert_repository::PgDispatchAlertRepository;
use serde_json::json;
use sqlx::PgPool;

fn overrun_alert(dedupe_key: &str, current: &str, next: &str) -> DispatchAlert {
    DispatchAlert {
        id: ulid::Ulid::new().to_string(),
        flight_id: Some("flight-1".to_string()),
        task_type: Some("boarding".to_string()),
        alert_type: "dispatch_schedule_overrun".to_string(),
        severity: AlertSeverity::Warning,
        message: "共享人员冲突预警".to_string(),
        is_resolved: false,
        resolved_at: None,
        resolved_by: None,
        resolution_notes: None,
        notify_users: vec!["user-1".to_string()],
        created_at: Some(Utc::now()),
        dedupe_key: Some(dedupe_key.to_string()),
        current_order_id: Some(current.to_string()),
        next_order_id: Some(next.to_string()),
        last_detected_at: Some(Utc::now()),
        occurrence_count: 1,
        acknowledged_at: None,
        acknowledged_by: None,
        details: json!({
            "shared_personnel": ["user-1"],
            "predicted_conflict_minutes": 12,
            "eta_missing": false,
        }),
    }
}

#[sqlx::test(migrations = "tests/migrations_dispatch_alerts")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn upsert_overrun_deduplicates_by_key(pool: PgPool) {
    let repo = PgDispatchAlertRepository::new(pool);
    let key = dispatch_overrun_dedupe_key("do-1", "do-2");

    let first = overrun_alert(&key, "do-1", "do-2");
    let outcome = repo.upsert_overrun(&first).await.unwrap();
    assert!(outcome.inserted, "first write must insert");
    assert!(!outcome.reopened);
    let first_id = outcome.alert.id.clone();

    let mut second = overrun_alert(&key, "do-1", "do-2");
    second.id = ulid::Ulid::new().to_string();
    second.message = "共享人员冲突预警(已更新)".to_string();
    let outcome = repo.upsert_overrun(&second).await.unwrap();
    assert!(!outcome.inserted, "second write must update, not insert");
    assert!(!outcome.reopened);
    assert_eq!(outcome.alert.id, first_id, "same dedupe_key must reuse the alert id");
    assert_eq!(outcome.alert.occurrence_count, 1, "active conflict keeps occurrence");

    let loaded = repo.find_by_id(&first_id).await.unwrap().expect("alert exists");
    assert_eq!(loaded.message, "共享人员冲突预警(已更新)");
    assert!(!loaded.is_resolved);
}

#[sqlx::test(migrations = "tests/migrations_dispatch_alerts")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn upsert_overrun_reopens_resolved_alert_with_incremented_occurrence(pool: PgPool) {
    let repo = PgDispatchAlertRepository::new(pool);
    let key = dispatch_overrun_dedupe_key("do-1", "do-2");

    let outcome = repo.upsert_overrun(&overrun_alert(&key, "do-1", "do-2")).await.unwrap();
    let alert_id = outcome.alert.id.clone();
    assert!(repo.resolve(&alert_id, "user-1", None).await.unwrap());

    let mut again = overrun_alert(&key, "do-1", "do-2");
    again.id = ulid::Ulid::new().to_string();
    let outcome = repo.upsert_overrun(&again).await.unwrap();
    assert!(!outcome.inserted);
    assert!(outcome.reopened, "reappearing conflict must reopen the alert");
    assert_eq!(outcome.alert.id, alert_id);
    assert_eq!(outcome.alert.occurrence_count, 2, "occurrence must increment");
    assert!(!outcome.alert.is_resolved);
    assert!(
        outcome.alert.acknowledged_at.is_none(),
        "ack state must be cleared on reopen"
    );
    assert!(outcome.alert.acknowledged_by.is_none());
    assert!(outcome.alert.resolved_at.is_none());
}

#[sqlx::test(migrations = "tests/migrations_dispatch_alerts")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn acknowledge_marks_seen_without_resolving(pool: PgPool) {
    let repo = PgDispatchAlertRepository::new(pool);
    let key = dispatch_overrun_dedupe_key("do-1", "do-2");

    let outcome = repo.upsert_overrun(&overrun_alert(&key, "do-1", "do-2")).await.unwrap();
    let alert_id = outcome.alert.id.clone();

    assert!(repo.acknowledge(&alert_id, "user-2").await.unwrap());
    let loaded = repo.find_by_id(&alert_id).await.unwrap().expect("alert exists");
    assert!(!loaded.is_resolved, "acknowledge must not resolve");
    assert_eq!(loaded.acknowledged_by.as_deref(), Some("user-2"));
    assert!(loaded.acknowledged_at.is_some());

    assert!(repo.resolve(&alert_id, "user-1", None).await.unwrap());
    assert!(
        !repo.acknowledge(&alert_id, "user-1").await.unwrap(),
        "acknowledge on a resolved alert must fail"
    );
}

#[sqlx::test(migrations = "tests/migrations_dispatch_alerts")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn upsert_overrun_refreshes_last_detected_at(pool: PgPool) {
    let repo = PgDispatchAlertRepository::new(pool);
    let key = dispatch_overrun_dedupe_key("do-1", "do-2");

    let mut alert = overrun_alert(&key, "do-1", "do-2");
    alert.last_detected_at = Some(Utc::now() - Duration::minutes(5));
    let outcome = repo.upsert_overrun(&alert).await.unwrap();
    let alert_id = outcome.alert.id.clone();

    let mut later = overrun_alert(&key, "do-1", "do-2");
    later.id = ulid::Ulid::new().to_string();
    later.last_detected_at = Some(Utc::now());
    let outcome = repo.upsert_overrun(&later).await.unwrap();
    assert!(!outcome.inserted);

    let loaded = repo.find_by_id(&alert_id).await.unwrap().expect("alert exists");
    assert!(loaded.last_detected_at.unwrap() > alert.last_detected_at.unwrap());
}

#[sqlx::test(migrations = "tests/migrations_dispatch_alerts")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn find_unresolved_filters_by_flight(pool: PgPool) {
    let repo = PgDispatchAlertRepository::new(pool);
    repo.upsert_overrun(&overrun_alert(
        &dispatch_overrun_dedupe_key("do-1", "do-2"),
        "do-1",
        "do-2",
    ))
    .await
    .unwrap();

    let all = repo.find_unresolved(None).await.unwrap();
    assert_eq!(all.len(), 1);

    let none = repo.find_unresolved(Some("other-flight")).await.unwrap();
    assert!(none.is_empty());
}
