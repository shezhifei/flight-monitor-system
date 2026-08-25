use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fms_domain::error::DomainError;
use fms_domain::events::DomainEventOutboxRow;
use fms_domain::ports::domain_event_outbox_repository::{
    DomainEventOutboxRepository, DomainEventOutboxTransactionalRepository,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PgDomainEventOutboxRepository {
    pool: PgPool,
}

impl PgDomainEventOutboxRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a domain event into the outbox within an existing transaction.
    /// `event_id` and `occurred_at` are generated internally; `source_change_id`
    /// is provided by the caller to keep CDC decoding column-aligned.
    /// Associated function — does not require a repository instance, only the transaction.
    /// 一条 INSERT，执行器由调用方给：事务里给 `&mut **tx`，事务外给 `&self.pool`。
    /// 原先它只接受事务，于是「往 outbox 写一行」这件事在事务外就没有走法，
    /// 逼出了下面 `insert_event_auto` 那种为单条语句开事务的写法。
    pub async fn insert_event<'e, E>(
        executor: E,
        aggregate_type: &str,
        aggregate_id: &str,
        event_type: &str,
        payload: Value,
        source_change_id: &str,
    ) -> Result<String, sqlx::Error>
    where
        E: sqlx::Executor<'e, Database = Postgres>,
    {
        let event_id = ulid::Ulid::new().to_string();
        let occurred_at = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO domain_event_outbox (
                event_id, aggregate_type, aggregate_id, event_type,
                payload, occurred_at, publish_attempts, source_change_id
            ) VALUES ($1, $2, $3, $4, $5, $6, 0, $7)
            "#,
        )
        .bind(&event_id)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(event_type)
        .bind(payload)
        .bind(occurred_at)
        .bind(source_change_id)
        .execute(executor)
        .await?;
        Ok(event_id)
    }

    /// Insert a domain event into the outbox with auto-commit (no caller transaction).
    /// Used by publishers that do not share a business-write transaction.
    pub async fn insert_event_auto(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
        event_type: &str,
        payload: Value,
        source_change_id: &str,
    ) -> Result<String, sqlx::Error> {
        Self::insert_event(
            &self.pool,
            aggregate_type,
            aggregate_id,
            event_type,
            payload,
            source_change_id,
        )
        .await
    }

    pub async fn mark_published_batch(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event_ids: &[String],
    ) -> Result<(), sqlx::Error> {
        if event_ids.is_empty() {
            return Ok(());
        }

        sqlx::query(
            r#"
            UPDATE domain_event_outbox
            SET published_at = NOW(),
                publish_attempts = publish_attempts + 1,
                last_error = NULL
            WHERE event_id = ANY($1)
            "#,
        )
        .bind(event_ids)
        .execute(&mut **tx)
        .await?;

        self.refresh_pending_gauge().await;
        Ok(())
    }

    pub async fn mark_failed(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        row: &DomainEventOutboxRow,
        error: &str,
        backoff_seconds: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE domain_event_outbox
            SET publish_attempts = publish_attempts + 1,
                next_retry_at = NOW() + make_interval(secs => $1),
                last_error = $2
            WHERE event_id = $3
            "#,
        )
        .bind(backoff_seconds)
        .bind(error)
        .bind(&row.event_id)
        .execute(&mut **tx)
        .await?;

        self.refresh_pending_gauge().await;
        Ok(())
    }

    /// 重新统计未发布事件数并刷新 `fms_outbox_pending_events` (Gauge)。
    async fn refresh_pending_gauge(&self) {
        match self.count_unpublished().await {
            Ok(count) => metrics::gauge!("fms_outbox_pending_events").set(count as f64),
            Err(_) => {}
        }
    }

    /// Claim pending rows inside an existing transaction (holds `FOR UPDATE` locks).
    /// Used by the SQL recovery relay which publishes and marks status in the same tx.
    pub async fn claim_pending_for_relay_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        limit: i64,
    ) -> Result<Vec<DomainEventOutboxRow>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT
                event_id,
                aggregate_type,
                aggregate_id,
                event_type,
                payload,
                occurred_at,
                publish_attempts,
                source_change_id
            FROM domain_event_outbox
            WHERE published_at IS NULL
              AND next_retry_at <= NOW()
            ORDER BY occurred_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT $1
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| DomainError::Internal(format!("failed to lock pending domain event outbox rows: {error}")))?;

        rows.into_iter().map(|row| decode_outbox_row(row)).collect()
    }
}

#[async_trait]
impl DomainEventOutboxRepository for PgDomainEventOutboxRepository {
    async fn insert_event(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
        event_type: &str,
        payload: Value,
        source_change_id: &str,
    ) -> Result<String, DomainError> {
        Self::insert_event(&self.pool, aggregate_type, aggregate_id, event_type, payload, source_change_id)
            .await
            .map_err(|e| DomainError::Internal(format!("failed to insert domain event outbox row: {e}")))
    }

    async fn claim_pending_for_relay(&self, limit: i64) -> Result<Vec<DomainEventOutboxRow>, DomainError> {
        let mut tx = self.pool.begin().await.map_err(|error| {
            DomainError::Internal(format!(
                "failed to start domain event outbox claim transaction: {error}"
            ))
        })?;
        let rows = self.claim_pending_for_relay_in_tx(&mut tx, limit).await?;
        // Hold locks only for the duration of claim; callers that need
        // claim+mark in one transaction should use `claim_pending_for_relay_in_tx`.
        tx.commit().await.map_err(|error| {
            DomainError::Internal(format!(
                "failed to commit domain event outbox claim transaction: {error}"
            ))
        })?;
        Ok(rows)
    }

    async fn count_unpublished(&self) -> Result<i64, DomainError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE published_at IS NULL")
            .fetch_one(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(format!("failed to count unpublished outbox events: {error}")))
    }

    async fn oldest_unpublished(&self) -> Result<Option<DateTime<Utc>>, DomainError> {
        sqlx::query_scalar(
            "SELECT occurred_at FROM domain_event_outbox WHERE published_at IS NULL ORDER BY occurred_at ASC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(format!("failed to query oldest unpublished outbox event: {error}")))
    }

    async fn delete_by_aggregate_and_type(
        &self,
        aggregate_id: &str,
        event_type: &str,
        older_than: DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        let result = sqlx::query(
            "DELETE FROM domain_event_outbox WHERE aggregate_id = $1 AND event_type = $2 AND occurred_at < $3",
        )
        .bind(aggregate_id)
        .bind(event_type)
        .bind(older_than)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(format!("failed to delete domain event outbox rows: {error}")))?;
        Ok(result.rows_affected())
    }

    async fn count_by_aggregates_and_type(
        &self,
        aggregate_ids: &[String],
        event_type: &str,
        older_than: DateTime<Utc>,
    ) -> Result<i64, DomainError> {
        if aggregate_ids.is_empty() {
            return Ok(0);
        }
        sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM domain_event_outbox WHERE aggregate_id = ANY($1) AND event_type = $2 AND occurred_at < $3",
        )
        .bind(aggregate_ids)
        .bind(event_type)
        .bind(older_than)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| {
            DomainError::Internal(format!("failed to count domain event outbox rows: {error}"))
        })
    }

    async fn delete_by_aggregates_and_type(
        &self,
        aggregate_ids: &[String],
        event_type: &str,
        older_than: DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        if aggregate_ids.is_empty() {
            return Ok(0);
        }
        let result = sqlx::query(
            "DELETE FROM domain_event_outbox WHERE aggregate_id = ANY($1) AND event_type = $2 AND occurred_at < $3",
        )
        .bind(aggregate_ids)
        .bind(event_type)
        .bind(older_than)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(format!("failed to delete domain event outbox rows: {error}")))?;
        Ok(result.rows_affected())
    }
}

fn decode_outbox_row(row: sqlx::postgres::PgRow) -> Result<DomainEventOutboxRow, DomainError> {
    Ok(DomainEventOutboxRow {
        event_id: row.try_get("event_id").map_err(sql_row_error)?,
        aggregate_type: row.try_get("aggregate_type").map_err(sql_row_error)?,
        aggregate_id: row.try_get("aggregate_id").map_err(sql_row_error)?,
        event_type: row.try_get("event_type").map_err(sql_row_error)?,
        payload: row
            .try_get::<sqlx::types::Json<serde_json::Value>, _>("payload")
            .map(|value| value.0)
            .map_err(sql_row_error)?,
        occurred_at: row.try_get("occurred_at").map_err(sql_row_error)?,
        publish_attempts: row.try_get("publish_attempts").map_err(sql_row_error)?,
        source_change_id: row.try_get("source_change_id").map_err(sql_row_error)?,
    })
}

fn sql_row_error(error: sqlx::Error) -> DomainError {
    DomainError::Internal(format!("failed to decode domain event outbox row: {error}"))
}

#[async_trait]
impl<'tx> DomainEventOutboxTransactionalRepository<Transaction<'tx, Postgres>> for PgDomainEventOutboxRepository {
    async fn insert_event_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        aggregate_type: &str,
        aggregate_id: &str,
        event_type: &str,
        payload: Value,
        source_change_id: &str,
    ) -> Result<String, DomainError> {
        Self::insert_event(&mut **tx, aggregate_type, aggregate_id, event_type, payload, source_change_id)
            .await
            .map_err(|e| DomainError::Internal(format!("failed to insert domain event outbox row: {e}")))
    }

    async fn mark_published_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        event_ids: &[String],
    ) -> Result<(), DomainError> {
        self.mark_published_batch(tx, event_ids)
            .await
            .map_err(|e| DomainError::Internal(format!("failed to mark relayed domain events as published: {e}")))
    }

    async fn mark_failed_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        row: &DomainEventOutboxRow,
        error: &str,
        backoff_seconds: i64,
    ) -> Result<(), DomainError> {
        self.mark_failed(tx, row, error, backoff_seconds).await.map_err(|e| {
            DomainError::Internal(format!(
                "failed to update failed domain event retry state for {}: {e}",
                row.event_id
            ))
        })
    }

    async fn claim_pending_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        limit: i64,
    ) -> Result<Vec<DomainEventOutboxRow>, DomainError> {
        self.claim_pending_for_relay_in_tx(tx, limit).await
    }
}
