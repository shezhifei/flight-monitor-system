//! PostgreSQL 事件规则仓储实现

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::ports::event_rule_repository::{
    AdjustmentActionType, AdjustmentRuleRecord, DispatchOrderAdjustmentRuleCreate, DispatchOrderAdjustmentRuleUpdate,
    EventDrivenGenerationRuleCreate, EventDrivenGenerationRuleUpdate, EventRuleRepository, GenerationRuleRecord,
    ListAdjustmentRulesParams, ListGenerationRulesParams,
};

pub struct PgEventRuleRepository {
    pool: PgPool,
}

impl PgEventRuleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn new_id() -> String {
        ulid::Ulid::new().to_string()
    }

    fn parse_adjustment_rule(row: PgRow) -> Result<AdjustmentRuleRecord, sqlx::Error> {
        let adjuster_type: String = row.get("adjuster_type");
        let event_patterns: serde_json::Value = row.get("event_patterns");
        let conditions: Option<serde_json::Value> = row.get("conditions");
        let config: serde_json::Value = row.get("config");

        Ok(AdjustmentRuleRecord {
            id: row.get("id"),
            adjuster_type,
            name: row.get("name"),
            description: row.get("description"),
            event_patterns: event_patterns
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            priority: row.get("priority"),
            conditions,
            config,
            is_enabled: row.get("is_enabled"),
            department_id: row.get("department_id"),
            department_name: row.get("department_name"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            created_by: row.get("created_by"),
        })
    }

    fn parse_generation_rule(row: PgRow) -> Result<GenerationRuleRecord, sqlx::Error> {
        let generator_type: String = row.get("generator_type");
        let event_patterns: serde_json::Value = row.get("event_patterns");
        let conditions: Option<serde_json::Value> = row.get("conditions");
        let config: serde_json::Value = row.get("config");

        Ok(GenerationRuleRecord {
            id: row.get("id"),
            generator_type,
            name: row.get("name"),
            description: row.get("description"),
            event_patterns: event_patterns
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            priority: row.get("priority"),
            conditions,
            config,
            is_enabled: row.get("is_enabled"),
            department_id: row.get("department_id"),
            department_name: row.get("department_name"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            created_by: row.get("created_by"),
        })
    }
}

#[async_trait]
impl EventRuleRepository for PgEventRuleRepository {
    async fn list_adjustment_rules(
        &self,
        params: &ListAdjustmentRulesParams,
    ) -> Result<Vec<AdjustmentRuleRecord>, DomainError> {
        let mut query = String::from(
            r#"
            SELECT r.id, r.adjuster_type, r.name, r.description, r.event_patterns,
                   r.priority, r.conditions, r.config, r.is_enabled,
                   r.department_id, d.name as department_name,
                   r.created_at, r.updated_at, r.created_by
            FROM dispatch_order_adjustment_rules r
            LEFT JOIN departments d ON r.department_id = d.id
            WHERE 1=1
            "#,
        );

        let mut conditions = Vec::new();
        let mut param_idx = 1;

        if params.is_enabled.is_some() {
            conditions.push(format!(" AND r.is_enabled = ${}", param_idx));
            param_idx += 1;
        }

        if params.department_id.is_some() {
            conditions.push(format!(" AND r.department_id = ${}", param_idx));
        }

        query.push_str(&conditions.join(""));
        query.push_str(" ORDER BY r.priority ASC, r.created_at DESC");

        if let (Some(page), Some(page_size)) = (params.page, params.page_size) {
            let offset = (page - 1) * page_size;
            query.push_str(&format!(" LIMIT {} OFFSET {}", page_size, offset));
        }

        let mut q = sqlx::query(&query);

        if let Some(enabled) = params.is_enabled {
            q = q.bind(enabled);
        }

        if let Some(ref department_id) = params.department_id {
            q = q.bind(department_id);
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        rows.into_iter()
            .map(Self::parse_adjustment_rule)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DomainError::Internal(e.to_string()))
    }

    async fn count_adjustment_rules(&self, params: &ListAdjustmentRulesParams) -> Result<i64, DomainError> {
        let mut query = String::from("SELECT COUNT(*) FROM dispatch_order_adjustment_rules WHERE 1=1");

        let q;
        if let Some(enabled) = params.is_enabled {
            query.push_str(" AND is_enabled = $1");
            q = sqlx::query_scalar::<_, i64>(&query).bind(enabled);
        } else if let Some(ref department_id) = params.department_id {
            query.push_str(" AND department_id = $1");
            q = sqlx::query_scalar::<_, i64>(&query).bind(department_id);
        } else {
            q = sqlx::query_scalar::<_, i64>(&query);
        }

        q.fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))
    }

    async fn get_adjustment_rule(&self, id: &str) -> Result<Option<AdjustmentRuleRecord>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT r.id, r.adjuster_type, r.name, r.description, r.event_patterns,
                   r.priority, r.conditions, r.config, r.is_enabled,
                   r.department_id, d.name as department_name,
                   r.created_at, r.updated_at, r.created_by
            FROM dispatch_order_adjustment_rules r
            LEFT JOIN departments d ON r.department_id = d.id
            WHERE r.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        match row {
            Some(r) => Self::parse_adjustment_rule(r)
                .map(Some)
                .map_err(|e| DomainError::Internal(e.to_string())),
            None => Ok(None),
        }
    }

    async fn create_adjustment_rule(
        &self,
        payload: DispatchOrderAdjustmentRuleCreate,
        created_by: Option<&str>,
    ) -> Result<AdjustmentRuleRecord, DomainError> {
        let id = Self::new_id();
        let now = Utc::now();
        let event_patterns = serde_json::json!(payload.event_patterns);
        let conditions = payload.conditions.and_then(|c| serde_json::to_value(c).ok());
        let adjuster_type_str = match payload.adjuster_type {
            AdjustmentActionType::AddCrewSlot => "add_crew_slot",
            AdjustmentActionType::IncreaseCrewCount => "increase_crew_count",
            AdjustmentActionType::UpgradeCrewLevel => "upgrade_crew_level",
            AdjustmentActionType::AddEquipmentSlot => "add_equipment_slot",
            AdjustmentActionType::IncreaseEquipmentCount => "increase_equipment_count",
            AdjustmentActionType::ExtendDuration => "extend_duration",
            AdjustmentActionType::ShortenDuration => "shorten_duration",
            AdjustmentActionType::AdvancePublish => "advance_publish",
            AdjustmentActionType::DelayPublish => "delay_publish",
            AdjustmentActionType::RequireDriverForEquipment => "require_driver_for_equipment",
        };

        let row = sqlx::query(
            r#"
            INSERT INTO dispatch_order_adjustment_rules (
                id, adjuster_type, name, description, event_patterns,
                priority, conditions, config, is_enabled, department_id,
                created_at, updated_at, created_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING id
            "#,
        )
        .bind(&id)
        .bind(adjuster_type_str)
        .bind(&payload.name)
        .bind(&payload.description)
        .bind(&event_patterns)
        .bind(payload.priority)
        .bind(&conditions)
        .bind(&payload.config)
        .bind(payload.is_enabled)
        .bind(&payload.department_id)
        .bind(now)
        .bind(now)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let created_id: String = row.get("id");
        self.get_adjustment_rule(&created_id)
            .await?
            .ok_or_else(|| DomainError::Internal("Failed to retrieve created rule".to_string()))
    }

    async fn update_adjustment_rule(
        &self,
        id: &str,
        payload: DispatchOrderAdjustmentRuleUpdate,
    ) -> Result<AdjustmentRuleRecord, DomainError> {
        let now = Utc::now();
        let mut updates = Vec::new();
        let mut param_idx = 1;

        if payload.name.is_some() {
            updates.push(format!("name = ${}", param_idx));
            param_idx += 1;
        }
        if payload.description.is_some() {
            updates.push(format!("description = ${}", param_idx));
            param_idx += 1;
        }
        if payload.event_patterns.is_some() {
            updates.push(format!("event_patterns = ${}", param_idx));
            param_idx += 1;
        }
        if payload.priority.is_some() {
            updates.push(format!("priority = ${}", param_idx));
            param_idx += 1;
        }
        if payload.conditions.is_some() {
            updates.push(format!("conditions = ${}", param_idx));
            param_idx += 1;
        }
        if payload.config.is_some() {
            updates.push(format!("config = ${}", param_idx));
            param_idx += 1;
        }
        if payload.is_enabled.is_some() {
            updates.push(format!("is_enabled = ${}", param_idx));
            param_idx += 1;
        }
        if payload.department_id.is_some() {
            updates.push(format!("department_id = ${}", param_idx));
            param_idx += 1;
        }

        if updates.is_empty() {
            return self
                .get_adjustment_rule(id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "adjustment_rule",
                    id: id.to_string(),
                });
        }

        updates.push(format!("updated_at = ${}", param_idx));
        param_idx += 1;

        let query = format!(
            "UPDATE dispatch_order_adjustment_rules SET {} WHERE id = ${}",
            updates.join(", "),
            param_idx
        );

        let mut q = sqlx::query(&query);

        if let Some(ref name) = payload.name {
            q = q.bind(name);
        }
        if let Some(ref desc) = payload.description {
            q = q.bind(desc);
        }
        if let Some(ref patterns) = payload.event_patterns {
            q = q.bind(serde_json::json!(patterns));
        }
        if let Some(priority) = payload.priority {
            q = q.bind(priority);
        }
        if let Some(ref conditions) = payload.conditions {
            let c = serde_json::to_value(conditions).ok();
            q = q.bind(c);
        } else {
            q = q.bind(serde_json::Value::Null);
        }
        if let Some(ref config) = payload.config {
            q = q.bind(config);
        }
        if let Some(enabled) = payload.is_enabled {
            q = q.bind(enabled);
        }
        if let Some(ref dept_id) = payload.department_id {
            q = q.bind(dept_id);
        }
        q = q.bind(now);
        q = q.bind(id);

        q.execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        self.get_adjustment_rule(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "adjustment_rule",
                id: id.to_string(),
            })
    }

    async fn delete_adjustment_rule(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM dispatch_order_adjustment_rules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn set_adjustment_rule_enabled(&self, id: &str, enabled: bool) -> Result<AdjustmentRuleRecord, DomainError> {
        let now = Utc::now();
        sqlx::query("UPDATE dispatch_order_adjustment_rules SET is_enabled = $1, updated_at = $2 WHERE id = $3")
            .bind(enabled)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        self.get_adjustment_rule(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "adjustment_rule",
                id: id.to_string(),
            })
    }

    async fn list_generation_rules(
        &self,
        params: &ListGenerationRulesParams,
    ) -> Result<Vec<GenerationRuleRecord>, DomainError> {
        let mut query = String::from(
            r#"
            SELECT r.id, r.generator_type, r.name, r.description, r.event_patterns,
                   r.priority, r.conditions, r.config, r.is_enabled,
                   r.department_id, d.name as department_name,
                   r.created_at, r.updated_at, r.created_by
            FROM event_driven_dispatch_generation_rules r
            LEFT JOIN departments d ON r.department_id = d.id
            WHERE 1=1
            "#,
        );

        let mut conditions = Vec::new();
        let mut param_idx = 1;

        if params.is_enabled.is_some() {
            conditions.push(format!(" AND r.is_enabled = ${}", param_idx));
            param_idx += 1;
        }

        if params.department_id.is_some() {
            conditions.push(format!(" AND r.department_id = ${}", param_idx));
        }

        query.push_str(&conditions.join(""));
        query.push_str(" ORDER BY r.priority ASC, r.created_at DESC");

        if let (Some(page), Some(page_size)) = (params.page, params.page_size) {
            let offset = (page - 1) * page_size;
            query.push_str(&format!(" LIMIT {} OFFSET {}", page_size, offset));
        }

        let mut q = sqlx::query(&query);

        if let Some(enabled) = params.is_enabled {
            q = q.bind(enabled);
        }

        if let Some(ref department_id) = params.department_id {
            q = q.bind(department_id);
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        rows.into_iter()
            .map(Self::parse_generation_rule)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DomainError::Internal(e.to_string()))
    }

    async fn count_generation_rules(&self, params: &ListGenerationRulesParams) -> Result<i64, DomainError> {
        let mut query = String::from("SELECT COUNT(*) FROM event_driven_dispatch_generation_rules WHERE 1=1");

        let q;
        if let Some(enabled) = params.is_enabled {
            query.push_str(" AND is_enabled = $1");
            q = sqlx::query_scalar::<_, i64>(&query).bind(enabled);
        } else if let Some(ref department_id) = params.department_id {
            query.push_str(" AND department_id = $1");
            q = sqlx::query_scalar::<_, i64>(&query).bind(department_id);
        } else {
            q = sqlx::query_scalar::<_, i64>(&query);
        }

        q.fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))
    }

    async fn get_generation_rule(&self, id: &str) -> Result<Option<GenerationRuleRecord>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT r.id, r.generator_type, r.name, r.description, r.event_patterns,
                   r.priority, r.conditions, r.config, r.is_enabled,
                   r.department_id, d.name as department_name,
                   r.created_at, r.updated_at, r.created_by
            FROM event_driven_dispatch_generation_rules r
            LEFT JOIN departments d ON r.department_id = d.id
            WHERE r.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        match row {
            Some(r) => Self::parse_generation_rule(r)
                .map(Some)
                .map_err(|e| DomainError::Internal(e.to_string())),
            None => Ok(None),
        }
    }

    async fn create_generation_rule(
        &self,
        payload: EventDrivenGenerationRuleCreate,
        created_by: Option<&str>,
    ) -> Result<GenerationRuleRecord, DomainError> {
        let id = Self::new_id();
        let now = Utc::now();
        let event_patterns = serde_json::json!(payload.event_patterns);
        let conditions = payload.conditions.and_then(|c| serde_json::to_value(c).ok());
        let config = serde_json::to_value(&payload.config).map_err(|e| DomainError::Internal(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO event_driven_dispatch_generation_rules (
                id, generator_type, name, description, event_patterns,
                priority, conditions, config, is_enabled, department_id,
                created_at, updated_at, created_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(&id)
        .bind(&payload.generator_type)
        .bind(&payload.name)
        .bind(&payload.description)
        .bind(&event_patterns)
        .bind(payload.priority)
        .bind(&conditions)
        .bind(&config)
        .bind(payload.is_enabled)
        .bind(&payload.department_id)
        .bind(now)
        .bind(now)
        .bind(created_by)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        self.get_generation_rule(&id)
            .await?
            .ok_or_else(|| DomainError::Internal("Failed to retrieve created rule".to_string()))
    }

    async fn update_generation_rule(
        &self,
        id: &str,
        payload: EventDrivenGenerationRuleUpdate,
    ) -> Result<GenerationRuleRecord, DomainError> {
        let now = Utc::now();
        let mut updates = Vec::new();
        let mut param_idx = 1;

        if payload.name.is_some() {
            updates.push(format!("name = ${}", param_idx));
            param_idx += 1;
        }
        if payload.description.is_some() {
            updates.push(format!("description = ${}", param_idx));
            param_idx += 1;
        }
        if payload.event_patterns.is_some() {
            updates.push(format!("event_patterns = ${}", param_idx));
            param_idx += 1;
        }
        if payload.priority.is_some() {
            updates.push(format!("priority = ${}", param_idx));
            param_idx += 1;
        }
        if payload.conditions.is_some() {
            updates.push(format!("conditions = ${}", param_idx));
            param_idx += 1;
        }
        if payload.config.is_some() {
            updates.push(format!("config = ${}", param_idx));
            param_idx += 1;
        }
        if payload.is_enabled.is_some() {
            updates.push(format!("is_enabled = ${}", param_idx));
            param_idx += 1;
        }
        if payload.department_id.is_some() {
            updates.push(format!("department_id = ${}", param_idx));
            param_idx += 1;
        }

        if updates.is_empty() {
            return self
                .get_generation_rule(id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "generation_rule",
                    id: id.to_string(),
                });
        }

        updates.push(format!("updated_at = ${}", param_idx));
        param_idx += 1;

        let query = format!(
            "UPDATE event_driven_dispatch_generation_rules SET {} WHERE id = ${}",
            updates.join(", "),
            param_idx
        );

        let mut q = sqlx::query(&query);

        if let Some(ref name) = payload.name {
            q = q.bind(name);
        }
        if let Some(ref desc) = payload.description {
            q = q.bind(desc);
        }
        if let Some(ref patterns) = payload.event_patterns {
            q = q.bind(serde_json::json!(patterns));
        }
        if let Some(priority) = payload.priority {
            q = q.bind(priority);
        }
        if let Some(ref conditions) = payload.conditions {
            let c = serde_json::to_value(conditions).ok();
            q = q.bind(c);
        } else {
            q = q.bind(serde_json::Value::Null);
        }
        if let Some(ref config) = payload.config {
            let cfg = serde_json::to_value(config).map_err(|e| DomainError::Internal(e.to_string()))?;
            q = q.bind(cfg);
        }
        if let Some(enabled) = payload.is_enabled {
            q = q.bind(enabled);
        }
        if let Some(ref dept_id) = payload.department_id {
            q = q.bind(dept_id);
        }
        q = q.bind(now);
        q = q.bind(id);

        q.execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        self.get_generation_rule(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "generation_rule",
                id: id.to_string(),
            })
    }

    async fn delete_generation_rule(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM event_driven_dispatch_generation_rules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn set_generation_rule_enabled(&self, id: &str, enabled: bool) -> Result<GenerationRuleRecord, DomainError> {
        let now = Utc::now();
        sqlx::query("UPDATE event_driven_dispatch_generation_rules SET is_enabled = $1, updated_at = $2 WHERE id = $3")
            .bind(enabled)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        self.get_generation_rule(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "generation_rule",
                id: id.to_string(),
            })
    }
}
