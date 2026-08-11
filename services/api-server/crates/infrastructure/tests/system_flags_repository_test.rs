use fms_domain::ports::system_flags_repository::SystemFlagsRepository;
use fms_infrastructure::repositories::pg_system_flags_repository::PgSystemFlagsRepository;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "tests/migrations_system_flags")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn snapshot_round_trip_round_trips_keys(pool: PgPool) {
    let repo = PgSystemFlagsRepository::new(pool);
    repo.replace_all(&json!({"a":{"b":1}}).as_object().unwrap().clone())
        .await
        .unwrap();
    let loaded = repo.load().await.unwrap();
    assert_eq!(loaded.get("a").and_then(|v| v.get("b")), Some(&json!(1)));
}

#[sqlx::test(migrations = "tests/migrations_system_flags")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn replace_all_overwrites_existing_rows(pool: PgPool) {
    let repo = PgSystemFlagsRepository::new(pool);
    repo.replace_all(&json!({"x":1}).as_object().unwrap().clone())
        .await
        .unwrap();
    repo.replace_all(&json!({"y":2}).as_object().unwrap().clone())
        .await
        .unwrap();
    let loaded = repo.load().await.unwrap();
    assert_eq!(loaded.get("x"), None);
    assert_eq!(loaded.get("y"), Some(&json!(2)));
}
