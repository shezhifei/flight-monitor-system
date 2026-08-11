use async_trait::async_trait;
use std::collections::HashMap;

use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::ports::workflow_dispatch_repository::WorkflowDispatchRepository;

pub struct PgWorkflowDispatchRepository {
    pool: PgPool,
}

impl PgWorkflowDispatchRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WorkflowDispatchRepository for PgWorkflowDispatchRepository {
    async fn replace_assignment_members(
        &self,
        dispatch_order_id: &str,
        assigned_user_ids: &[String],
    ) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        sqlx::query(
            r#"
                UPDATE dispatch_order_members
                SET
                    is_active = FALSE,
                    check_out_time = COALESCE(check_out_time, CURRENT_TIMESTAMP)
                WHERE dispatch_order_id = $1
                  AND is_active = TRUE
            "#,
        )
        .bind(dispatch_order_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        if !assigned_user_ids.is_empty() {
            let assigned_at = Utc::now();
            let mut builder = QueryBuilder::<Postgres>::new(
                r#"
                    INSERT INTO dispatch_order_members (
                        id,
                        dispatch_order_id,
                        user_id,
                        role,
                        source_type,
                        source_team_id,
                        assigned_at,
                        is_active
                    )
                "#,
            );

            builder.push_values(assigned_user_ids.iter(), |mut row, user_id| {
                row.push_bind(ulid::Ulid::new().to_string())
                    .push_bind(dispatch_order_id)
                    .push_bind(user_id)
                    .push_bind("member")
                    .push_bind("individual")
                    .push_bind(Option::<String>::None)
                    .push_bind(assigned_at)
                    .push_bind(true);
            });

            builder.push(
                r#"
                    ON CONFLICT (dispatch_order_id, user_id) DO UPDATE SET
                        role = EXCLUDED.role,
                        source_type = EXCLUDED.source_type,
                        source_team_id = EXCLUDED.source_team_id,
                        assigned_at = EXCLUDED.assigned_at,
                        check_in_time = NULL,
                        check_out_time = NULL,
                        is_active = TRUE
                "#,
            );

            builder
                .build()
                .execute(&mut *tx)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn get_active_workload_by_users(&self, user_ids: &[String]) -> Result<HashMap<String, i64>, DomainError> {
        let normalized = user_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if normalized.is_empty() {
            return Ok(HashMap::new());
        }

        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
                SELECT workload.user_id, COUNT(*)::BIGINT AS active_count
                FROM (
                    SELECT d.individual_user_id AS user_id, d.id AS dispatch_order_id
                    FROM dispatch_orders d
                    WHERE d.individual_user_id IS NOT NULL
                      AND d.status IN ('pending', 'assigned', 'in_progress')
                      AND d.individual_user_id IN (
            "#,
        );
        let mut separated = builder.separated(", ");
        for user_id in &normalized {
            separated.push_bind(user_id);
        }
        separated.push_unseparated(")");
        builder.push(
            r#"
                    UNION ALL
                    SELECT dom.user_id AS user_id, dom.dispatch_order_id
                    FROM dispatch_order_members dom
                    JOIN dispatch_orders d ON d.id = dom.dispatch_order_id
                    WHERE dom.is_active = TRUE
                      AND d.status IN ('pending', 'assigned', 'in_progress')
                      AND dom.user_id IN (
            "#,
        );
        let mut separated = builder.separated(", ");
        for user_id in &normalized {
            separated.push_bind(user_id);
        }
        separated.push_unseparated(")");
        builder.push(
            r#"
                ) workload
                GROUP BY workload.user_id
            "#,
        );

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        let mut result = HashMap::new();
        for row in rows {
            let user_id: String = row
                .try_get("user_id")
                .map_err(|error| DomainError::Internal(error.to_string()))?;
            let active_count: i64 = row
                .try_get("active_count")
                .map_err(|error| DomainError::Internal(error.to_string()))?;
            result.insert(user_id, active_count);
        }
        Ok(result)
    }
}
