//! PostgreSQL 派工告警仓储实现

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{AlertSeverity, DispatchAlert};
use fms_domain::ports::dispatch_repository::{DispatchAlertRepository, OverrunAlertUpsert};

pub struct PgDispatchAlertRepository {
    pool: PgPool,
}

impl PgDispatchAlertRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const ALERT_COLUMNS: &str = r#"
    id, flight_id, task_type, alert_type, severity, message,
    is_resolved, resolved_at, resolved_by, resolution_notes, notify_users, created_at,
    dedupe_key, current_order_id, next_order_id, last_detected_at,
    occurrence_count, acknowledged_at, acknowledged_by, details
"#;

#[async_trait]
impl DispatchAlertRepository for PgDispatchAlertRepository {
    async fn save(&self, alert: &DispatchAlert) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO dispatch_alerts (
                id, flight_id, task_type, alert_type, severity, message,
                is_resolved, resolved_at, resolved_by, resolution_notes, notify_users, created_at,
                dedupe_key, current_order_id, next_order_id, last_detected_at,
                occurrence_count, acknowledged_at, acknowledged_by, details
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
            ON CONFLICT (id) DO UPDATE SET
                flight_id = EXCLUDED.flight_id,
                task_type = EXCLUDED.task_type,
                alert_type = EXCLUDED.alert_type,
                severity = EXCLUDED.severity,
                message = EXCLUDED.message,
                is_resolved = EXCLUDED.is_resolved,
                resolved_at = EXCLUDED.resolved_at,
                resolved_by = EXCLUDED.resolved_by,
                resolution_notes = EXCLUDED.resolution_notes,
                notify_users = EXCLUDED.notify_users,
                dedupe_key = EXCLUDED.dedupe_key,
                current_order_id = EXCLUDED.current_order_id,
                next_order_id = EXCLUDED.next_order_id,
                last_detected_at = EXCLUDED.last_detected_at,
                occurrence_count = EXCLUDED.occurrence_count,
                acknowledged_at = EXCLUDED.acknowledged_at,
                acknowledged_by = EXCLUDED.acknowledged_by,
                details = EXCLUDED.details
            "#,
        )
        .bind(&alert.id)
        .bind(&alert.flight_id)
        .bind(&alert.task_type)
        .bind(&alert.alert_type)
        .bind(alert_severity_value(alert.severity))
        .bind(&alert.message)
        .bind(alert.is_resolved)
        .bind(alert.resolved_at)
        .bind(&alert.resolved_by)
        .bind(&alert.resolution_notes)
        .bind(&alert.notify_users)
        .bind(alert.created_at.unwrap_or_else(Utc::now))
        .bind(&alert.dedupe_key)
        .bind(&alert.current_order_id)
        .bind(&alert.next_order_id)
        .bind(alert.last_detected_at)
        .bind(alert.occurrence_count)
        .bind(alert.acknowledged_at)
        .bind(&alert.acknowledged_by)
        .bind(&alert.details)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<DispatchAlert>, DomainError> {
        let row = sqlx::query(&format!("SELECT {ALERT_COLUMNS} FROM dispatch_alerts WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(row.as_ref().map(row_to_alert))
    }

    async fn find_unresolved(&self, flight_id: Option<&str>) -> Result<Vec<DispatchAlert>, DomainError> {
        let rows = sqlx::query(&format!(
            r#"
            SELECT {ALERT_COLUMNS}
            FROM dispatch_alerts
            WHERE is_resolved = FALSE
              AND ($1::varchar IS NULL OR flight_id = $1)
            ORDER BY created_at DESC, id DESC
            "#
        ))
        .bind(flight_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(rows.iter().map(row_to_alert).collect())
    }

    async fn resolve(&self, id: &str, resolved_by: &str, notes: Option<&str>) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE dispatch_alerts
            SET is_resolved = TRUE,
                resolved_at = NOW(),
                resolved_by = $2,
                resolution_notes = $3
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(resolved_by)
        .bind(notes)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn upsert_overrun(&self, alert: &DispatchAlert) -> Result<OverrunAlertUpsert, DomainError> {
        let dedupe_key = alert
            .dedupe_key
            .as_deref()
            .ok_or_else(|| DomainError::ValidationError("预排冲突告警缺少 dedupe_key".to_string()))?;
        let row = sqlx::query(
            r#"
            WITH existing AS (
                SELECT is_resolved FROM dispatch_alerts WHERE dedupe_key = $1
            ),
            upserted AS (
                INSERT INTO dispatch_alerts (
                    id, flight_id, task_type, alert_type, severity, message,
                    is_resolved, resolved_at, resolved_by, resolution_notes, notify_users, created_at,
                    dedupe_key, current_order_id, next_order_id, last_detected_at,
                    occurrence_count, acknowledged_at, acknowledged_by, details
                ) VALUES (
                    $2, $3, $4, $5, $6, $7, FALSE, NULL, NULL, NULL, $8, $9,
                    $1, $10, $11, $12, $13, NULL, NULL, $14
                )
                ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL DO UPDATE SET
                    flight_id = EXCLUDED.flight_id,
                    task_type = EXCLUDED.task_type,
                    alert_type = EXCLUDED.alert_type,
                    severity = EXCLUDED.severity,
                    message = EXCLUDED.message,
                    is_resolved = CASE WHEN dispatch_alerts.is_resolved THEN FALSE ELSE dispatch_alerts.is_resolved END,
                    resolved_at = CASE WHEN dispatch_alerts.is_resolved THEN NULL ELSE dispatch_alerts.resolved_at END,
                    resolved_by = CASE WHEN dispatch_alerts.is_resolved THEN NULL ELSE dispatch_alerts.resolved_by END,
                    resolution_notes = CASE WHEN dispatch_alerts.is_resolved THEN NULL ELSE dispatch_alerts.resolution_notes END,
                    acknowledged_at = CASE WHEN dispatch_alerts.is_resolved THEN NULL ELSE dispatch_alerts.acknowledged_at END,
                    acknowledged_by = CASE WHEN dispatch_alerts.is_resolved THEN NULL ELSE dispatch_alerts.acknowledged_by END,
                    occurrence_count = CASE
                        WHEN dispatch_alerts.is_resolved THEN dispatch_alerts.occurrence_count + 1
                        ELSE dispatch_alerts.occurrence_count
                    END,
                    last_detected_at = EXCLUDED.last_detected_at,
                    current_order_id = EXCLUDED.current_order_id,
                    next_order_id = EXCLUDED.next_order_id,
                    notify_users = EXCLUDED.notify_users,
                    details = EXCLUDED.details
                RETURNING *
            )
            SELECT upserted.*,
                   (existing.is_resolved IS NULL) AS inserted_flag,
                   (existing.is_resolved IS TRUE) AS reopened_flag
            FROM upserted LEFT JOIN existing ON TRUE
            "#,
        )
        .bind(dedupe_key)
        .bind(&alert.id)
        .bind(&alert.flight_id)
        .bind(&alert.task_type)
        .bind(&alert.alert_type)
        .bind(alert_severity_value(alert.severity))
        .bind(&alert.message)
        .bind(&alert.notify_users)
        .bind(alert.created_at.unwrap_or_else(Utc::now))
        .bind(&alert.current_order_id)
        .bind(&alert.next_order_id)
        .bind(alert.last_detected_at)
        .bind(alert.occurrence_count)
        .bind(&alert.details)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        let inserted = row.get::<bool, _>("inserted_flag");
        let reopened = row.get::<bool, _>("reopened_flag");
        Ok(OverrunAlertUpsert {
            alert: row_to_alert(&row),
            inserted,
            reopened,
        })
    }

    async fn acknowledge(&self, id: &str, acknowledged_by: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE dispatch_alerts
            SET acknowledged_at = NOW(),
                acknowledged_by = $2
            WHERE id = $1 AND is_resolved = FALSE
            "#,
        )
        .bind(id)
        .bind(acknowledged_by)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn auto_resolve(&self, id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE dispatch_alerts
            SET is_resolved = TRUE,
                resolved_at = NOW(),
                resolved_by = NULL,
                resolution_notes = 'auto'
            WHERE id = $1 AND is_resolved = FALSE
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}

fn row_to_alert(row: &sqlx::postgres::PgRow) -> DispatchAlert {
    DispatchAlert {
        id: row.try_get("id").unwrap_or_default(),
        flight_id: row.try_get("flight_id").ok(),
        task_type: row.try_get("task_type").ok(),
        alert_type: row.try_get("alert_type").unwrap_or_default(),
        severity: parse_alert_severity(row.try_get::<Option<String>, _>("severity").ok().flatten().as_deref()),
        message: row.try_get("message").unwrap_or_default(),
        is_resolved: row.try_get("is_resolved").unwrap_or(false),
        resolved_at: row.try_get("resolved_at").ok(),
        resolved_by: row.try_get("resolved_by").ok(),
        resolution_notes: row.try_get("resolution_notes").ok(),
        notify_users: row.try_get("notify_users").unwrap_or_default(),
        created_at: row.try_get("created_at").ok(),
        dedupe_key: row.try_get("dedupe_key").ok(),
        current_order_id: row.try_get("current_order_id").ok(),
        next_order_id: row.try_get("next_order_id").ok(),
        last_detected_at: row.try_get("last_detected_at").ok(),
        occurrence_count: row.try_get("occurrence_count").unwrap_or(1),
        acknowledged_at: row.try_get("acknowledged_at").ok(),
        acknowledged_by: row.try_get("acknowledged_by").ok(),
        details: row.try_get("details").unwrap_or_default(),
    }
}

fn parse_alert_severity(value: Option<&str>) -> AlertSeverity {
    match value.unwrap_or("warning") {
        "info" => AlertSeverity::Info,
        "critical" => AlertSeverity::Critical,
        _ => AlertSeverity::Warning,
    }
}

fn alert_severity_value(value: AlertSeverity) -> &'static str {
    match value {
        AlertSeverity::Info => "info",
        AlertSeverity::Warning => "warning",
        AlertSeverity::Critical => "critical",
    }
}
