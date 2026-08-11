use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fms_domain::error::DomainError;
use fms_domain::ports::flight_timeline_event_repository::{
    FlightTimelineEvent, FlightTimelineEventRepository, FlightTimelineEventTransactionalRepository,
    FlightTimelineWriteResult,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Debug, Clone)]
pub struct PgFlightTimelineEventRepository {
    pool: PgPool,
}

impl PgFlightTimelineEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_event(row: &sqlx::postgres::PgRow) -> FlightTimelineEvent {
    FlightTimelineEvent {
        timeline_id: row.try_get("timeline_id").unwrap_or_default(),
        flight_id: row.try_get("flight_id").unwrap_or_default(),
        milestone_code: row.try_get("milestone_code").unwrap_or_default(),
        occurred_at: row.try_get("occurred_at").unwrap_or_else(|_| Utc::now()),
        leg_type: row.try_get("leg_type").ok().flatten(),
        recorded_by: row.try_get("recorded_by").ok().flatten(),
        client_action_id: row.try_get("client_action_id").ok().flatten(),
        source: row.try_get("source").unwrap_or_else(|_| "manual".to_string()),
        payload: row.try_get("payload").unwrap_or_else(|_| serde_json::json!({})),
        created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
    }
}

#[async_trait]
impl FlightTimelineEventRepository for PgFlightTimelineEventRepository {
    async fn list_by_flight(&self, flight_id: &str, limit: i64) -> Result<Vec<FlightTimelineEvent>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT timeline_id, flight_id, milestone_code, occurred_at, leg_type, recorded_by, client_action_id, source, payload, created_at
            FROM flight_dispatch_timeline_events
            WHERE flight_id = $1
            ORDER BY occurred_at DESC, created_at DESC
            LIMIT $2
            "#,
        )
        .bind(flight_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(rows.iter().map(row_to_event).collect())
    }

    async fn latest_snapshots(
        &self,
        flight_ids: &[String],
    ) -> Result<HashMap<String, HashMap<String, DateTime<Utc>>>, DomainError> {
        if flight_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Last-write-wins: the current cell value is the most recently recorded
        // event for the milestone, not the maximum business occurred_at. This
        // allows correcting a wrong time to an earlier value.
        let query = r#"
            SELECT DISTINCT ON (flight_id, milestone_code) flight_id, milestone_code, occurred_at
            FROM flight_dispatch_timeline_events
            WHERE flight_id = ANY($1)
            ORDER BY flight_id, milestone_code, created_at DESC, timeline_id DESC
        "#;

        let rows = sqlx::query(query)
            .bind(flight_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        let mut map: HashMap<String, HashMap<String, DateTime<Utc>>> = HashMap::new();
        for row in rows {
            let flight_id: String = row.get("flight_id");
            let code: String = row.get("milestone_code");
            let occurred_at: DateTime<Utc> = row.get("occurred_at");
            map.entry(flight_id).or_default().insert(code, occurred_at);
        }
        Ok(map)
    }
}

#[async_trait]
impl<'tx> FlightTimelineEventTransactionalRepository<Transaction<'tx, Postgres>> for PgFlightTimelineEventRepository {
    async fn insert_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        event: &FlightTimelineEvent,
        client_action_id: Option<&str>,
    ) -> Result<FlightTimelineWriteResult, DomainError> {
        let row = sqlx::query(
            r#"
            WITH inserted AS (
                INSERT INTO flight_dispatch_timeline_events (
                    timeline_id, flight_id, milestone_code, occurred_at, leg_type, recorded_by,
                    client_action_id, source, payload, created_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (flight_id, client_action_id)
                WHERE client_action_id IS NOT NULL
                DO NOTHING
                RETURNING timeline_id, flight_id, milestone_code, occurred_at, leg_type, recorded_by,
                          client_action_id, source, payload, created_at, TRUE AS inserted
            )
            SELECT timeline_id, flight_id, milestone_code, occurred_at, leg_type, recorded_by,
                   client_action_id, source, payload, created_at, inserted
            FROM inserted
            UNION ALL
            SELECT timeline_id, flight_id, milestone_code, occurred_at, leg_type, recorded_by,
                   client_action_id, source, payload, created_at, FALSE AS inserted
            FROM flight_dispatch_timeline_events
            WHERE $7::text IS NOT NULL
              AND flight_id = $2
              AND client_action_id = $7
            LIMIT 1
            "#,
        )
        .bind(&event.timeline_id)
        .bind(&event.flight_id)
        .bind(&event.milestone_code)
        .bind(event.occurred_at)
        .bind(&event.leg_type)
        .bind(&event.recorded_by)
        .bind(client_action_id)
        .bind(&event.source)
        .bind(&event.payload)
        .bind(event.created_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(FlightTimelineWriteResult {
            event: row_to_event(&row),
            inserted: row.try_get("inserted").unwrap_or(true),
        })
    }

    async fn delete_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        flight_id: &str,
        timeline_id: &str,
    ) -> Result<bool, DomainError> {
        sqlx::query("DELETE FROM flight_dispatch_timeline_events WHERE flight_id = $1 AND timeline_id = $2")
            .bind(flight_id)
            .bind(timeline_id)
            .execute(&mut **tx)
            .await
            .map(|result| result.rows_affected() > 0)
            .map_err(|error| DomainError::Internal(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_type_exists() {
        let _ = std::any::type_name::<PgFlightTimelineEventRepository>();
    }
}
