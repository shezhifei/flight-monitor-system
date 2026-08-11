use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::ports::domain_event_subscription_state_repository::{
    DomainEventDeadLetterRecord, DomainEventProcessingRecord, DomainEventSubscriptionStateRepository,
};
use sqlx::{types::Json, PgPool};

const MAX_ERROR_LENGTH: usize = 1000;

#[derive(Debug, Clone)]
pub struct PgDomainEventSubscriptionStateRepository {
    pool: PgPool,
}

impl PgDomainEventSubscriptionStateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn truncate_error(error: &str) -> String {
    error.chars().take(MAX_ERROR_LENGTH).collect()
}

#[async_trait]
impl DomainEventSubscriptionStateRepository for PgDomainEventSubscriptionStateRepository {
    async fn is_processed(&self, event_id: &str) -> Result<bool, DomainError> {
        let row = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1
            FROM domain_event_processed
            WHERE event_id = $1
              AND success = TRUE
            LIMIT 1
            "#,
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            DomainError::Internal(format!(
                "failed to query domain_event_processed for {event_id}: {error}"
            ))
        })?;

        Ok(row.is_some())
    }

    async fn mark_processed(&self, record: &DomainEventProcessingRecord) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO domain_event_processed (
                event_id,
                source_change_id,
                event_type,
                aggregate_type,
                aggregate_id,
                success,
                retry_count,
                last_error,
                last_attempt_at,
                processed_at
            )
            VALUES ($1, $2, $3, $4, $5, TRUE, 0, NULL, NOW(), NOW())
            ON CONFLICT (event_id) DO UPDATE
            SET source_change_id = EXCLUDED.source_change_id,
                event_type = EXCLUDED.event_type,
                aggregate_type = EXCLUDED.aggregate_type,
                aggregate_id = EXCLUDED.aggregate_id,
                success = TRUE,
                last_error = NULL,
                last_attempt_at = NOW(),
                processed_at = NOW()
            "#,
        )
        .bind(&record.event_id)
        .bind(record.source_change_id.as_deref())
        .bind(&record.event_type)
        .bind(&record.aggregate_type)
        .bind(&record.aggregate_id)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            DomainError::Internal(format!(
                "failed to mark domain event {} as processed: {error}",
                record.event_id
            ))
        })?;

        Ok(())
    }

    async fn mark_failed(&self, record: &DomainEventProcessingRecord, error_message: &str) -> Result<i32, DomainError> {
        sqlx::query_scalar::<_, i32>(
            r#"
            INSERT INTO domain_event_processed (
                event_id,
                source_change_id,
                event_type,
                aggregate_type,
                aggregate_id,
                success,
                retry_count,
                last_error,
                last_attempt_at,
                processed_at
            )
            VALUES ($1, $2, $3, $4, $5, FALSE, 1, $6, NOW(), NULL)
            ON CONFLICT (event_id) DO UPDATE
            SET source_change_id = EXCLUDED.source_change_id,
                event_type = EXCLUDED.event_type,
                aggregate_type = EXCLUDED.aggregate_type,
                aggregate_id = EXCLUDED.aggregate_id,
                success = FALSE,
                retry_count = domain_event_processed.retry_count + 1,
                last_error = EXCLUDED.last_error,
                last_attempt_at = NOW()
            RETURNING retry_count
            "#,
        )
        .bind(&record.event_id)
        .bind(record.source_change_id.as_deref())
        .bind(&record.event_type)
        .bind(&record.aggregate_type)
        .bind(&record.aggregate_id)
        .bind(truncate_error(error_message))
        .fetch_one(&self.pool)
        .await
        .map_err(|error| {
            DomainError::Internal(format!(
                "failed to mark domain event {} as failed: {error}",
                record.event_id
            ))
        })
    }

    async fn insert_dead_letter(&self, record: &DomainEventDeadLetterRecord) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO domain_event_dead_letters (
                event_id,
                source_change_id,
                aggregate_type,
                aggregate_id,
                event_type,
                payload,
                stream_message_id,
                retry_count,
                error_message,
                dead_lettered_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
            ON CONFLICT (event_id) DO UPDATE
            SET source_change_id = EXCLUDED.source_change_id,
                aggregate_type = EXCLUDED.aggregate_type,
                aggregate_id = EXCLUDED.aggregate_id,
                event_type = EXCLUDED.event_type,
                payload = EXCLUDED.payload,
                stream_message_id = EXCLUDED.stream_message_id,
                retry_count = EXCLUDED.retry_count,
                error_message = EXCLUDED.error_message,
                dead_lettered_at = NOW()
            "#,
        )
        .bind(&record.event_id)
        .bind(record.source_change_id.as_deref())
        .bind(&record.aggregate_type)
        .bind(&record.aggregate_id)
        .bind(&record.event_type)
        .bind(Json(record.payload.clone()))
        .bind(&record.stream_message_id)
        .bind(record.retry_count.max(1))
        .bind(truncate_error(&record.error_message))
        .execute(&self.pool)
        .await
        .map_err(|error| {
            DomainError::Internal(format!(
                "failed to dead-letter domain event {}: {error}",
                record.event_id
            ))
        })?;

        Ok(())
    }

    async fn upsert_consumer_offset(
        &self,
        consumer_group: &str,
        consumer_name: &str,
        topic: &str,
        message_id: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO domain_event_consumer_offsets (
                consumer_group,
                consumer_name,
                topic,
                last_message_id,
                updated_at
            )
            VALUES ($1, $2, $3, $4, NOW())
            ON CONFLICT (consumer_group, consumer_name, topic) DO UPDATE
            SET last_message_id = EXCLUDED.last_message_id,
                updated_at = NOW()
            "#,
        )
        .bind(consumer_group)
        .bind(consumer_name)
        .bind(topic)
        .bind(message_id)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            DomainError::Internal(format!(
                "failed to upsert domain event consumer offset for {message_id}: {error}"
            ))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_constructs() {
        let _ = std::any::type_name::<PgDomainEventSubscriptionStateRepository>();
    }
}
