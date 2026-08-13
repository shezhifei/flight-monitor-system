//! PostgreSQL adapter for snapshot-backed AI context objects.

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::ports::ai_context_snapshot_repository::{AiContextSnapshotKind, AiContextSnapshotRepository};
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct PgAiContextSnapshotRepository {
    pool: PgPool,
}

impl PgAiContextSnapshotRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AiContextSnapshotRepository for PgAiContextSnapshotRepository {
    async fn load_snapshot(
        &self,
        kind: AiContextSnapshotKind,
        object_id: &str,
    ) -> Result<Option<serde_json::Value>, DomainError> {
        let row = sqlx::query(snapshot_query(kind))
            .bind(object_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.map(|row| row.try_get::<serde_json::Value, _>("data"))
            .transpose()
            .map_err(|error| DomainError::Internal(error.to_string()))
    }
}

fn snapshot_query(kind: AiContextSnapshotKind) -> &'static str {
    match kind {
        AiContextSnapshotKind::FlightLeg => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM flight_legs WHERE leg_id = $1 OR flight_id = $1 OR CONCAT(flight_id, ':', leg_type) = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Stand => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM stands WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Team => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM teams WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Equipment => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM equipment WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::WorkflowRun => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM business_case_workflow_runs WHERE run_id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Notification => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM notifications WHERE notification_id = $1 LIMIT 1) snapshot"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::snapshot_query;
    use fms_domain::ports::ai_context_snapshot_repository::AiContextSnapshotKind;

    #[test]
    fn snapshot_kinds_map_to_fixed_queries() {
        let cases = [
            (AiContextSnapshotKind::FlightLeg, "FROM flight_legs"),
            (AiContextSnapshotKind::Stand, "FROM stands"),
            (AiContextSnapshotKind::Team, "FROM teams"),
            (AiContextSnapshotKind::Equipment, "FROM equipment"),
            (AiContextSnapshotKind::WorkflowRun, "FROM business_case_workflow_runs"),
            (AiContextSnapshotKind::Notification, "FROM notifications"),
        ];

        for (kind, expected_table) in cases {
            let query = snapshot_query(kind);
            assert!(query.contains(expected_table));
            assert!(query.contains("$1"));
        }
    }
}
