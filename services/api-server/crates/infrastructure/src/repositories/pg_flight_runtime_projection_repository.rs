use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fms_domain::error::DomainError;
use fms_domain::models::business_case::{
    BusinessCaseWorkflowReceiptItem, BusinessCaseWorkflowReceiptProjection, BusinessCaseWorkflowReceiptSummary,
};
use fms_domain::ports::flight_runtime_projection_repository::{
    FlightRuntimeProjection, FlightRuntimeProjectionRepository,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

pub struct PgFlightRuntimeProjectionRepository {
    pool: PgPool,
    hot_cache: RwLock<HashMap<String, FlightRuntimeProjection>>,
}

impl PgFlightRuntimeProjectionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            hot_cache: RwLock::new(HashMap::new()),
        }
    }

    pub async fn find_by_flight_ids(
        &self,
        flight_ids: &[String],
    ) -> Result<HashMap<String, FlightRuntimeProjection>, DomainError> {
        let normalized_ids = normalize_flight_ids(flight_ids);
        if normalized_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut result = HashMap::new();
        let mut missing_ids = Vec::new();
        {
            let cache = self.hot_cache.read().unwrap_or_else(|poisoned| poisoned.into_inner());
            for flight_id in &normalized_ids {
                if let Some(projection) = cache.get(flight_id) {
                    result.insert(flight_id.clone(), projection.clone());
                } else {
                    missing_ids.push(flight_id.clone());
                }
            }
        }

        if missing_ids.is_empty() {
            return Ok(result);
        }

        let rows = sqlx::query(
            r#"
            SELECT flight_id, timeline_snapshot, business_cases
            FROM flight_runtime_list_projection
            WHERE flight_id = ANY($1)
            "#,
        )
        .bind(&missing_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        let loaded = rows.iter().map(row_to_projection).collect::<Vec<_>>();
        {
            let mut cache = self.hot_cache.write().unwrap_or_else(|poisoned| poisoned.into_inner());
            for projection in loaded {
                result.insert(projection.flight_id.clone(), projection.clone());
                cache.insert(projection.flight_id.clone(), projection);
            }
        }

        Ok(result)
    }

    pub async fn rebuild_for_flight(&self, flight_id: &str) -> Result<(), DomainError> {
        let flight_id = flight_id.trim();
        if flight_id.is_empty() {
            return Ok(());
        }

        let projection = self.build_projection(flight_id).await?;

        sqlx::query(
            r#"
            INSERT INTO flight_runtime_list_projection (
                flight_id, timeline_snapshot, business_cases, updated_at
            )
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (flight_id)
            DO UPDATE SET
                timeline_snapshot = EXCLUDED.timeline_snapshot,
                business_cases = EXCLUDED.business_cases,
                updated_at = NOW()
            "#,
        )
        .bind(flight_id)
        .bind(timeline_snapshot_to_json(&projection.timeline_snapshot))
        .bind(Value::Array(projection.business_cases.clone()))
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        self.hot_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(flight_id.to_string(), projection);

        Ok(())
    }

    async fn build_projection(&self, flight_id: &str) -> Result<FlightRuntimeProjection, DomainError> {
        Ok(FlightRuntimeProjection {
            flight_id: flight_id.to_string(),
            timeline_snapshot: load_timeline_snapshot(&self.pool, flight_id).await?,
            business_cases: load_business_cases(&self.pool, flight_id).await?,
        })
    }

    pub async fn delete_for_flight(&self, flight_id: &str) -> Result<(), DomainError> {
        let flight_id = flight_id.trim();
        if flight_id.is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM flight_runtime_list_projection WHERE flight_id = $1")
            .bind(flight_id)
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        self.hot_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(flight_id);

        Ok(())
    }

    pub async fn invalidate_flight(&self, flight_id: &str) {
        let flight_id = flight_id.trim();
        if flight_id.is_empty() {
            return;
        }
        self.hot_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(flight_id);
    }

    pub async fn rebuild_recent(&self, limit: i64) -> Result<usize, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT flight_id
            FROM flights
            WHERE flight_id IS NOT NULL
            ORDER BY COALESCE(scheduled_departure, scheduled_arrival) DESC
            LIMIT $1
            "#,
        )
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        let mut rebuilt = 0usize;
        for row in rows {
            let flight_id = row.get::<String, _>("flight_id");
            self.rebuild_for_flight(&flight_id).await?;
            rebuilt += 1;
        }
        Ok(rebuilt)
    }
}

#[async_trait]
impl FlightRuntimeProjectionRepository for PgFlightRuntimeProjectionRepository {
    async fn find_by_flight_ids(
        &self,
        flight_ids: &[String],
    ) -> Result<HashMap<String, FlightRuntimeProjection>, DomainError> {
        PgFlightRuntimeProjectionRepository::find_by_flight_ids(self, flight_ids).await
    }

    async fn rebuild_for_flight(&self, flight_id: &str) -> Result<(), DomainError> {
        PgFlightRuntimeProjectionRepository::rebuild_for_flight(self, flight_id).await
    }

    async fn delete_for_flight(&self, flight_id: &str) -> Result<(), DomainError> {
        PgFlightRuntimeProjectionRepository::delete_for_flight(self, flight_id).await
    }

    async fn invalidate_flight(&self, flight_id: &str) {
        PgFlightRuntimeProjectionRepository::invalidate_flight(self, flight_id).await;
    }

    async fn rebuild_recent(&self, limit: i64) -> Result<usize, DomainError> {
        PgFlightRuntimeProjectionRepository::rebuild_recent(self, limit).await
    }
}

fn normalize_flight_ids(flight_ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::with_capacity(flight_ids.len());
    let mut acc = Vec::with_capacity(flight_ids.len());
    for item in flight_ids {
        let item = item.trim();
        if item.is_empty() || !seen.insert(item) {
            continue;
        }
        acc.push(item.to_string());
    }
    acc
}

fn row_to_projection(row: &sqlx::postgres::PgRow) -> FlightRuntimeProjection {
    let flight_id = row.get::<String, _>("flight_id");
    let timeline_value = row
        .try_get::<Value, _>("timeline_snapshot")
        .unwrap_or_else(|_| json!({}));
    let business_cases_value = row.try_get::<Value, _>("business_cases").unwrap_or_else(|_| json!([]));

    FlightRuntimeProjection {
        flight_id,
        timeline_snapshot: timeline_json_to_snapshot(timeline_value),
        business_cases: match business_cases_value {
            Value::Array(items) => items,
            _ => Vec::new(),
        },
    }
}

fn timeline_json_to_snapshot(value: Value) -> HashMap<String, DateTime<Utc>> {
    match value {
        Value::Object(map) => map
            .into_iter()
            .filter_map(|(key, value)| {
                value
                    .as_str()
                    .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                    .map(|timestamp| (key, timestamp.with_timezone(&Utc)))
            })
            .collect(),
        _ => HashMap::new(),
    }
}

fn timeline_snapshot_to_json(snapshot: &HashMap<String, DateTime<Utc>>) -> Value {
    Value::Object(
        snapshot
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.to_rfc3339())))
            .collect(),
    )
}

async fn load_timeline_snapshot(pool: &PgPool, flight_id: &str) -> Result<HashMap<String, DateTime<Utc>>, DomainError> {
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT ON (milestone_code)
            milestone_code, occurred_at
        FROM flight_dispatch_timeline_events
        WHERE flight_id = $1
        ORDER BY milestone_code, created_at DESC, timeline_id DESC
        "#,
    )
    .bind(flight_id)
    .fetch_all(pool)
    .await
    .map_err(|error| DomainError::Internal(error.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("milestone_code"),
                row.get::<DateTime<Utc>, _>("occurred_at"),
            )
        })
        .collect())
}

async fn load_business_cases(pool: &PgPool, flight_id: &str) -> Result<Vec<Value>, DomainError> {
    let rows = sqlx::query(
        r#"
        WITH case_rows AS (
            SELECT
                c.*,
                COALESCE(f.flight_number, '') AS flight_no,
                bct.name AS case_type_name,
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
            LEFT JOIN business_case_types bct ON bct.code = c.case_type
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
                            '未知账号'
                        ),
                        'recipient_display_name', n.recipient_display_name_snapshot,
                        'recipient_department', n.recipient_department_snapshot,
                        'recipient_job_title', n.recipient_job_title_snapshot,
                        'ack_status', n.ack_status,
                        'ack_at', n.ack_at,
                        'ack_note', n.ack_note,
                        'updated_at', COALESCE(n.ack_at, n.read_at, n.delivered_at, n.created_at)
                    )
                    ORDER BY n.created_at ASC, n.notification_id ASC
                ) AS items_json
                FROM notifications n
                WHERE n.receipt_group_id = wr.receipt_group_id
            ) wr_items ON true
            WHERE c.flight_id = $1
            ORDER BY c.created_at DESC
        ),
        append_summary AS (
            SELECT
                a.case_id,
                COUNT(*)::int AS append_count,
                (
                    SELECT jsonb_build_object(
                        'append_id', latest.append_id,
                        'case_id', latest.case_id,
                        'content', latest.content,
                        'client_action_id', latest.client_action_id,
                        'submitted_by', latest.submitted_by,
                        'submitted_operator_name', latest.submitted_operator_name,
                        'appended_at', latest.appended_at,
                        'metadata', COALESCE(latest.metadata, 'null'::jsonb)
                    )
                    FROM flight_business_case_appends latest
                    WHERE latest.case_id = a.case_id
                    ORDER BY latest.appended_at DESC, latest.append_id DESC
                    LIMIT 1
                ) AS latest_append
            FROM flight_business_case_appends a
            WHERE a.case_id IN (SELECT case_id FROM case_rows)
            GROUP BY a.case_id
        )
        SELECT
            cr.*,
            COALESCE(aps.append_count, 0) AS append_count,
            aps.latest_append
        FROM case_rows cr
        LEFT JOIN append_summary aps ON aps.case_id = cr.case_id
        ORDER BY cr.created_at DESC
        "#,
    )
    .bind(flight_id)
    .fetch_all(pool)
    .await
    .map_err(|error| DomainError::Internal(error.to_string()))?;

    Ok(rows.iter().map(business_case_row_to_json).collect())
}

fn business_case_row_to_json(row: &sqlx::postgres::PgRow) -> Value {
    let context = row
        .try_get::<Option<Value>, _>("context")
        .ok()
        .flatten()
        .unwrap_or_else(|| json!({}));
    let terminal_metadata = context.get("workflow_terminal").cloned().unwrap_or(Value::Null);
    let log = row
        .try_get::<Option<Vec<String>>, _>("log")
        .ok()
        .flatten()
        .map(|items| {
            Value::Array(
                items
                    .into_iter()
                    .map(|item| serde_json::from_str(&item).unwrap_or(Value::String(item)))
                    .collect(),
            )
        })
        .unwrap_or_else(|| json!([]));
    let department_id = row.try_get::<Option<String>, _>("department_id").ok().flatten();
    let department_name_snapshot = row
        .try_get::<Option<String>, _>("department_name_snapshot")
        .ok()
        .flatten();
    let visibility_scope = resolve_visibility_scope(
        row.try_get::<Option<String>, _>("visibility_scope")
            .ok()
            .flatten()
            .as_deref(),
        department_id.as_deref(),
        department_name_snapshot.as_deref(),
    );
    let workflow_receipt = build_receipt_from_row(row);

    let mut item = json!({
        "case_id": row.get::<String, _>("case_id"),
        "case_type": row.get::<String, _>("case_type"),
        "flight_id": row.get::<String, _>("flight_id"),
        "flight_no": row.try_get::<Option<String>, _>("flight_no").ok().flatten().unwrap_or_default(),
        "created_at": row.get::<DateTime<Utc>, _>("created_at"),
        "created_by": row.try_get::<Option<String>, _>("created_by").ok().flatten().unwrap_or_else(|| "system".to_string()),
        "updated_by": row.try_get::<Option<String>, _>("updated_by").ok().flatten().unwrap_or_else(|| "system".to_string()),
        "description": row.try_get::<Option<String>, _>("description").ok().flatten().unwrap_or_default(),
        "status": row.try_get::<Option<String>, _>("status").ok().flatten().unwrap_or_else(|| "PENDING".to_string()),
        "stand": row.try_get::<Option<String>, _>("stand").ok().flatten(),
        "gate": row.try_get::<Option<String>, _>("gate").ok().flatten(),
        "visibility_scope": visibility_scope,
        "department_id": department_id,
        "department_name_snapshot": department_name_snapshot,
        "finished_at": row.try_get::<Option<DateTime<Utc>>, _>("finished_at").ok().flatten(),
        "cancelled_at": row.try_get::<Option<DateTime<Utc>>, _>("cancelled_at").ok().flatten(),
        "log": log,
        "context": context,
        "workflow_receipt": workflow_receipt,
        "terminal_metadata": terminal_metadata,
        "append_count": row.try_get::<i32, _>("append_count").unwrap_or_default(),
        "latest_append": row.try_get::<Option<Value>, _>("latest_append").ok().flatten(),
        "append_entries": [],
    });

    if let Some(case_type_name) = row
        .try_get::<Option<String>, _>("case_type_name")
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        item["case_type_name"] = Value::String(case_type_name);
    }

    if let Some(latest_append) = item.get_mut("latest_append").and_then(Value::as_object_mut) {
        if latest_append
            .get("client_action_id")
            .map(Value::is_null)
            .unwrap_or(false)
        {
            latest_append.remove("client_action_id");
        }
    }

    item
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

fn resolve_visibility_scope(
    raw: Option<&str>,
    department_id: Option<&str>,
    department_name_snapshot: Option<&str>,
) -> &'static str {
    match raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .as_deref()
    {
        Some("DEPARTMENT") => "DEPARTMENT",
        Some("COMMON") => "COMMON",
        _ => {
            if department_id.map(str::trim).filter(|value| !value.is_empty()).is_some()
                || department_name_snapshot
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_some()
            {
                "DEPARTMENT"
            } else {
                "COMMON"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{timeline_json_to_snapshot, timeline_snapshot_to_json};
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn timeline_snapshot_json_round_trips_rfc3339_values() {
        let occurred_at = Utc.with_ymd_and_hms(2026, 5, 27, 12, 0, 0).unwrap();
        let mut snapshot = HashMap::new();
        snapshot.insert("on_blocks_time".to_string(), occurred_at);

        let value = timeline_snapshot_to_json(&snapshot);
        assert_eq!(value["on_blocks_time"], json!(occurred_at.to_rfc3339()));

        let decoded = timeline_json_to_snapshot(value);
        assert_eq!(decoded.get("on_blocks_time"), Some(&occurred_at));
    }
}
