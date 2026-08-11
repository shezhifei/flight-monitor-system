//! PostgreSQL 异常仓储实现

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, Row, Transaction};

use fms_domain::error::DomainError;
use fms_domain::models::anomaly::*;
use fms_domain::ports::anomaly_repository::{AnomalyRepository, AnomalyTransactionalRepository};

pub struct PgAnomalyRepository {
    pool: PgPool,
}

impl PgAnomalyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AnomalyRepository for PgAnomalyRepository {
    async fn find_by_id(&self, anomaly_id: &str) -> Result<Option<Anomaly>, DomainError> {
        let row = sqlx::query("SELECT * FROM anomalies WHERE anomaly_id = $1")
            .bind(anomaly_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_anomaly(&r)))
    }

    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<Anomaly>, DomainError> {
        let rows = sqlx::query("SELECT * FROM anomalies WHERE flight_id = $1 ORDER BY detected_at DESC")
            .bind(flight_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_anomaly).collect())
    }

    async fn find_by_status(&self, status: AnomalyStatus) -> Result<Vec<Anomaly>, DomainError> {
        let status_str = match status {
            AnomalyStatus::Open => "open",
            AnomalyStatus::Acknowledged => "acknowledged",
            AnomalyStatus::Resolved => "resolved",
        };
        let rows = sqlx::query("SELECT * FROM anomalies WHERE status = $1 ORDER BY detected_at DESC LIMIT 200")
            .bind(status_str)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_anomaly).collect())
    }

    async fn list_rules(&self, enabled_only: bool) -> Result<Vec<AnomalyRule>, DomainError> {
        let rows = if enabled_only {
            sqlx::query("SELECT * FROM anomaly_rules WHERE enabled = TRUE ORDER BY rule_id ASC")
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query("SELECT * FROM anomaly_rules ORDER BY rule_id ASC")
                .fetch_all(&self.pool)
                .await
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_rule).collect())
    }

    async fn get_rule(&self, rule_id: &str) -> Result<Option<AnomalyRule>, DomainError> {
        let row = sqlx::query("SELECT * FROM anomaly_rules WHERE rule_id = $1")
            .bind(rule_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|row| row_to_rule(&row)))
    }

    async fn upsert_rule(&self, rule: &AnomalyRule) -> Result<AnomalyRule, DomainError> {
        let row = sqlx::query(
            r#"INSERT INTO anomaly_rules (
                rule_id, rule_type, name, enabled, config,
                severity, auto_create_todo, todo_priority, escalation_intervals
            ) VALUES (
                $1, $2, $3, $4, $5::jsonb,
                $6, $7, $8, $9::jsonb
            )
            ON CONFLICT (rule_id) DO UPDATE SET
                rule_type = EXCLUDED.rule_type,
                name = EXCLUDED.name,
                enabled = EXCLUDED.enabled,
                config = EXCLUDED.config,
                severity = EXCLUDED.severity,
                auto_create_todo = EXCLUDED.auto_create_todo,
                todo_priority = EXCLUDED.todo_priority,
                escalation_intervals = EXCLUDED.escalation_intervals,
                updated_at = CURRENT_TIMESTAMP
            RETURNING *"#,
        )
        .bind(&rule.rule_id)
        .bind(&rule.rule_type)
        .bind(&rule.name)
        .bind(rule.enabled)
        .bind(serde_json::to_value(&rule.config).unwrap_or_else(|_| serde_json::json!({})))
        .bind(&rule.severity)
        .bind(rule.auto_create_todo)
        .bind(normalize_todo_priority_value(Some(rule.todo_priority.as_str())))
        .bind(serde_json::json!(rule.normalized_intervals()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row_to_rule(&row))
    }

    async fn save(&self, a: &Anomaly) -> Result<(), DomainError> {
        let context_json = serde_json::to_value(&a.context_data)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize anomaly context_data: {e}")))?;
        sqlx::query(
            r#"INSERT INTO anomalies (
                anomaly_id, flight_id, anomaly_type, severity, title, description,
                status, detected_at, resolved_at, escalation_level,
                last_escalated_at, linked_todo_id, rule_id,
                context_data, created_at, updated_at
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16
            )
            ON CONFLICT (anomaly_id) DO UPDATE SET
                status = EXCLUDED.status,
                resolved_at = EXCLUDED.resolved_at,
                escalation_level = EXCLUDED.escalation_level,
                last_escalated_at = EXCLUDED.last_escalated_at,
                updated_at = EXCLUDED.updated_at"#,
        )
        .bind(&a.anomaly_id)
        .bind(&a.flight_id)
        .bind(a.anomaly_type.as_ref())
        .bind(a.severity.as_ref())
        .bind(&a.title)
        .bind(&a.description)
        .bind(a.status.as_ref())
        .bind(a.detected_at)
        .bind(a.resolved_at)
        .bind(a.escalation_level)
        .bind(a.last_escalated_at)
        .bind(&a.linked_todo_id)
        .bind(&a.rule_id)
        .bind(context_json)
        .bind(a.created_at)
        .bind(a.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, a: &Anomaly) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE anomalies SET status = $1, resolved_at = $2, escalation_level = $3, last_escalated_at = $4, updated_at = $5 WHERE anomaly_id = $6",
        )
        .bind(a.status.as_ref())
        .bind(a.resolved_at)
        .bind(a.escalation_level)
        .bind(a.last_escalated_at)
        .bind(Utc::now())
        .bind(&a.anomaly_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn acknowledge(&self, anomaly_id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE anomalies SET status = 'acknowledged', updated_at = $1 WHERE anomaly_id = $2 AND status = 'open'",
        )
        .bind(Utc::now())
        .bind(anomaly_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn resolve(&self, anomaly_id: &str) -> Result<bool, DomainError> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE anomalies SET status = 'resolved', resolved_at = $1, updated_at = $1 WHERE anomaly_id = $2 AND status != 'resolved'",
        )
        .bind(now)
        .bind(anomaly_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn escalate(&self, anomaly_id: &str) -> Result<bool, DomainError> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE anomalies SET escalation_level = escalation_level + 1, last_escalated_at = $1, updated_at = $1 WHERE anomaly_id = $2",
        )
        .bind(now)
        .bind(anomaly_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl<'tx> AnomalyTransactionalRepository<Transaction<'tx, Postgres>> for PgAnomalyRepository {
    async fn acknowledge_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        anomaly_id: &str,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE anomalies SET status = 'acknowledged', updated_at = $1 WHERE anomaly_id = $2 AND status = 'open'",
        )
        .bind(Utc::now())
        .bind(anomaly_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn escalate_in_tx(&self, tx: &mut Transaction<'tx, Postgres>, anomaly_id: &str) -> Result<bool, DomainError> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE anomalies SET escalation_level = escalation_level + 1, last_escalated_at = $1, updated_at = $1 WHERE anomaly_id = $2",
        )
        .bind(now)
        .bind(anomaly_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn resolve_in_tx(&self, tx: &mut Transaction<'tx, Postgres>, anomaly_id: &str) -> Result<bool, DomainError> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE anomalies SET status = 'resolved', resolved_at = $1, updated_at = $1 WHERE anomaly_id = $2 AND status <> 'resolved'",
        )
        .bind(now)
        .bind(anomaly_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// Row mapping
// ---------------------------------------------------------------------------

fn parse_anomaly_type(s: &str) -> AnomalyType {
    match s {
        "gate_stand_conflict" => AnomalyType::GateStandConflict,
        "kpi_degradation" => AnomalyType::KpiDegradation,
        "ai_risk" => AnomalyType::AiRisk,
        "dispatch_issue" => AnomalyType::DispatchIssue,
        _ => AnomalyType::ServiceNodeTimeout,
    }
}

fn parse_severity(s: &str) -> AnomalySeverity {
    match s {
        "medium" => AnomalySeverity::Medium,
        "high" => AnomalySeverity::High,
        "critical" => AnomalySeverity::Critical,
        _ => AnomalySeverity::Low,
    }
}

fn parse_status(s: &str) -> AnomalyStatus {
    match s {
        "acknowledged" => AnomalyStatus::Acknowledged,
        "resolved" => AnomalyStatus::Resolved,
        _ => AnomalyStatus::Open,
    }
}

fn row_to_rule(r: &sqlx::postgres::PgRow) -> AnomalyRule {
    let config = r
        .get::<Option<serde_json::Value>, _>("config")
        .unwrap_or_else(|| serde_json::json!({}));
    let escalation_intervals = r
        .get::<Option<serde_json::Value>, _>("escalation_intervals")
        .and_then(|value| serde_json::from_value::<Vec<i64>>(value).ok())
        .unwrap_or_else(|| vec![5, 15, 30]);

    AnomalyRule {
        rule_id: r.get("rule_id"),
        rule_type: r.get("rule_type"),
        name: r.get("name"),
        enabled: r.get::<Option<bool>, _>("enabled").unwrap_or(true),
        config: serde_json::from_value(config).unwrap_or_default(),
        severity: r.get("severity"),
        auto_create_todo: r.get::<Option<bool>, _>("auto_create_todo").unwrap_or(true),
        todo_priority: normalize_todo_priority_value(r.get::<Option<String>, _>("todo_priority").as_deref()),
        escalation_intervals,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn normalize_todo_priority_value(value: Option<&str>) -> String {
    let normalized = value.unwrap_or_default().trim();
    if normalized.is_empty() {
        return "HIGH".to_string();
    }
    normalized.to_string()
}

fn row_to_anomaly(r: &sqlx::postgres::PgRow) -> Anomaly {
    let ctx: serde_json::Value = r
        .get::<Option<serde_json::Value>, _>("context_data")
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let context_data: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(ctx).unwrap_or_default();

    Anomaly {
        anomaly_id: r.get("anomaly_id"),
        flight_id: r.get("flight_id"),
        anomaly_type: parse_anomaly_type(r.get::<String, _>("anomaly_type").as_str()),
        severity: parse_severity(r.get::<String, _>("severity").as_str()),
        title: r.get("title"),
        description: r.get("description"),
        status: parse_status(r.get::<String, _>("status").as_str()),
        detected_at: r.get("detected_at"),
        resolved_at: r.get("resolved_at"),
        escalation_level: r.get::<Option<i32>, _>("escalation_level").unwrap_or(0),
        last_escalated_at: r.get("last_escalated_at"),
        linked_todo_id: r.get("linked_todo_id"),
        rule_id: r.get("rule_id"),
        context_data,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}
