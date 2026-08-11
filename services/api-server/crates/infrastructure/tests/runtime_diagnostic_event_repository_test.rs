use fms_infrastructure::repositories::pg_runtime_diagnostic_event_repository::PgRuntimeDiagnosticEventRepository;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "tests/migrations")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn fetch_recent_returns_payload_in_desc_order(pool: PgPool) {
    let repo = PgRuntimeDiagnosticEventRepository::new(pool);
    repo.insert("shadow_compare", "shadow.diff", json!({"path":"/api/v2/x"}), None)
        .await
        .unwrap();
    repo.insert("shadow_compare", "shadow.diff", json!({"path":"/api/v2/y"}), None)
        .await
        .unwrap();
    let rows = repo.fetch_recent("shadow_compare", 10).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("path"), Some(&json!("/api/v2/y")));
}

#[sqlx::test(migrations = "tests/migrations")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn fetch_recent_respects_limit(pool: PgPool) {
    let repo = PgRuntimeDiagnosticEventRepository::new(pool);
    for i in 0..5 {
        repo.insert("t", "e", json!({"i": i}), None).await.unwrap();
    }
    let rows = repo.fetch_recent("t", 3).await.unwrap();
    assert_eq!(rows.len(), 3);
}

#[sqlx::test(migrations = "tests/migrations")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn count_by_topic_returns_correct_count(pool: PgPool) {
    let repo = PgRuntimeDiagnosticEventRepository::new(pool);
    repo.insert("a", "e", json!({}), None).await.unwrap();
    repo.insert("a", "e", json!({}), None).await.unwrap();
    repo.insert("b", "e", json!({}), None).await.unwrap();
    assert_eq!(repo.count_by_topic("a").await.unwrap(), 2);
    assert_eq!(repo.count_by_topic("b").await.unwrap(), 1);
}

#[sqlx::test(migrations = "tests/migrations")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn ping_returns_true(pool: PgPool) {
    let repo = PgRuntimeDiagnosticEventRepository::new(pool);
    assert!(repo.ping().await.unwrap());
}

#[sqlx::test(migrations = "tests/migrations")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn insert_on_conflict_does_nothing(pool: PgPool) {
    let repo = PgRuntimeDiagnosticEventRepository::new(pool);
    repo.insert("t", "e", json!({"v": 1}), Some("dup-id".into()))
        .await
        .unwrap();
    repo.insert("t", "e", json!({"v": 2}), Some("dup-id".into()))
        .await
        .unwrap();
    let rows = repo.fetch_recent("t", 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("v"), Some(&json!(1)));
}
