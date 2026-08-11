use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

use fms_domain::error::DomainError;
use fms_domain::models::business_case_workflow::BusinessCaseWorkflowRun;
use fms_domain::ports::business_case_workflow_run_repository::BusinessCaseWorkflowRunRepository;

pub struct PgBusinessCaseWorkflowRunRepository {
    pool: PgPool,
}

impl PgBusinessCaseWorkflowRunRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BusinessCaseWorkflowRunRepository for PgBusinessCaseWorkflowRunRepository {
    async fn save(&self, run: &BusinessCaseWorkflowRun) -> Result<BusinessCaseWorkflowRun, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO business_case_workflow_runs (
                run_id, template_code, case_id, flight_id, process_definition_key,
                process_instance_id, waiting_task_id, receipt_group_id, status, outcome,
                recipient_snapshot, flight_context_snapshot, start_payload, started_by,
                completed_at, failed_reason, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12, $13, $14,
                $15, $16, $17, $18
            )
            ON CONFLICT (run_id) DO UPDATE SET
                waiting_task_id = EXCLUDED.waiting_task_id,
                receipt_group_id = EXCLUDED.receipt_group_id,
                status = EXCLUDED.status,
                outcome = EXCLUDED.outcome,
                recipient_snapshot = EXCLUDED.recipient_snapshot,
                flight_context_snapshot = EXCLUDED.flight_context_snapshot,
                start_payload = EXCLUDED.start_payload,
                started_by = EXCLUDED.started_by,
                completed_at = EXCLUDED.completed_at,
                failed_reason = EXCLUDED.failed_reason,
                updated_at = EXCLUDED.updated_at
            RETURNING *
            "#,
        )
        .bind(&run.run_id)
        .bind(&run.template_code)
        .bind(&run.case_id)
        .bind(&run.flight_id)
        .bind(&run.process_definition_key)
        .bind(&run.process_instance_id)
        .bind(&run.waiting_task_id)
        .bind(&run.receipt_group_id)
        .bind(&run.status)
        .bind(&run.outcome)
        .bind(serde_json::to_value(&run.recipient_snapshot).unwrap_or_else(|_| serde_json::json!([])))
        .bind(serde_json::to_value(&run.flight_context_snapshot).unwrap_or_else(|_| serde_json::json!({})))
        .bind(serde_json::to_value(&run.start_payload).unwrap_or_else(|_| serde_json::json!({})))
        .bind(&run.started_by)
        .bind(run.completed_at)
        .bind(&run.failed_reason)
        .bind(run.created_at)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row_to_run(&row))
    }

    async fn find_by_run_id(&self, run_id: &str) -> Result<Option<BusinessCaseWorkflowRun>, DomainError> {
        let row = sqlx::query("SELECT * FROM business_case_workflow_runs WHERE run_id = $1 LIMIT 1")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(row.map(|value| row_to_run(&value)))
    }

    async fn find_by_case_id(&self, case_id: &str) -> Result<Option<BusinessCaseWorkflowRun>, DomainError> {
        let row = sqlx::query("SELECT * FROM business_case_workflow_runs WHERE case_id = $1 LIMIT 1")
            .bind(case_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(row.map(|value| row_to_run(&value)))
    }

    async fn find_by_receipt_group_id(
        &self,
        receipt_group_id: &str,
    ) -> Result<Option<BusinessCaseWorkflowRun>, DomainError> {
        let row = sqlx::query("SELECT * FROM business_case_workflow_runs WHERE receipt_group_id = $1 LIMIT 1")
            .bind(receipt_group_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(row.map(|value| row_to_run(&value)))
    }

    async fn list_by_receipt_group_id(
        &self,
        receipt_group_id: &str,
    ) -> Result<Vec<BusinessCaseWorkflowRun>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM business_case_workflow_runs WHERE receipt_group_id = $1 ORDER BY created_at ASC, run_id ASC",
        )
        .bind(receipt_group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(rows.iter().map(row_to_run).collect())
    }
}

fn row_to_run(row: &sqlx::postgres::PgRow) -> BusinessCaseWorkflowRun {
    BusinessCaseWorkflowRun {
        run_id: row.get("run_id"),
        template_code: row.get("template_code"),
        case_id: row.get("case_id"),
        flight_id: row.get("flight_id"),
        process_definition_key: row.get("process_definition_key"),
        process_instance_id: row.get("process_instance_id"),
        waiting_task_id: row.get("waiting_task_id"),
        receipt_group_id: row.get("receipt_group_id"),
        status: row
            .get::<Option<String>, _>("status")
            .unwrap_or_else(|| "pending".to_string()),
        outcome: row.get("outcome"),
        recipient_snapshot: json_array_to_vec_map(row.get("recipient_snapshot")),
        flight_context_snapshot: json_object_to_map(row.get("flight_context_snapshot")),
        start_payload: json_object_to_map(row.get("start_payload")),
        started_by: row
            .get::<Option<String>, _>("started_by")
            .unwrap_or_else(|| "system".to_string()),
        completed_at: row.get("completed_at"),
        failed_reason: row.get("failed_reason"),
        created_at: row
            .get::<Option<chrono::DateTime<Utc>>, _>("created_at")
            .unwrap_or_else(Utc::now),
        updated_at: row
            .get::<Option<chrono::DateTime<Utc>>, _>("updated_at")
            .unwrap_or_else(Utc::now),
    }
}

fn json_array_to_vec_map(value: Option<serde_json::Value>) -> Vec<HashMap<String, serde_json::Value>> {
    match value {
        Some(serde_json::Value::Array(items)) => items
            .into_iter()
            .filter_map(|item| match item {
                serde_json::Value::Object(map) => Some(map.into_iter().collect()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn json_object_to_map(value: Option<serde_json::Value>) -> HashMap<String, serde_json::Value> {
    match value {
        Some(serde_json::Value::Object(map)) => map.into_iter().collect(),
        _ => HashMap::new(),
    }
}
