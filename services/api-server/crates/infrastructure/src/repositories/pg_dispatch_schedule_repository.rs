//! PostgreSQL 派工排班仓储实现。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    DispatchLockLevel, DispatchLockRule, EquipmentDowntime, LeaveRecord, ShiftInstance, ShiftTemplate,
};
use fms_domain::ports::dispatch_repository::{
    ScheduleExceptionRepository, ShiftInstanceRepository, ShiftTemplateRepository,
};

pub struct PgShiftTemplateRepository {
    pool: PgPool,
}

impl PgShiftTemplateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ShiftTemplateRepository for PgShiftTemplateRepository {
    async fn save(&self, template: &ShiftTemplate) -> Result<ShiftTemplate, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO shift_templates (
                id, name, resource_type, resource_id, terminal,
                start_time_local, end_time_local, weekdays,
                max_continuous_minutes, min_rest_minutes, enabled
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                resource_type = EXCLUDED.resource_type,
                resource_id = EXCLUDED.resource_id,
                terminal = EXCLUDED.terminal,
                start_time_local = EXCLUDED.start_time_local,
                end_time_local = EXCLUDED.end_time_local,
                weekdays = EXCLUDED.weekdays,
                max_continuous_minutes = EXCLUDED.max_continuous_minutes,
                min_rest_minutes = EXCLUDED.min_rest_minutes,
                enabled = EXCLUDED.enabled,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, name, resource_type, resource_id, terminal, start_time_local,
                      end_time_local, weekdays, max_continuous_minutes, min_rest_minutes,
                      enabled, created_at, updated_at
            "#,
        )
        .bind(&template.id)
        .bind(&template.name)
        .bind(&template.resource_type)
        .bind(&template.resource_id)
        .bind(&template.terminal)
        .bind(&template.start_time_local)
        .bind(&template.end_time_local)
        .bind(serde_json::to_value(&template.weekdays).unwrap_or_else(|_| serde_json::json!([])))
        .bind(template.max_continuous_minutes)
        .bind(template.min_rest_minutes)
        .bind(template.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_shift_template(&row))
    }

    async fn find_all(
        &self,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        enabled: Option<bool>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShiftTemplate>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, name, resource_type, resource_id, terminal, start_time_local, end_time_local, weekdays, max_continuous_minutes, min_rest_minutes, enabled, created_at, updated_at FROM shift_templates WHERE 1=1",
        );
        if let Some(value) = resource_type {
            builder.push(" AND resource_type = ").push_bind(value);
        }
        if let Some(value) = resource_id {
            builder.push(" AND resource_id = ").push_bind(value);
        }
        if let Some(value) = enabled {
            builder.push(" AND enabled = ").push_bind(value);
        }
        builder
            .push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(limit.max(1))
            .push(" OFFSET ")
            .push_bind(offset.max(0));

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_shift_template).collect())
    }
}

pub struct PgShiftInstanceRepository {
    pool: PgPool,
}

impl PgShiftInstanceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ShiftInstanceRepository for PgShiftInstanceRepository {
    async fn save(&self, instance: &ShiftInstance) -> Result<ShiftInstance, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO shift_instances (
                id, template_id, resource_type, resource_id, terminal,
                start_time, end_time, status, max_continuous_minutes, min_rest_minutes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO UPDATE SET
                template_id = EXCLUDED.template_id,
                resource_type = EXCLUDED.resource_type,
                resource_id = EXCLUDED.resource_id,
                terminal = EXCLUDED.terminal,
                start_time = EXCLUDED.start_time,
                end_time = EXCLUDED.end_time,
                status = EXCLUDED.status,
                max_continuous_minutes = EXCLUDED.max_continuous_minutes,
                min_rest_minutes = EXCLUDED.min_rest_minutes,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, template_id, resource_type, resource_id, terminal, start_time,
                      end_time, status, max_continuous_minutes, min_rest_minutes,
                      created_at, updated_at
            "#,
        )
        .bind(&instance.id)
        .bind(&instance.template_id)
        .bind(&instance.resource_type)
        .bind(&instance.resource_id)
        .bind(&instance.terminal)
        .bind(instance.start_time)
        .bind(instance.end_time)
        .bind(&instance.status)
        .bind(instance.max_continuous_minutes)
        .bind(instance.min_rest_minutes)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_shift_instance(&row))
    }

    async fn find_all(
        &self,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShiftInstance>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, template_id, resource_type, resource_id, terminal, start_time, end_time, status, max_continuous_minutes, min_rest_minutes, created_at, updated_at FROM shift_instances WHERE 1=1",
        );
        if let Some(value) = resource_type {
            builder.push(" AND resource_type = ").push_bind(value);
        }
        if let Some(value) = resource_id {
            builder.push(" AND resource_id = ").push_bind(value);
        }
        if let Some(value) = window_start {
            builder.push(" AND end_time >= ").push_bind(value);
        }
        if let Some(value) = window_end {
            builder.push(" AND start_time <= ").push_bind(value);
        }
        builder
            .push(" ORDER BY start_time ASC LIMIT ")
            .push_bind(limit.max(1))
            .push(" OFFSET ")
            .push_bind(offset.max(0));

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_shift_instance).collect())
    }

    async fn find_for_resource_window(
        &self,
        resource_type: &str,
        resource_id: &str,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<ShiftInstance>, DomainError> {
        self.find_all(
            Some(resource_type),
            Some(resource_id),
            Some(window_start),
            Some(window_end),
            200,
            0,
        )
        .await
    }
}

pub struct PgScheduleExceptionRepository {
    pool: PgPool,
}

impl PgScheduleExceptionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduleExceptionRepository for PgScheduleExceptionRepository {
    async fn save_leave_record(&self, record: &LeaveRecord) -> Result<LeaveRecord, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO schedule_leave_records (
                id, user_id, team_id, start_time, end_time, reason, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                user_id = EXCLUDED.user_id,
                team_id = EXCLUDED.team_id,
                start_time = EXCLUDED.start_time,
                end_time = EXCLUDED.end_time,
                reason = EXCLUDED.reason,
                status = EXCLUDED.status
            RETURNING id, user_id, team_id, start_time, end_time, reason, status, created_at
            "#,
        )
        .bind(&record.id)
        .bind(&record.user_id)
        .bind(&record.team_id)
        .bind(record.start_time)
        .bind(record.end_time)
        .bind(&record.reason)
        .bind(&record.status)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_leave_record(&row))
    }

    async fn find_leave_records(
        &self,
        user_ids: &[String],
        team_id: Option<&str>,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Vec<LeaveRecord>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, user_id, team_id, start_time, end_time, reason, status, created_at FROM schedule_leave_records WHERE 1=1",
        );
        if !user_ids.is_empty() {
            builder.push(" AND user_id IN (");
            let mut separated = builder.separated(", ");
            for user_id in user_ids {
                separated.push_bind(user_id);
            }
            separated.push_unseparated(")");
        }
        if let Some(value) = team_id.filter(|item| !item.is_empty()) {
            builder.push(" AND team_id = ").push_bind(value);
        }
        if let Some(value) = window_start {
            builder.push(" AND end_time >= ").push_bind(value);
        }
        if let Some(value) = window_end {
            builder.push(" AND start_time <= ").push_bind(value);
        }
        builder.push(" ORDER BY start_time ASC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_leave_record).collect())
    }

    async fn save_equipment_downtime(&self, downtime: &EquipmentDowntime) -> Result<EquipmentDowntime, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO equipment_downtimes (
                id, equipment_id, start_time, end_time, reason, status
            ) VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (id) DO UPDATE SET
                equipment_id = EXCLUDED.equipment_id,
                start_time = EXCLUDED.start_time,
                end_time = EXCLUDED.end_time,
                reason = EXCLUDED.reason,
                status = EXCLUDED.status
            RETURNING id, equipment_id, start_time, end_time, reason, status, created_at
            "#,
        )
        .bind(&downtime.id)
        .bind(&downtime.equipment_id)
        .bind(downtime.start_time)
        .bind(downtime.end_time)
        .bind(&downtime.reason)
        .bind(&downtime.status)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_equipment_downtime(&row))
    }

    async fn find_equipment_downtimes(
        &self,
        equipment_ids: &[String],
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Vec<EquipmentDowntime>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, equipment_id, start_time, end_time, reason, status, created_at FROM equipment_downtimes WHERE 1=1",
        );
        if !equipment_ids.is_empty() {
            builder.push(" AND equipment_id IN (");
            let mut separated = builder.separated(", ");
            for equipment_id in equipment_ids {
                separated.push_bind(equipment_id);
            }
            separated.push_unseparated(")");
        }
        if let Some(value) = window_start {
            builder.push(" AND end_time >= ").push_bind(value);
        }
        if let Some(value) = window_end {
            builder.push(" AND start_time <= ").push_bind(value);
        }
        builder.push(" ORDER BY start_time ASC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_equipment_downtime).collect())
    }

    async fn save_lock_rule(&self, rule: &DispatchLockRule) -> Result<DispatchLockRule, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO dispatch_lock_rules (
                id, dispatch_order_id, flight_id, team_id, lock_level, start_time, end_time, reason
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                dispatch_order_id = EXCLUDED.dispatch_order_id,
                flight_id = EXCLUDED.flight_id,
                team_id = EXCLUDED.team_id,
                lock_level = EXCLUDED.lock_level,
                start_time = EXCLUDED.start_time,
                end_time = EXCLUDED.end_time,
                reason = EXCLUDED.reason
            RETURNING id, dispatch_order_id, flight_id, team_id, lock_level, start_time, end_time, reason, created_at
            "#,
        )
        .bind(&rule.id)
        .bind(&rule.dispatch_order_id)
        .bind(&rule.flight_id)
        .bind(&rule.team_id)
        .bind(dispatch_lock_level_value(rule.lock_level))
        .bind(rule.start_time)
        .bind(rule.end_time)
        .bind(&rule.reason)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_lock_rule(&row))
    }

    async fn find_lock_rules(
        &self,
        dispatch_order_ids: &[String],
        team_id: Option<&str>,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Vec<DispatchLockRule>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, dispatch_order_id, flight_id, team_id, lock_level, start_time, end_time, reason, created_at FROM dispatch_lock_rules WHERE 1=1",
        );
        if !dispatch_order_ids.is_empty() {
            builder.push(" AND dispatch_order_id IN (");
            let mut separated = builder.separated(", ");
            for dispatch_order_id in dispatch_order_ids {
                separated.push_bind(dispatch_order_id);
            }
            separated.push_unseparated(")");
        }
        if let Some(value) = team_id {
            builder.push(" AND team_id = ").push_bind(value);
        }
        if let Some(value) = window_start {
            builder.push(" AND end_time >= ").push_bind(value);
        }
        if let Some(value) = window_end {
            builder.push(" AND start_time <= ").push_bind(value);
        }
        builder.push(" ORDER BY start_time ASC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_lock_rule).collect())
    }

    async fn list_exceptions(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT * FROM (
                SELECT id, 'leave' AS exception_type, user_id AS resource_id, team_id,
                       NULL::VARCHAR(26) AS dispatch_order_id, status, start_time, end_time, reason, created_at
                FROM schedule_leave_records
                UNION ALL
                SELECT id, 'equipment_downtime' AS exception_type, equipment_id AS resource_id,
                       NULL::VARCHAR(26) AS team_id, NULL::VARCHAR(26) AS dispatch_order_id,
                       status, start_time, end_time, reason, created_at
                FROM equipment_downtimes
                UNION ALL
                SELECT id, 'dispatch_lock' AS exception_type,
                       COALESCE(team_id, flight_id, dispatch_order_id) AS resource_id,
                       team_id, dispatch_order_id, lock_level AS status, start_time, end_time, reason, created_at
                FROM dispatch_lock_rules
            ) schedule_exceptions
            WHERE 1=1
            "#,
        );
        if let Some(value) = window_start {
            builder.push(" AND end_time >= ").push_bind(value);
        }
        if let Some(value) = window_end {
            builder.push(" AND start_time <= ").push_bind(value);
        }
        builder.push(" ORDER BY start_time ASC LIMIT ").push_bind(limit.max(1));

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "exception_type": row.get::<String, _>("exception_type"),
                    "resource_id": row.get::<Option<String>, _>("resource_id"),
                    "team_id": row.get::<Option<String>, _>("team_id"),
                    "dispatch_order_id": row.get::<Option<String>, _>("dispatch_order_id"),
                    "status": row.get::<String, _>("status"),
                    "start_time": row.get::<DateTime<Utc>, _>("start_time"),
                    "end_time": row.get::<DateTime<Utc>, _>("end_time"),
                    "reason": row.get::<Option<String>, _>("reason"),
                })
            })
            .collect())
    }
}

fn row_to_shift_template(row: &sqlx::postgres::PgRow) -> ShiftTemplate {
    ShiftTemplate {
        id: row.get("id"),
        name: row.get("name"),
        resource_type: row.get("resource_type"),
        resource_id: row.get("resource_id"),
        terminal: row.get("terminal"),
        start_time_local: row.get("start_time_local"),
        end_time_local: row.get("end_time_local"),
        weekdays: row
            .get::<Option<serde_json::Value>, _>("weekdays")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| item.as_i64().map(|value| value as i32))
            .collect(),
        max_continuous_minutes: row.get("max_continuous_minutes"),
        min_rest_minutes: row.get("min_rest_minutes"),
        enabled: row.get::<Option<bool>, _>("enabled").unwrap_or(true),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_shift_instance(row: &sqlx::postgres::PgRow) -> ShiftInstance {
    ShiftInstance {
        id: row.get("id"),
        template_id: row.get("template_id"),
        resource_type: row.get("resource_type"),
        resource_id: row.get("resource_id"),
        terminal: row.get("terminal"),
        start_time: row.get("start_time"),
        end_time: row.get("end_time"),
        status: row
            .get::<Option<String>, _>("status")
            .unwrap_or_else(|| "scheduled".to_string()),
        max_continuous_minutes: row.get("max_continuous_minutes"),
        min_rest_minutes: row.get("min_rest_minutes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_leave_record(row: &sqlx::postgres::PgRow) -> LeaveRecord {
    LeaveRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        team_id: row.get("team_id"),
        start_time: row.get("start_time"),
        end_time: row.get("end_time"),
        reason: row.get("reason"),
        status: row
            .get::<Option<String>, _>("status")
            .unwrap_or_else(|| "approved".to_string()),
        created_at: row.get("created_at"),
    }
}

fn row_to_equipment_downtime(row: &sqlx::postgres::PgRow) -> EquipmentDowntime {
    EquipmentDowntime {
        id: row.get("id"),
        equipment_id: row.get("equipment_id"),
        start_time: row.get("start_time"),
        end_time: row.get("end_time"),
        reason: row.get("reason"),
        status: row
            .get::<Option<String>, _>("status")
            .unwrap_or_else(|| "scheduled".to_string()),
        created_at: row.get("created_at"),
    }
}

fn row_to_lock_rule(row: &sqlx::postgres::PgRow) -> DispatchLockRule {
    DispatchLockRule {
        id: row.get("id"),
        dispatch_order_id: row.get("dispatch_order_id"),
        flight_id: row.get("flight_id"),
        team_id: row.get("team_id"),
        lock_level: parse_dispatch_lock_level(row.get::<Option<String>, _>("lock_level").as_deref()),
        start_time: row.get("start_time"),
        end_time: row.get("end_time"),
        reason: row.get("reason"),
        created_at: row.get("created_at"),
    }
}

fn parse_dispatch_lock_level(value: Option<&str>) -> DispatchLockLevel {
    match value.unwrap_or("optimizable") {
        "active" => DispatchLockLevel::Active,
        "frozen" => DispatchLockLevel::Frozen,
        "manual_lock" => DispatchLockLevel::ManualLock,
        _ => DispatchLockLevel::Optimizable,
    }
}

fn dispatch_lock_level_value(level: DispatchLockLevel) -> &'static str {
    match level {
        DispatchLockLevel::Active => "active",
        DispatchLockLevel::Frozen => "frozen",
        DispatchLockLevel::ManualLock => "manual_lock",
        DispatchLockLevel::Optimizable => "optimizable",
    }
}
