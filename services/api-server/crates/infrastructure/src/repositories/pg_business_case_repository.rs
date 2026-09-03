//! PostgreSQL 业务事项仓储实现。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::business_case::{
    BusinessCaseAppendEntry, BusinessCaseTerminalMetadata, BusinessCaseWorkflowReceiptItem,
    BusinessCaseWorkflowReceiptProjection, BusinessCaseWorkflowReceiptSummary, FlightBusinessCase, VisibilityScope,
};
use fms_domain::ports::business_case_repository::{BusinessCaseRepository, BusinessCaseTransactionalRepository};

use super::soft_delete_audit::record_soft_delete;

pub struct PgBusinessCaseRepository {
    pool: PgPool,
}

impl PgBusinessCaseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BusinessCaseRepository for PgBusinessCaseRepository {
    async fn save(&self, case: &FlightBusinessCase) -> Result<(), DomainError> {
        let context_json = build_context_json(case);
        let log_entries = business_case_log_to_text_array(&case.log);
        sqlx::query(
            r#"
            INSERT INTO flight_business_cases (
                case_id, flight_id, case_type, description, context, created_by, updated_by,
                created_at, status, stand, gate, visibility_scope, department_id,
                department_name_snapshot, finished_at, cancelled_at, log
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
            ON CONFLICT (case_id) DO UPDATE SET
                case_type = EXCLUDED.case_type,
                description = EXCLUDED.description,
                context = EXCLUDED.context,
                updated_by = EXCLUDED.updated_by,
                status = EXCLUDED.status,
                stand = EXCLUDED.stand,
                gate = EXCLUDED.gate,
                visibility_scope = EXCLUDED.visibility_scope,
                department_id = EXCLUDED.department_id,
                department_name_snapshot = EXCLUDED.department_name_snapshot,
                finished_at = EXCLUDED.finished_at,
                cancelled_at = EXCLUDED.cancelled_at,
                log = EXCLUDED.log,
                deleted_at = NULL
            "#,
        )
        .bind(&case.case_id)
        .bind(&case.flight_id)
        .bind(&case.case_type)
        .bind(&case.description)
        .bind(&context_json)
        .bind(&case.created_by)
        .bind(&case.updated_by)
        .bind(case.created_at)
        .bind(&case.status)
        .bind(&case.stand)
        .bind(&case.gate)
        .bind(case.visibility_scope.as_str())
        .bind(&case.department_id)
        .bind(&case.department_name_snapshot)
        .bind(case.finished_at)
        .bind(case.cancelled_at)
        .bind(&log_entries)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, case_id: &str) -> Result<Option<FlightBusinessCase>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                c.*,
                COALESCE(f.flight_number, '') AS flight_no,
                wr.receipt_group_id,
                wr_sg.title AS receipt_title,
                wr_sg.severity AS receipt_severity,
                wr_sg.origin_type AS receipt_origin_type,
                wr_sg.created_at AS receipt_created_at,
                wr_sg.total_count AS receipt_total_count,
                wr_sg.pending_count AS receipt_pending_count,
                wr_sg.acknowledged_count AS receipt_acknowledged_count,
                wr_sg.rejected_count AS receipt_rejected_count,
                wr_sg.latest_updated_at AS receipt_latest_updated_at,
                wr_sg.remind_after_at AS receipt_remind_after_at,
                wr_items.items_json AS receipt_items
            FROM flight_business_cases c
            LEFT JOIN flights f ON f.flight_id = c.flight_id
            LEFT JOIN business_case_workflow_runs wr ON wr.case_id = c.case_id
            LEFT JOIN LATERAL (
                SELECT
                    MIN(n.title) AS title,
                    MIN(n.severity) AS severity,
                    MIN(n.origin_type) AS origin_type,
                    MIN(n.created_at) AS created_at,
                    COUNT(*) AS total_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'pending') AS pending_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'acknowledged') AS acknowledged_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'rejected') AS rejected_count,
                    MAX(COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)) AS latest_updated_at,
                    MIN(n.created_at) + INTERVAL '2 minutes' AS remind_after_at
                FROM notifications n
                WHERE n.receipt_group_id = wr.receipt_group_id
                GROUP BY n.receipt_group_id
            ) wr_sg ON true
            LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'user_id', n.user_id,
                        'recipient_user_id', n.user_id,
                        'recipient_username', COALESCE(
                            NULLIF(trim(n.recipient_username_snapshot), ''),
                            NULLIF(trim(n.recipient_display_name_snapshot), ''),
                            NULLIF(trim(ru.username), ''),
                            '未知账号'
                        ),
                        'recipient_display_name', COALESCE(NULLIF(trim(n.recipient_display_name_snapshot), ''), ru.display_name),
                        'recipient_department', COALESCE(NULLIF(trim(n.recipient_department_snapshot), ''), ru.department),
                        'recipient_job_title', COALESCE(NULLIF(trim(n.recipient_job_title_snapshot), ''), ru.job_title),
                        'ack_status', n.ack_status,
                        'ack_at', n.ack_at,
                        'ack_note', n.ack_note,
                        'updated_at', COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)
                    )
                    ORDER BY n.created_at ASC, n.notification_id ASC
                ) AS items_json
                FROM notifications n
                LEFT JOIN users ru ON ru.id = n.user_id
                WHERE n.receipt_group_id = wr.receipt_group_id
            ) wr_items ON true
            WHERE c.case_id = $1 AND c.deleted_at IS NULL
            "#,
        )
        .bind(case_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        match row {
            Some(row) => {
                let append_entries = load_append_entries(&self.pool, &[case_id.to_string()]).await?;
                Ok(Some(row_to_case(
                    &row,
                    append_entries.get(case_id).cloned().unwrap_or_default(),
                    true,
                )))
            }
            None => Ok(None),
        }
    }

    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
        self.find_filtered(Some(flight_id), None, None, None, None).await
    }

    async fn find_by_id_scoped(
        &self,
        case_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        Ok(self
            .find_by_id(case_id)
            .await?
            .filter(|case| is_case_visible(case, viewer_department_id, viewer_department_name, include_common)))
    }

    async fn find_by_flight_scoped(
        &self,
        flight_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .find_by_flight(flight_id)
            .await?
            .into_iter()
            .filter(|case| is_case_visible(case, viewer_department_id, viewer_department_name, include_common))
            .collect())
    }

    async fn find_by_flight_ids(&self, flight_ids: &[String]) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let normalized_ids = flight_ids
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if normalized_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT
                c.*,
                COALESCE(f.flight_number, '') AS flight_no,
                wr.receipt_group_id,
                wr_sg.title AS receipt_title,
                wr_sg.severity AS receipt_severity,
                wr_sg.origin_type AS receipt_origin_type,
                wr_sg.created_at AS receipt_created_at,
                wr_sg.total_count AS receipt_total_count,
                wr_sg.pending_count AS receipt_pending_count,
                wr_sg.acknowledged_count AS receipt_acknowledged_count,
                wr_sg.rejected_count AS receipt_rejected_count,
                wr_sg.latest_updated_at AS receipt_latest_updated_at,
                wr_sg.remind_after_at AS receipt_remind_after_at,
                wr_items.items_json AS receipt_items
            FROM flight_business_cases c
            LEFT JOIN flights f ON f.flight_id = c.flight_id
            LEFT JOIN business_case_workflow_runs wr ON wr.case_id = c.case_id
            LEFT JOIN LATERAL (
                SELECT
                    MIN(n.title) AS title,
                    MIN(n.severity) AS severity,
                    MIN(n.origin_type) AS origin_type,
                    MIN(n.created_at) AS created_at,
                    COUNT(*) AS total_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'pending') AS pending_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'acknowledged') AS acknowledged_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'rejected') AS rejected_count,
                    MAX(COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)) AS latest_updated_at,
                    MIN(n.created_at) + INTERVAL '2 minutes' AS remind_after_at
                FROM notifications n
                WHERE n.receipt_group_id = wr.receipt_group_id
                GROUP BY n.receipt_group_id
            ) wr_sg ON true
            LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'user_id', n.user_id,
                        'recipient_user_id', n.user_id,
                        'recipient_username', COALESCE(
                            NULLIF(trim(n.recipient_username_snapshot), ''),
                            NULLIF(trim(n.recipient_display_name_snapshot), ''),
                            NULLIF(trim(ru.username), ''),
                            '未知账号'
                        ),
                        'recipient_display_name', COALESCE(NULLIF(trim(n.recipient_display_name_snapshot), ''), ru.display_name),
                        'recipient_department', COALESCE(NULLIF(trim(n.recipient_department_snapshot), ''), ru.department),
                        'recipient_job_title', COALESCE(NULLIF(trim(n.recipient_job_title_snapshot), ''), ru.job_title),
                        'ack_status', n.ack_status,
                        'ack_at', n.ack_at,
                        'ack_note', n.ack_note,
                        'updated_at', COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)
                    )
                    ORDER BY n.created_at ASC, n.notification_id ASC
                ) AS items_json
                FROM notifications n
                LEFT JOIN users ru ON ru.id = n.user_id
                WHERE n.receipt_group_id = wr.receipt_group_id
            ) wr_items ON true
            WHERE c.flight_id = ANY($1) AND c.deleted_at IS NULL
            ORDER BY c.created_at DESC
            "#,
        )
        .bind(&normalized_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        let case_ids = rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("case_id").ok())
            .collect::<Vec<_>>();
        let append_entries = load_append_entries(&self.pool, &case_ids).await?;

        Ok(rows
            .iter()
            .map(|row| {
                let case_id = row.try_get::<String, _>("case_id").unwrap_or_default();
                row_to_case(row, append_entries.get(&case_id).cloned().unwrap_or_default(), false)
            })
            .collect())
    }

    async fn find_by_copilot_batch_action(
        &self,
        batch_id: &str,
        action_id: &str,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT c.*, COALESCE(f.flight_number, '') AS flight_no
            FROM flight_business_cases c
            LEFT JOIN flights f ON f.flight_id = c.flight_id
            WHERE c.context->>'source' = 'ai_copilot_voice'
              AND c.context->>'copilot_batch_id' = $1
              AND c.context->>'copilot_action_id' = $2
              AND c.deleted_at IS NULL
            ORDER BY c.created_at ASC, c.case_id ASC
            LIMIT 1
            "#,
        )
        .bind(batch_id)
        .bind(action_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        match row {
            Some(row) => {
                let case_id = row.try_get::<String, _>("case_id").unwrap_or_default();
                let append_entries = load_append_entries(&self.pool, std::slice::from_ref(&case_id)).await?;
                Ok(Some(row_to_case(
                    &row,
                    append_entries.get(&case_id).cloned().unwrap_or_default(),
                    false,
                )))
            }
            None => Ok(None),
        }
    }

    async fn list_by_copilot_batch(&self, batch_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT c.*, COALESCE(f.flight_number, '') AS flight_no
            FROM flight_business_cases c
            LEFT JOIN flights f ON f.flight_id = c.flight_id
            WHERE c.context->>'source' = 'ai_copilot_voice'
              AND c.context->>'copilot_batch_id' = $1
              AND c.deleted_at IS NULL
            ORDER BY c.created_at ASC, c.case_id ASC
            "#,
        )
        .bind(batch_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        let case_ids = rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("case_id").ok())
            .collect::<Vec<_>>();
        let append_entries = load_append_entries(&self.pool, &case_ids).await?;

        Ok(rows
            .iter()
            .map(|row| {
                let case_id = row.try_get::<String, _>("case_id").unwrap_or_default();
                row_to_case(row, append_entries.get(&case_id).cloned().unwrap_or_default(), false)
            })
            .collect())
    }

    async fn find_by_flight_ids_scoped(
        &self,
        flight_ids: &[String],
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .find_by_flight_ids(flight_ids)
            .await?
            .into_iter()
            .filter(|case| is_case_visible(case, viewer_department_id, viewer_department_name, include_common))
            .collect())
    }

    async fn find_all(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let select = r#"
            SELECT
                c.*,
                COALESCE(f.flight_number, '') AS flight_no,
                wr.receipt_group_id,
                wr_sg.title AS receipt_title,
                wr_sg.severity AS receipt_severity,
                wr_sg.origin_type AS receipt_origin_type,
                wr_sg.created_at AS receipt_created_at,
                wr_sg.total_count AS receipt_total_count,
                wr_sg.pending_count AS receipt_pending_count,
                wr_sg.acknowledged_count AS receipt_acknowledged_count,
                wr_sg.rejected_count AS receipt_rejected_count,
                wr_sg.latest_updated_at AS receipt_latest_updated_at,
                wr_sg.remind_after_at AS receipt_remind_after_at,
                wr_items.items_json AS receipt_items
            FROM flight_business_cases c
            LEFT JOIN flights f ON f.flight_id = c.flight_id
            LEFT JOIN business_case_workflow_runs wr ON wr.case_id = c.case_id
            LEFT JOIN LATERAL (
                SELECT
                    MIN(n.title) AS title,
                    MIN(n.severity) AS severity,
                    MIN(n.origin_type) AS origin_type,
                    MIN(n.created_at) AS created_at,
                    COUNT(*) AS total_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'pending') AS pending_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'acknowledged') AS acknowledged_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'rejected') AS rejected_count,
                    MAX(COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)) AS latest_updated_at,
                    MIN(n.created_at) + INTERVAL '2 minutes' AS remind_after_at
                FROM notifications n
                WHERE n.receipt_group_id = wr.receipt_group_id
                GROUP BY n.receipt_group_id
            ) wr_sg ON true
            LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'user_id', n.user_id,
                        'recipient_user_id', n.user_id,
                        'recipient_username', COALESCE(
                            NULLIF(trim(n.recipient_username_snapshot), ''),
                            NULLIF(trim(n.recipient_display_name_snapshot), ''),
                            NULLIF(trim(ru.username), ''),
                            '未知账号'
                        ),
                        'recipient_display_name', COALESCE(NULLIF(trim(n.recipient_display_name_snapshot), ''), ru.display_name),
                        'recipient_department', COALESCE(NULLIF(trim(n.recipient_department_snapshot), ''), ru.department),
                        'recipient_job_title', COALESCE(NULLIF(trim(n.recipient_job_title_snapshot), ''), ru.job_title),
                        'ack_status', n.ack_status,
                        'ack_at', n.ack_at,
                        'ack_note', n.ack_note,
                        'updated_at', COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)
                    )
                    ORDER BY n.created_at ASC, n.notification_id ASC
                ) AS items_json
                FROM notifications n
                LEFT JOIN users ru ON ru.id = n.user_id
                WHERE n.receipt_group_id = wr.receipt_group_id
            ) wr_items ON true
        "#;
        let base = if status.is_some() {
            format!(
                r#"{} WHERE c.deleted_at IS NULL AND c.status = $1 ORDER BY c.created_at DESC LIMIT $2 OFFSET $3"#,
                select
            )
        } else {
            format!(
                r#"{} WHERE c.deleted_at IS NULL ORDER BY c.created_at DESC LIMIT $1 OFFSET $2"#,
                select
            )
        };

        let rows = if let Some(status) = status {
            sqlx::query(&base)
                .bind(status)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query(&base).bind(limit).bind(offset).fetch_all(&self.pool).await
        }
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        let case_ids = rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("case_id").ok())
            .collect::<Vec<_>>();
        let append_entries = load_append_entries(&self.pool, &case_ids).await?;

        Ok(rows
            .iter()
            .map(|row| {
                let case_id = row.try_get::<String, _>("case_id").unwrap_or_default();
                row_to_case(row, append_entries.get(&case_id).cloned().unwrap_or_default(), false)
            })
            .collect())
    }

    async fn find_all_scoped(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        self.find_filtered_scoped(
            None,
            None,
            status,
            viewer_department_id,
            viewer_department_name,
            include_common,
            Some(limit),
            Some(offset),
        )
        .await
    }

    async fn find_filtered(
        &self,
        flight_id: Option<&str>,
        case_type: Option<&str>,
        status: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let mut conditions = Vec::new();
        let mut binds = Vec::new();

        // 审计要求软删除：读侧预置过滤
        conditions.push("c.deleted_at IS NULL".to_string());

        if let Some(flight_id) = flight_id.filter(|value| !value.trim().is_empty()) {
            binds.push(flight_id.trim().to_string());
            conditions.push(format!("c.flight_id = ${}", binds.len()));
        }
        if let Some(case_type) = case_type.filter(|value| !value.trim().is_empty()) {
            binds.push(case_type.trim().to_string());
            conditions.push(format!("c.case_type = ${}", binds.len()));
        }
        if let Some(status) = status.filter(|value| !value.trim().is_empty()) {
            binds.push(status.trim().to_string());
            conditions.push(format!("c.status = ${}", binds.len()));
        }

        let mut sql = String::from(
            r#"
            SELECT
                c.*,
                COALESCE(f.flight_number, '') AS flight_no,
                wr.receipt_group_id,
                wr_sg.title AS receipt_title,
                wr_sg.severity AS receipt_severity,
                wr_sg.origin_type AS receipt_origin_type,
                wr_sg.created_at AS receipt_created_at,
                wr_sg.total_count AS receipt_total_count,
                wr_sg.pending_count AS receipt_pending_count,
                wr_sg.acknowledged_count AS receipt_acknowledged_count,
                wr_sg.rejected_count AS receipt_rejected_count,
                wr_sg.latest_updated_at AS receipt_latest_updated_at,
                wr_sg.remind_after_at AS receipt_remind_after_at,
                wr_items.items_json AS receipt_items
            FROM flight_business_cases c
            LEFT JOIN flights f ON f.flight_id = c.flight_id
            LEFT JOIN business_case_workflow_runs wr ON wr.case_id = c.case_id
            LEFT JOIN LATERAL (
                SELECT
                    MIN(n.title) AS title,
                    MIN(n.severity) AS severity,
                    MIN(n.origin_type) AS origin_type,
                    MIN(n.created_at) AS created_at,
                    COUNT(*) AS total_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'pending') AS pending_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'acknowledged') AS acknowledged_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'rejected') AS rejected_count,
                    MAX(COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)) AS latest_updated_at,
                    MIN(n.created_at) + INTERVAL '2 minutes' AS remind_after_at
                FROM notifications n
                WHERE n.receipt_group_id = wr.receipt_group_id
                GROUP BY n.receipt_group_id
            ) wr_sg ON true
            LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'user_id', n.user_id,
                        'recipient_user_id', n.user_id,
                        'recipient_username', COALESCE(
                            NULLIF(trim(n.recipient_username_snapshot), ''),
                            NULLIF(trim(n.recipient_display_name_snapshot), ''),
                            NULLIF(trim(ru.username), ''),
                            '未知账号'
                        ),
                        'recipient_display_name', COALESCE(NULLIF(trim(n.recipient_display_name_snapshot), ''), ru.display_name),
                        'recipient_department', COALESCE(NULLIF(trim(n.recipient_department_snapshot), ''), ru.department),
                        'recipient_job_title', COALESCE(NULLIF(trim(n.recipient_job_title_snapshot), ''), ru.job_title),
                        'ack_status', n.ack_status,
                        'ack_at', n.ack_at,
                        'ack_note', n.ack_note,
                        'updated_at', COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)
                    )
                    ORDER BY n.created_at ASC, n.notification_id ASC
                ) AS items_json
                FROM notifications n
                LEFT JOIN users ru ON ru.id = n.user_id
                WHERE n.receipt_group_id = wr.receipt_group_id
            ) wr_items ON true
            "#,
        );
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY c.created_at DESC");
        if let Some(limit) = limit {
            binds.push(limit.max(1).to_string());
            sql.push_str(&format!(" LIMIT ${}", binds.len()));
        }
        if let Some(offset) = offset {
            binds.push(offset.max(0).to_string());
            sql.push_str(&format!(" OFFSET ${}", binds.len()));
        }

        let mut query = sqlx::query(&sql);
        for bind in &binds {
            query = query.bind(bind);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        let case_ids = rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("case_id").ok())
            .collect::<Vec<_>>();
        let append_entries = load_append_entries(&self.pool, &case_ids).await?;

        Ok(rows
            .iter()
            .map(|row| {
                let case_id = row.try_get::<String, _>("case_id").unwrap_or_default();
                row_to_case(row, append_entries.get(&case_id).cloned().unwrap_or_default(), false)
            })
            .collect())
    }

    async fn find_filtered_scoped(
        &self,
        flight_id: Option<&str>,
        case_type: Option<&str>,
        status: Option<&str>,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let mut conditions = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        // 审计要求软删除：读侧预置过滤
        conditions.push("c.deleted_at IS NULL".to_string());

        if let Some(flight_id) = flight_id.filter(|value| !value.trim().is_empty()) {
            binds.push(flight_id.trim().to_string());
            conditions.push(format!("c.flight_id = ${}", binds.len()));
        }
        if let Some(case_type) = case_type.filter(|value| !value.trim().is_empty()) {
            binds.push(case_type.trim().to_string());
            conditions.push(format!("c.case_type = ${}", binds.len()));
        }
        if let Some(status) = status.filter(|value| !value.trim().is_empty()) {
            binds.push(status.trim().to_string());
            conditions.push(format!("c.status = ${}", binds.len()));
        }

        let viewer_department_id = normalize_optional_scope_value(viewer_department_id);
        let viewer_department_name = normalize_optional_scope_value(viewer_department_name);

        let scope_conditions = if include_common {
            if viewer_department_id.is_some() || viewer_department_name.is_some() {
                let mut or_parts = Vec::new();
                or_parts.push("c.visibility_scope = 'COMMON'".to_string());
                if let Some(ref dept_id) = viewer_department_id {
                    binds.push(dept_id.clone());
                    or_parts.push(format!(
                        "(c.visibility_scope = 'DEPARTMENT' AND c.department_id = ${})",
                        binds.len()
                    ));
                }
                if let Some(ref dept_name) = viewer_department_name {
                    binds.push(dept_name.clone());
                    or_parts.push(format!(
                        "(c.visibility_scope = 'DEPARTMENT' AND c.department_name_snapshot = ${})",
                        binds.len()
                    ));
                }
                Some(format!("({})", or_parts.join(" OR ")))
            } else {
                Some("c.visibility_scope = 'COMMON'".to_string())
            }
        } else if viewer_department_id.is_some() || viewer_department_name.is_some() {
            let mut or_parts = Vec::new();
            if let Some(ref dept_id) = viewer_department_id {
                binds.push(dept_id.clone());
                or_parts.push(format!(
                    "(c.visibility_scope = 'DEPARTMENT' AND c.department_id = ${})",
                    binds.len()
                ));
            }
            if let Some(ref dept_name) = viewer_department_name {
                binds.push(dept_name.clone());
                or_parts.push(format!(
                    "(c.visibility_scope = 'DEPARTMENT' AND c.department_name_snapshot = ${})",
                    binds.len()
                ));
            }
            if or_parts.is_empty() {
                Some("1 = 0".to_string())
            } else {
                Some(format!("({})", or_parts.join(" OR ")))
            }
        } else {
            Some("1 = 0".to_string())
        };

        if let Some(scope_sql) = scope_conditions {
            conditions.push(scope_sql);
        }

        let mut sql = String::from(
            r#"
            SELECT
                c.*,
                COALESCE(f.flight_number, '') AS flight_no,
                wr.receipt_group_id,
                wr_sg.title AS receipt_title,
                wr_sg.severity AS receipt_severity,
                wr_sg.origin_type AS receipt_origin_type,
                wr_sg.created_at AS receipt_created_at,
                wr_sg.total_count AS receipt_total_count,
                wr_sg.pending_count AS receipt_pending_count,
                wr_sg.acknowledged_count AS receipt_acknowledged_count,
                wr_sg.rejected_count AS receipt_rejected_count,
                wr_sg.latest_updated_at AS receipt_latest_updated_at,
                wr_sg.remind_after_at AS receipt_remind_after_at,
                wr_items.items_json AS receipt_items
            FROM flight_business_cases c
            LEFT JOIN flights f ON f.flight_id = c.flight_id
            LEFT JOIN business_case_workflow_runs wr ON wr.case_id = c.case_id
            LEFT JOIN LATERAL (
                SELECT
                    MIN(n.title) AS title,
                    MIN(n.severity) AS severity,
                    MIN(n.origin_type) AS origin_type,
                    MIN(n.created_at) AS created_at,
                    COUNT(*) AS total_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'pending') AS pending_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'acknowledged') AS acknowledged_count,
                    COUNT(*) FILTER (WHERE n.ack_status = 'rejected') AS rejected_count,
                    MAX(COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)) AS latest_updated_at,
                    MIN(n.created_at) + INTERVAL '2 minutes' AS remind_after_at
                FROM notifications n
                WHERE n.receipt_group_id = wr.receipt_group_id
                GROUP BY n.receipt_group_id
            ) wr_sg ON true
            LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'user_id', n.user_id,
                        'recipient_user_id', n.user_id,
                        'recipient_username', COALESCE(
                            NULLIF(trim(n.recipient_username_snapshot), ''),
                            NULLIF(trim(n.recipient_display_name_snapshot), ''),
                            NULLIF(trim(ru.username), ''),
                            '未知账号'
                        ),
                        'recipient_display_name', COALESCE(NULLIF(trim(n.recipient_display_name_snapshot), ''), ru.display_name),
                        'recipient_department', COALESCE(NULLIF(trim(n.recipient_department_snapshot), ''), ru.department),
                        'recipient_job_title', COALESCE(NULLIF(trim(n.recipient_job_title_snapshot), ''), ru.job_title),
                        'ack_status', n.ack_status,
                        'ack_at', n.ack_at,
                        'ack_note', n.ack_note,
                        'updated_at', COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)
                    )
                    ORDER BY n.created_at ASC, n.notification_id ASC
                ) AS items_json
                FROM notifications n
                LEFT JOIN users ru ON ru.id = n.user_id
                WHERE n.receipt_group_id = wr.receipt_group_id
            ) wr_items ON true
            "#,
        );
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY c.created_at DESC");
        if let Some(limit) = limit {
            binds.push(limit.max(1).to_string());
            sql.push_str(&format!(" LIMIT ${}", binds.len()));
        }
        if let Some(offset) = offset {
            binds.push(offset.max(0).to_string());
            sql.push_str(&format!(" OFFSET ${}", binds.len()));
        }

        let mut query = sqlx::query(&sql);
        for bind in &binds {
            query = query.bind(bind);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        let case_ids = rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("case_id").ok())
            .collect::<Vec<_>>();
        let append_entries = load_append_entries(&self.pool, &case_ids).await?;

        Ok(rows
            .iter()
            .map(|row| {
                let case_id = row.try_get::<String, _>("case_id").unwrap_or_default();
                row_to_case(row, append_entries.get(&case_id).cloned().unwrap_or_default(), false)
            })
            .collect())
    }

    async fn update_case(&self, case: &FlightBusinessCase) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE flight_business_cases
            SET case_type = $1,
                description = $2,
                context = $3,
                updated_by = $4,
                status = $5,
                stand = $6,
                gate = $7,
                finished_at = $8,
                cancelled_at = $9,
                log = $10
            WHERE case_id = $11 AND deleted_at IS NULL
            "#,
        )
        .bind(&case.case_type)
        .bind(&case.description)
        .bind(build_context_json(case))
        .bind(&case.updated_by)
        .bind(&case.status)
        .bind(&case.stand)
        .bind(&case.gate)
        .bind(case.finished_at)
        .bind(case.cancelled_at)
        .bind(business_case_log_to_text_array(&case.log))
        .bind(&case.case_id)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_status(&self, case_id: &str, status: &str, actor: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE flight_business_cases SET status = $1, updated_by = $2 \
             WHERE case_id = $3 AND status IS DISTINCT FROM $1 AND deleted_at IS NULL",
        )
        .bind(status)
        .bind(actor)
        .bind(case_id)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn insert_append(&self, append: &BusinessCaseAppendEntry) -> Result<BusinessCaseAppendEntry, DomainError> {
        let metadata_json = serde_json::to_value(&append.metadata).unwrap_or(serde_json::Value::Null);
        let row = sqlx::query(
            r#"
            INSERT INTO flight_business_case_appends (
                append_id, case_id, content, client_action_id, submitted_by, submitted_operator_name, appended_at, metadata
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            RETURNING append_id, case_id, content, client_action_id, submitted_by, submitted_operator_name, appended_at, metadata
            "#,
        )
        .bind(&append.append_id)
        .bind(&append.case_id)
        .bind(&append.content)
        .bind(&append.client_action_id)
        .bind(&append.submitted_by)
        .bind(&append.submitted_operator_name)
        .bind(append.appended_at)
        .bind(&metadata_json)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row_to_append_entry(&row))
    }

    async fn insert_append_once(
        &self,
        append: &BusinessCaseAppendEntry,
    ) -> Result<(BusinessCaseAppendEntry, bool), DomainError> {
        let metadata_json = serde_json::to_value(&append.metadata).unwrap_or(serde_json::Value::Null);
        let row = sqlx::query(
            r#"
            WITH inserted AS (
                INSERT INTO flight_business_case_appends (
                    append_id, case_id, content, client_action_id, submitted_by, submitted_operator_name, appended_at, metadata
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                ON CONFLICT (case_id, client_action_id)
                    WHERE client_action_id IS NOT NULL
                DO NOTHING
                RETURNING append_id, case_id, content, client_action_id, submitted_by, submitted_operator_name, appended_at, metadata, TRUE AS inserted
            )
            SELECT append_id, case_id, content, client_action_id, submitted_by, submitted_operator_name, appended_at, metadata, inserted
            FROM inserted
            UNION ALL
            SELECT append_id, case_id, content, client_action_id, submitted_by, submitted_operator_name, appended_at, metadata, FALSE AS inserted
            FROM flight_business_case_appends
            WHERE case_id = $2 AND client_action_id = $4 AND $4 IS NOT NULL
            LIMIT 1
            "#,
        )
        .bind(&append.append_id)
        .bind(&append.case_id)
        .bind(&append.content)
        .bind(&append.client_action_id)
        .bind(&append.submitted_by)
        .bind(&append.submitted_operator_name)
        .bind(append.appended_at)
        .bind(&metadata_json)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok((row_to_append_entry(&row), row.get("inserted")))
    }

    async fn find_append_by_id(&self, append_id: &str) -> Result<Option<BusinessCaseAppendEntry>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT append_id, case_id, content, client_action_id, submitted_by, submitted_operator_name, appended_at, metadata
            FROM flight_business_case_appends
            WHERE append_id = $1
            "#,
        )
        .bind(append_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row.map(|r| row_to_append_entry(&r)))
    }

    async fn update_append_metadata(&self, append_id: &str, metadata: serde_json::Value) -> Result<bool, DomainError> {
        let result = sqlx::query("UPDATE flight_business_case_appends SET metadata = $1 WHERE append_id = $2")
            .bind(metadata)
            .bind(append_id)
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete(&self, case_id: &str) -> Result<bool, DomainError> {
        // 审计要求软删除：仅标记 deleted_at，行与 append 记录全部保留
        let result = sqlx::query(
            "UPDATE flight_business_cases SET deleted_at = NOW() \
             WHERE case_id = $1 AND deleted_at IS NULL",
        )
        .bind(case_id)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        let deleted = result.rows_affected() > 0;
        if deleted {
            record_soft_delete(&self.pool, "flight_business_case", case_id, "soft_delete").await;
        }
        Ok(deleted)
    }
}

#[async_trait]
impl<'tx> BusinessCaseTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>> for PgBusinessCaseRepository {
    async fn save_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        case: &FlightBusinessCase,
    ) -> Result<(), DomainError> {
        let context_json = build_context_json(case);
        let log_entries = business_case_log_to_text_array(&case.log);
        sqlx::query(
            r#"
            INSERT INTO flight_business_cases (
                case_id, flight_id, case_type, description, context, created_by, updated_by,
                created_at, status, stand, gate, visibility_scope, department_id,
                department_name_snapshot, finished_at, cancelled_at, log
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
            ON CONFLICT (case_id) DO UPDATE SET
                case_type = EXCLUDED.case_type,
                description = EXCLUDED.description,
                context = EXCLUDED.context,
                updated_by = EXCLUDED.updated_by,
                status = EXCLUDED.status,
                stand = EXCLUDED.stand,
                gate = EXCLUDED.gate,
                visibility_scope = EXCLUDED.visibility_scope,
                department_id = EXCLUDED.department_id,
                department_name_snapshot = EXCLUDED.department_name_snapshot,
                finished_at = EXCLUDED.finished_at,
                cancelled_at = EXCLUDED.cancelled_at,
                log = EXCLUDED.log,
                deleted_at = NULL
            "#,
        )
        .bind(&case.case_id)
        .bind(&case.flight_id)
        .bind(&case.case_type)
        .bind(&case.description)
        .bind(&context_json)
        .bind(&case.created_by)
        .bind(&case.updated_by)
        .bind(case.created_at)
        .bind(&case.status)
        .bind(&case.stand)
        .bind(&case.gate)
        .bind(case.visibility_scope.as_str())
        .bind(&case.department_id)
        .bind(&case.department_name_snapshot)
        .bind(case.finished_at)
        .bind(case.cancelled_at)
        .bind(&log_entries)
        .execute(&mut **tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn update_case_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        case: &FlightBusinessCase,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE flight_business_cases
            SET case_type = $1,
                description = $2,
                context = $3,
                updated_by = $4,
                status = $5,
                stand = $6,
                gate = $7,
                finished_at = $8,
                cancelled_at = $9,
                log = $10
            WHERE case_id = $11 AND deleted_at IS NULL
            "#,
        )
        .bind(&case.case_type)
        .bind(&case.description)
        .bind(build_context_json(case))
        .bind(&case.updated_by)
        .bind(&case.status)
        .bind(&case.stand)
        .bind(&case.gate)
        .bind(case.finished_at)
        .bind(case.cancelled_at)
        .bind(business_case_log_to_text_array(&case.log))
        .bind(&case.case_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}

async fn load_append_entries(
    pool: &PgPool,
    case_ids: &[String],
) -> Result<std::collections::HashMap<String, Vec<BusinessCaseAppendEntry>>, DomainError> {
    let normalized_ids = case_ids
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if normalized_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT append_id, case_id, content, client_action_id, submitted_by, submitted_operator_name, appended_at, metadata
        FROM flight_business_case_appends
        WHERE case_id = ANY($1)
        ORDER BY appended_at ASC, append_id ASC
        "#,
    )
    .bind(&normalized_ids)
    .fetch_all(pool)
    .await
    .map_err(|error| DomainError::Internal(error.to_string()))?;

    let mut result = normalized_ids
        .into_iter()
        .map(|case_id| (case_id, Vec::new()))
        .collect::<std::collections::HashMap<_, _>>();

    for row in rows {
        let case_id = row.get::<String, _>("case_id");
        result
            .entry(case_id.clone())
            .or_default()
            .push(row_to_append_entry(&row));
    }

    Ok(result)
}

fn row_to_append_entry(row: &sqlx::postgres::PgRow) -> BusinessCaseAppendEntry {
    BusinessCaseAppendEntry {
        append_id: row.get("append_id"),
        case_id: row.get("case_id"),
        content: row.get("content"),
        client_action_id: row.try_get("client_action_id").ok().flatten(),
        submitted_by: row.get("submitted_by"),
        submitted_operator_name: row.get("submitted_operator_name"),
        appended_at: row.get("appended_at"),
        metadata: row
            .get::<Option<serde_json::Value>, _>("metadata")
            .unwrap_or(serde_json::Value::Null),
    }
}

fn row_to_case(
    row: &sqlx::postgres::PgRow,
    append_entries: Vec<BusinessCaseAppendEntry>,
    include_append_entries: bool,
) -> FlightBusinessCase {
    let context_val = row
        .try_get::<Option<serde_json::Value>, _>("context")
        .ok()
        .flatten()
        .unwrap_or_else(|| serde_json::json!({}));
    let context: std::collections::HashMap<String, serde_json::Value> = match context_val {
        serde_json::Value::Object(map) => map.into_iter().collect(),
        _ => Default::default(),
    };
    let terminal_metadata = context
        .get("workflow_terminal")
        .and_then(|value| serde_json::from_value::<BusinessCaseTerminalMetadata>(value.clone()).ok());
    let log = if let Some(items) = row.try_get::<Option<Vec<String>>, _>("log").ok().flatten() {
        business_case_log_from_text_array(items)
    } else {
        let log_val = row
            .try_get::<Option<serde_json::Value>, _>("log")
            .ok()
            .flatten()
            .unwrap_or_else(|| serde_json::json!([]));
        match log_val {
            serde_json::Value::Array(items) => items,
            _ => Vec::new(),
        }
    };
    let latest_append = append_entries.last().cloned();
    let department_id = row.try_get("department_id").ok().flatten();
    let department_name_snapshot = row.try_get("department_name_snapshot").ok().flatten();
    let visibility_scope = VisibilityScope::from_optional_str(
        row.try_get::<Option<String>, _>("visibility_scope")
            .ok()
            .flatten()
            .as_deref(),
        department_id.as_deref(),
        department_name_snapshot.as_deref(),
    );
    let workflow_receipt = build_receipt_from_row(row);

    FlightBusinessCase {
        case_id: row.get("case_id"),
        case_type: row.get("case_type"),
        case_type_name: None,
        flight_id: row.get("flight_id"),
        flight_no: row
            .try_get::<Option<String>, _>("flight_no")
            .ok()
            .flatten()
            .unwrap_or_default(),
        created_at: row
            .try_get::<Option<chrono::DateTime<Utc>>, _>("created_at")
            .ok()
            .flatten()
            .unwrap_or_else(Utc::now),
        created_by: row
            .try_get::<Option<String>, _>("created_by")
            .ok()
            .flatten()
            .unwrap_or_else(|| "system".to_string()),
        updated_by: row
            .try_get::<Option<String>, _>("updated_by")
            .ok()
            .flatten()
            .unwrap_or_else(|| "system".to_string()),
        description: row
            .try_get::<Option<String>, _>("description")
            .ok()
            .flatten()
            .unwrap_or_default(),
        status: row
            .try_get::<Option<String>, _>("status")
            .ok()
            .flatten()
            .unwrap_or_else(|| "PENDING".to_string()),
        stand: row.try_get("stand").ok().flatten(),
        gate: row.try_get("gate").ok().flatten(),
        visibility_scope,
        department_id,
        department_name_snapshot,
        finished_at: row.try_get("finished_at").ok().flatten(),
        cancelled_at: row.try_get("cancelled_at").ok().flatten(),
        log,
        context,
        workflow_receipt,
        terminal_metadata,
        append_count: append_entries.len() as i32,
        latest_append,
        append_entries: if include_append_entries {
            append_entries
        } else {
            Vec::new()
        },
    }
}

fn build_receipt_from_row(row: &sqlx::postgres::PgRow) -> Option<BusinessCaseWorkflowReceiptProjection> {
    let receipt_group_id: String = row.try_get("receipt_group_id").ok().flatten()?;
    let receipt_group_id = receipt_group_id.trim().to_string();
    if receipt_group_id.is_empty() {
        return None;
    }

    let pending_count: i64 = row.try_get("receipt_pending_count").ok().unwrap_or(0);
    let rejected_count: i64 = row.try_get("receipt_rejected_count").ok().unwrap_or(0);
    let remind_after_at: Option<DateTime<Utc>> = row.try_get("receipt_remind_after_at").ok().flatten();
    let is_overdue = pending_count > 0 && remind_after_at.is_some_and(|value| value <= Utc::now());

    Some(BusinessCaseWorkflowReceiptProjection {
        receipt_group_id,
        title: row.try_get("receipt_title").ok().flatten(),
        severity: row.try_get("receipt_severity").ok().flatten(),
        origin_type: row
            .try_get("receipt_origin_type")
            .ok()
            .flatten()
            .map(|value: String| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "manual".to_string()),
        created_at: row.try_get("receipt_created_at").ok().flatten(),
        summary: BusinessCaseWorkflowReceiptSummary {
            total_count: row.try_get("receipt_total_count").ok().unwrap_or(0),
            pending_count,
            acknowledged_count: row.try_get("receipt_acknowledged_count").ok().unwrap_or(0),
            rejected_count,
            latest_updated_at: row.try_get("receipt_latest_updated_at").ok().flatten(),
            remind_after_at,
            is_overdue,
            overall_status: derive_receipt_projection_status(pending_count, rejected_count),
        },
        items: row
            .try_get::<Option<serde_json::Value>, _>("receipt_items")
            .ok()
            .flatten()
            .and_then(|value| serde_json::from_value::<Vec<BusinessCaseWorkflowReceiptItem>>(value).ok())
            .unwrap_or_default(),
    })
}

fn derive_receipt_projection_status(pending_count: i64, rejected_count: i64) -> String {
    if rejected_count > 0 {
        "rejected".to_string()
    } else if pending_count > 0 {
        "pending".to_string()
    } else {
        "acknowledged".to_string()
    }
}

fn build_context_json(case: &FlightBusinessCase) -> serde_json::Value {
    let mut context = case.context.clone();
    if let Some(metadata) = case.terminal_metadata.as_ref() {
        context.insert(
            "workflow_terminal".to_string(),
            serde_json::to_value(metadata).unwrap_or(serde_json::Value::Null),
        );
    }
    serde_json::to_value(context).unwrap_or_default()
}

fn business_case_log_to_text_array(log: &[serde_json::Value]) -> Vec<String> {
    log.iter()
        .map(|entry| serde_json::to_string(entry).unwrap_or_else(|_| entry.to_string()))
        .collect()
}

fn business_case_log_from_text_array(log: Vec<String>) -> Vec<serde_json::Value> {
    log.into_iter()
        .map(|entry| serde_json::from_str::<serde_json::Value>(&entry).unwrap_or(serde_json::Value::String(entry)))
        .collect()
}

fn normalize_optional_scope_value(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|item| !item.is_empty()).map(str::to_string)
}

fn is_case_visible(
    case: &FlightBusinessCase,
    viewer_department_id: Option<&str>,
    viewer_department_name: Option<&str>,
    include_common: bool,
) -> bool {
    match case.visibility_scope {
        VisibilityScope::Common => include_common,
        VisibilityScope::Department => {
            let viewer_department_id = normalize_optional_scope_value(viewer_department_id);
            let viewer_department_name = normalize_optional_scope_value(viewer_department_name);

            case.department_id.is_some() && case.department_id == viewer_department_id
                || case.department_name_snapshot.is_some() && case.department_name_snapshot == viewer_department_name
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{business_case_log_from_text_array, business_case_log_to_text_array};

    #[test]
    fn business_case_log_round_trips_json_entries_through_text_array_storage() {
        let source = vec![
            serde_json::json!({
                "action": "complete",
                "operator": "ops",
                "target_status": "SUCCESS"
            }),
            serde_json::Value::String("legacy plain text".to_string()),
        ];

        let encoded = business_case_log_to_text_array(&source);

        assert_eq!(
            encoded[0],
            r#"{"action":"complete","operator":"ops","target_status":"SUCCESS"}"#
        );
        assert_eq!(encoded[1], "\"legacy plain text\"");
        assert_eq!(business_case_log_from_text_array(encoded), source);
    }
}
