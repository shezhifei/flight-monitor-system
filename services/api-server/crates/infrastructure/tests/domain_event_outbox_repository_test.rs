use chrono::Utc;
use fms_domain::events::DomainEventOutboxRow;
use fms_infrastructure::repositories::pg_domain_event_outbox_repository::PgDomainEventOutboxRepository;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};

async fn insert_row(pool: &PgPool, row: &DomainEventOutboxRow) {
    sqlx::query(
        "INSERT INTO domain_event_outbox \
         (event_id, aggregate_type, aggregate_id, event_type, payload, occurred_at, publish_attempts, next_retry_at, source_change_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&row.event_id)
    .bind(row.aggregate_type.as_deref().unwrap_or(""))
    .bind(row.aggregate_id.as_deref().unwrap_or(""))
    .bind(row.event_type.as_deref().unwrap_or(""))
    .bind(&row.payload)
    .bind(row.occurred_at)
    .bind(row.publish_attempts)
    .bind(row.occurred_at)
    .bind(row.source_change_id.as_deref().unwrap_or(""))
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "tests/migrations_outbox")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn mark_published_batch_sets_published_at(pool: PgPool) {
    let repo = PgDomainEventOutboxRepository::new(pool.clone());
    let row = DomainEventOutboxRow {
        event_id: "evt-1".into(),
        aggregate_type: Some("flight".into()),
        aggregate_id: Some("CA123".into()),
        event_type: Some("flight.updated".into()),
        payload: json!({"ok": true}),
        occurred_at: Utc::now(),
        publish_attempts: 0,
        source_change_id: Some("chg-1".into()),
    };
    insert_row(&pool, &row).await;

    let mut tx: Transaction<'_, Postgres> = pool.begin().await.unwrap();
    repo.mark_published_batch(&mut tx, &["evt-1".into()]).await.unwrap();
    tx.commit().await.unwrap();

    let published_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT published_at FROM domain_event_outbox WHERE event_id = $1")
            .bind("evt-1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(published_at.is_some());
}

#[sqlx::test(migrations = "tests/migrations_outbox")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn mark_failed_sets_next_retry_and_last_error(pool: PgPool) {
    let repo = PgDomainEventOutboxRepository::new(pool.clone());
    let row = DomainEventOutboxRow {
        event_id: "evt-2".into(),
        aggregate_type: Some("flight".into()),
        aggregate_id: Some("CA123".into()),
        event_type: Some("flight.updated".into()),
        payload: json!({"ok": true}),
        occurred_at: Utc::now(),
        publish_attempts: 1,
        source_change_id: Some("chg-2".into()),
    };
    insert_row(&pool, &row).await;

    let mut tx: Transaction<'_, Postgres> = pool.begin().await.unwrap();
    repo.mark_failed(&mut tx, &row, "boom", 30).await.unwrap();
    tx.commit().await.unwrap();

    let (attempts, next_retry_at, last_error): (i32, chrono::DateTime<Utc>, Option<String>) = sqlx::query_as(
        "SELECT publish_attempts, next_retry_at, last_error FROM domain_event_outbox WHERE event_id = $1",
    )
    .bind("evt-2")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attempts, 2);
    assert!(last_error.unwrap().contains("boom"));
    assert!(next_retry_at > Utc::now());
}
