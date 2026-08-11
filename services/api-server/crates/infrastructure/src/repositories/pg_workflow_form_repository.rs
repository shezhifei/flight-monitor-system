use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::workflow_form::{
    WorkflowFormAssignmentMode, WorkflowFormBinding, WorkflowFormBindingSource, WorkflowFormSubmission,
    WorkflowFormSubmissionStatus, WorkflowFormTemplate, WorkflowFormTemplateStatus, WorkflowFormWriteBackMode,
};
use fms_domain::ports::workflow_form_repository::WorkflowFormRepository;

pub struct PgWorkflowFormRepository {
    pool: PgPool,
}

impl PgWorkflowFormRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WorkflowFormRepository for PgWorkflowFormRepository {
    async fn save_template(&self, template: &WorkflowFormTemplate) -> Result<WorkflowFormTemplate, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO workflow_form_templates (
                id, form_code, name, version, schema_json, ui_schema_json,
                status, description, created_by, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11
            )
            ON CONFLICT (form_code, version) DO UPDATE SET
                name = EXCLUDED.name,
                schema_json = EXCLUDED.schema_json,
                ui_schema_json = EXCLUDED.ui_schema_json,
                status = EXCLUDED.status,
                description = EXCLUDED.description,
                updated_at = EXCLUDED.updated_at
            RETURNING *
            "#,
        )
        .bind(&template.id)
        .bind(&template.form_code)
        .bind(&template.name)
        .bind(template.version)
        .bind(&template.schema_json)
        .bind(&template.ui_schema_json)
        .bind(template.status.as_str())
        .bind(&template.description)
        .bind(&template.created_by)
        .bind(template.created_at)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row_to_template(&row)
    }

    async fn find_template_by_code_version(
        &self,
        form_code: &str,
        version: i32,
    ) -> Result<Option<WorkflowFormTemplate>, DomainError> {
        let row = sqlx::query("SELECT * FROM workflow_form_templates WHERE form_code = $1 AND version = $2 LIMIT 1")
            .bind(form_code)
            .bind(version)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_template).transpose()
    }

    async fn find_active_template_by_code(&self, form_code: &str) -> Result<Option<WorkflowFormTemplate>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT *
            FROM workflow_form_templates
            WHERE form_code = $1 AND status = 'ACTIVE'
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(form_code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_template).transpose()
    }

    async fn save_binding(&self, binding: &WorkflowFormBinding) -> Result<WorkflowFormBinding, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO workflow_form_bindings (
                id, template_code, process_definition_key, task_definition_key, form_code,
                form_version, target_department_id, target_department_name, target_roles,
                assignment_mode, write_back_mode, write_back_key, flowable_variable_prefix,
                complete_task_on_submit, allow_resubmit, source, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12, $13,
                $14, $15, $16, $17, $18
            )
            ON CONFLICT (template_code, task_definition_key, form_code) DO UPDATE SET
                process_definition_key = EXCLUDED.process_definition_key,
                form_version = EXCLUDED.form_version,
                target_department_id = EXCLUDED.target_department_id,
                target_department_name = EXCLUDED.target_department_name,
                target_roles = EXCLUDED.target_roles,
                assignment_mode = EXCLUDED.assignment_mode,
                write_back_mode = EXCLUDED.write_back_mode,
                write_back_key = EXCLUDED.write_back_key,
                flowable_variable_prefix = EXCLUDED.flowable_variable_prefix,
                complete_task_on_submit = EXCLUDED.complete_task_on_submit,
                allow_resubmit = EXCLUDED.allow_resubmit,
                source = EXCLUDED.source,
                updated_at = EXCLUDED.updated_at
            RETURNING *
            "#,
        )
        .bind(&binding.id)
        .bind(&binding.template_code)
        .bind(&binding.process_definition_key)
        .bind(&binding.task_definition_key)
        .bind(&binding.form_code)
        .bind(binding.form_version)
        .bind(&binding.target_department_id)
        .bind(&binding.target_department_name)
        .bind(serde_json::to_value(&binding.target_roles).unwrap_or_else(|_| serde_json::json!([])))
        .bind(binding.assignment_mode.as_str())
        .bind(binding.write_back_mode.as_str())
        .bind(&binding.write_back_key)
        .bind(&binding.flowable_variable_prefix)
        .bind(binding.complete_task_on_submit)
        .bind(binding.allow_resubmit)
        .bind(binding.source.as_str())
        .bind(binding.created_at)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row_to_binding(&row)
    }

    async fn find_bindings_by_process_task(
        &self,
        process_definition_key: &str,
        task_definition_key: &str,
    ) -> Result<Vec<WorkflowFormBinding>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM workflow_form_bindings
            WHERE process_definition_key = $1 AND task_definition_key = $2
            ORDER BY created_at ASC
            "#,
        )
        .bind(process_definition_key)
        .bind(task_definition_key)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        rows.iter().map(row_to_binding).collect()
    }

    async fn find_bindings_by_template_code(
        &self,
        template_code: &str,
    ) -> Result<Vec<WorkflowFormBinding>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM workflow_form_bindings
            WHERE template_code = $1
            ORDER BY task_definition_key ASC, created_at ASC
            "#,
        )
        .bind(template_code)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        rows.iter().map(row_to_binding).collect()
    }

    async fn insert_submission(
        &self,
        submission: &WorkflowFormSubmission,
    ) -> Result<WorkflowFormSubmission, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO workflow_form_submissions (
                id, case_id, run_id, process_instance_id, task_id, task_definition_key,
                form_code, form_version, data_json, normalized_summary_json,
                submitted_by, submitted_operator_name, submitted_department_id,
                submitted_department_name, submitted_at, status
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10,
                $11, $12, $13,
                $14, $15, $16
            )
            RETURNING *
            "#,
        )
        .bind(&submission.id)
        .bind(&submission.case_id)
        .bind(&submission.run_id)
        .bind(&submission.process_instance_id)
        .bind(&submission.task_id)
        .bind(&submission.task_definition_key)
        .bind(&submission.form_code)
        .bind(submission.form_version)
        .bind(&submission.data_json)
        .bind(&submission.normalized_summary_json)
        .bind(&submission.submitted_by)
        .bind(&submission.submitted_operator_name)
        .bind(&submission.submitted_department_id)
        .bind(&submission.submitted_department_name)
        .bind(submission.submitted_at)
        .bind(submission.status.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row_to_submission(&row)
    }

    async fn find_submissions_by_case(&self, case_id: &str) -> Result<Vec<WorkflowFormSubmission>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM workflow_form_submissions
            WHERE case_id = $1
            ORDER BY submitted_at DESC
            "#,
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        rows.iter().map(row_to_submission).collect()
    }

    async fn find_latest_submission(
        &self,
        case_id: &str,
        task_definition_key: &str,
        form_code: &str,
    ) -> Result<Option<WorkflowFormSubmission>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT *
            FROM workflow_form_submissions
            WHERE case_id = $1
              AND task_definition_key = $2
              AND form_code = $3
              AND status = 'SUBMITTED'
            ORDER BY submitted_at DESC
            LIMIT 1
            "#,
        )
        .bind(case_id)
        .bind(task_definition_key)
        .bind(form_code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_submission).transpose()
    }
}

fn row_to_template(row: &sqlx::postgres::PgRow) -> Result<WorkflowFormTemplate, DomainError> {
    let status_raw = row.get::<String, _>("status");
    let status = WorkflowFormTemplateStatus::from_db(&status_raw)
        .ok_or_else(|| DomainError::Internal(format!("invalid workflow form status: {status_raw}")))?;

    Ok(WorkflowFormTemplate {
        id: row.get("id"),
        form_code: row.get("form_code"),
        name: row.get("name"),
        version: row.get("version"),
        schema_json: row
            .get::<Option<serde_json::Value>, _>("schema_json")
            .unwrap_or_else(|| serde_json::json!({})),
        ui_schema_json: row
            .get::<Option<serde_json::Value>, _>("ui_schema_json")
            .unwrap_or_else(|| serde_json::json!({})),
        status,
        description: row.get("description"),
        created_by: row.get("created_by"),
        created_at: row
            .get::<Option<chrono::DateTime<Utc>>, _>("created_at")
            .unwrap_or_else(Utc::now),
        updated_at: row
            .get::<Option<chrono::DateTime<Utc>>, _>("updated_at")
            .unwrap_or_else(Utc::now),
    })
}

fn row_to_binding(row: &sqlx::postgres::PgRow) -> Result<WorkflowFormBinding, DomainError> {
    let assignment_mode_raw = row.get::<String, _>("assignment_mode");
    let assignment_mode = WorkflowFormAssignmentMode::from_db(&assignment_mode_raw).ok_or_else(|| {
        DomainError::Internal(format!("invalid workflow form assignment mode: {assignment_mode_raw}"))
    })?;
    let write_back_mode_raw = row.get::<String, _>("write_back_mode");
    let write_back_mode = WorkflowFormWriteBackMode::from_db(&write_back_mode_raw).ok_or_else(|| {
        DomainError::Internal(format!("invalid workflow form write back mode: {write_back_mode_raw}"))
    })?;
    let source_raw = row.get::<String, _>("source");
    let source = WorkflowFormBindingSource::from_db(&source_raw)
        .ok_or_else(|| DomainError::Internal(format!("invalid workflow form binding source: {source_raw}")))?;

    Ok(WorkflowFormBinding {
        id: row.get("id"),
        template_code: row.get("template_code"),
        process_definition_key: row.get("process_definition_key"),
        task_definition_key: row.get("task_definition_key"),
        form_code: row.get("form_code"),
        form_version: row.get("form_version"),
        target_department_id: row.get("target_department_id"),
        target_department_name: row.get("target_department_name"),
        target_roles: row
            .get::<Option<serde_json::Value>, _>("target_roles")
            .map(json_array_to_vec_string)
            .unwrap_or_default(),
        assignment_mode,
        write_back_mode,
        write_back_key: row.get("write_back_key"),
        flowable_variable_prefix: row.get("flowable_variable_prefix"),
        complete_task_on_submit: row.get("complete_task_on_submit"),
        allow_resubmit: row.get("allow_resubmit"),
        source,
        created_at: row
            .get::<Option<chrono::DateTime<Utc>>, _>("created_at")
            .unwrap_or_else(Utc::now),
        updated_at: row
            .get::<Option<chrono::DateTime<Utc>>, _>("updated_at")
            .unwrap_or_else(Utc::now),
    })
}

fn row_to_submission(row: &sqlx::postgres::PgRow) -> Result<WorkflowFormSubmission, DomainError> {
    let status_raw = row.get::<String, _>("status");
    let status = WorkflowFormSubmissionStatus::from_db(&status_raw)
        .ok_or_else(|| DomainError::Internal(format!("invalid workflow form submission status: {status_raw}")))?;

    Ok(WorkflowFormSubmission {
        id: row.get("id"),
        case_id: row.get("case_id"),
        run_id: row.get("run_id"),
        process_instance_id: row.get("process_instance_id"),
        task_id: row.get("task_id"),
        task_definition_key: row.get("task_definition_key"),
        form_code: row.get("form_code"),
        form_version: row.get("form_version"),
        data_json: row
            .get::<Option<serde_json::Value>, _>("data_json")
            .unwrap_or_else(|| serde_json::json!({})),
        normalized_summary_json: row
            .get::<Option<serde_json::Value>, _>("normalized_summary_json")
            .unwrap_or_else(|| serde_json::json!({})),
        submitted_by: row.get("submitted_by"),
        submitted_operator_name: row.get("submitted_operator_name"),
        submitted_department_id: row.get("submitted_department_id"),
        submitted_department_name: row.get("submitted_department_name"),
        submitted_at: row
            .get::<Option<chrono::DateTime<Utc>>, _>("submitted_at")
            .unwrap_or_else(Utc::now),
        status,
    })
}

fn json_array_to_vec_string(value: serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}
