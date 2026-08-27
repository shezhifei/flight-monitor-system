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
        AiContextSnapshotKind::Stand => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM stands WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Team => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM teams WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Equipment => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM equipment WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Terminal => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM terminals WHERE terminal_id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Gate => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM gates WHERE gate_id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::BaggageCarousel => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM baggage_carousels WHERE carousel_id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::StandOccupation => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM stand_occupations WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::GateAssignment => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM gate_assignments WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::CarouselAssignment => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM carousel_assignments WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Department => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM departments WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::EquipmentType => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM equipment_types WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Aircraft => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM aircraft WHERE registration = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::TurnaroundLink => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM turnaround_links WHERE id = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Qualification => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM department_qualification_catalog WHERE id = $1 OR qualification_code = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::TaskType => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM task_types WHERE id = $1 OR code = $1 LIMIT 1) snapshot"
        }
        AiContextSnapshotKind::Personnel => {
            "SELECT to_jsonb(snapshot) AS data FROM (SELECT u.id, u.username, u.display_name, u.department, u.job_title, u.is_active, u.account_type, pr.current_status AS runtime_status, pr.current_stand_id, pr.current_position_lat, pr.current_position_lng FROM users u LEFT JOIN personnel_runtime pr ON pr.user_id = u.id WHERE u.id = $1 LIMIT 1) snapshot"
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
            (AiContextSnapshotKind::Stand, "FROM stands"),
            (AiContextSnapshotKind::Team, "FROM teams"),
            (AiContextSnapshotKind::Equipment, "FROM equipment"),
            (AiContextSnapshotKind::Terminal, "FROM terminals"),
            (AiContextSnapshotKind::Gate, "FROM gates"),
            (AiContextSnapshotKind::BaggageCarousel, "FROM baggage_carousels"),
            (AiContextSnapshotKind::StandOccupation, "FROM stand_occupations"),
            (AiContextSnapshotKind::GateAssignment, "FROM gate_assignments"),
            (AiContextSnapshotKind::CarouselAssignment, "FROM carousel_assignments"),
            (AiContextSnapshotKind::Department, "FROM departments"),
            (AiContextSnapshotKind::EquipmentType, "FROM equipment_types"),
            (AiContextSnapshotKind::Aircraft, "FROM aircraft"),
            (AiContextSnapshotKind::TurnaroundLink, "FROM turnaround_links"),
            (AiContextSnapshotKind::Qualification, "FROM department_qualification_catalog"),
            (AiContextSnapshotKind::TaskType, "FROM task_types"),
            (AiContextSnapshotKind::Personnel, "FROM users"),
        ];

        for (kind, expected_table) in cases {
            let query = snapshot_query(kind);
            assert!(query.contains(expected_table));
            assert!(query.contains("$1"));
        }
    }
}
