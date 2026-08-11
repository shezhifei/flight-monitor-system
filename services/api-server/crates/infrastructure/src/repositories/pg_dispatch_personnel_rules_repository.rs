//! PostgreSQL 派工人员规则仓储实现。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    DepartmentQualificationCatalog, DepartmentQualificationLevel, DepartmentRuleStatus,
    DepartmentTaskTypeRequirementVersion, DispatchPublicationState, FlightGenerationRule, GenerationAdjustmentRule,
    LegScope, PublishTriggerMode, QualificationGrant, QualificationGrantStatus, TaskTypeCrewSlotRequirement,
    TaskTypeEquipmentRequirement, TemporaryTaskTemplate, TurnaroundConstraintMode, TurnaroundContinuityRule,
    TurnaroundSlotPair,
};
use fms_domain::ports::dispatch_repository::{
    DepartmentQualificationRepository, DepartmentTaskTypeRequirementRepository, FlightGenerationRuleRepository,
    GenerationAdjustmentRuleRepository, QualificationGrantRepository, TemporaryTaskTemplateRepository,
};

pub struct PgDepartmentQualificationRepository {
    pool: PgPool,
}

impl PgDepartmentQualificationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DepartmentQualificationRepository for PgDepartmentQualificationRepository {
    async fn save_catalog(
        &self,
        catalog: &DepartmentQualificationCatalog,
    ) -> Result<DepartmentQualificationCatalog, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO department_qualification_catalog (
                id, department_id, qualification_code, qualification_name, description, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (department_id, qualification_code) DO UPDATE SET
                qualification_name = EXCLUDED.qualification_name,
                description = EXCLUDED.description,
                is_active = EXCLUDED.is_active,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, department_id, qualification_code, qualification_name, description,
                      is_active, created_at, updated_at
            "#,
        )
        .bind(&catalog.id)
        .bind(&catalog.department_id)
        .bind(&catalog.qualification_code)
        .bind(&catalog.qualification_name)
        .bind(&catalog.description)
        .bind(catalog.is_active)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_catalog(&row))
    }

    async fn list_catalogs(
        &self,
        department_id: &str,
        include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationCatalog>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, department_id, qualification_code, qualification_name, description, is_active, created_at, updated_at FROM department_qualification_catalog WHERE department_id = ",
        );
        builder.push_bind(department_id);
        if !include_inactive {
            builder.push(" AND is_active = TRUE");
        }
        builder.push(" ORDER BY qualification_code ASC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_catalog).collect())
    }

    async fn save_level(
        &self,
        level: &DepartmentQualificationLevel,
    ) -> Result<DepartmentQualificationLevel, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO department_qualification_levels (
                id, department_id, qualification_code, level_code, level_name,
                level_rank, covered_level_codes, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (department_id, qualification_code, level_code) DO UPDATE SET
                level_name = EXCLUDED.level_name,
                level_rank = EXCLUDED.level_rank,
                covered_level_codes = EXCLUDED.covered_level_codes,
                is_active = EXCLUDED.is_active,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, department_id, qualification_code, level_code, level_name,
                      level_rank, covered_level_codes, is_active, created_at, updated_at
            "#,
        )
        .bind(&level.id)
        .bind(&level.department_id)
        .bind(&level.qualification_code)
        .bind(&level.level_code)
        .bind(&level.level_name)
        .bind(level.level_rank)
        .bind(serde_json::to_value(&level.covered_level_codes).unwrap_or_else(|_| serde_json::json!([])))
        .bind(level.is_active)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_level(&row))
    }

    async fn list_levels(
        &self,
        department_id: &str,
        qualification_code: Option<&str>,
        include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationLevel>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, department_id, qualification_code, level_code, level_name, level_rank, covered_level_codes, is_active, created_at, updated_at FROM department_qualification_levels WHERE department_id = ",
        );
        builder.push_bind(department_id);
        if let Some(value) = qualification_code {
            builder.push(" AND qualification_code = ").push_bind(value);
        }
        if !include_inactive {
            builder.push(" AND is_active = TRUE");
        }
        builder.push(" ORDER BY qualification_code ASC, level_rank DESC, level_code ASC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_level).collect())
    }
}

pub struct PgQualificationGrantRepository {
    pool: PgPool,
}

impl PgQualificationGrantRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl QualificationGrantRepository for PgQualificationGrantRepository {
    async fn save(&self, grant: &QualificationGrant) -> Result<QualificationGrant, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO qualification_grants (
                id, user_id, department_id, qualification_code, level_code,
                valid_from, valid_to, status, source_team_id, metadata
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (user_id, department_id, qualification_code, level_code) DO UPDATE SET
                valid_from = EXCLUDED.valid_from,
                valid_to = EXCLUDED.valid_to,
                status = EXCLUDED.status,
                source_team_id = EXCLUDED.source_team_id,
                metadata = EXCLUDED.metadata,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, user_id, department_id, qualification_code, level_code,
                      valid_from, valid_to, status, source_team_id, metadata, created_at, updated_at
            "#,
        )
        .bind(&grant.id)
        .bind(&grant.user_id)
        .bind(&grant.department_id)
        .bind(&grant.qualification_code)
        .bind(&grant.level_code)
        .bind(grant.valid_from)
        .bind(grant.valid_to)
        .bind(qualification_grant_status_value(grant.status))
        .bind(&grant.source_team_id)
        .bind(serde_json::to_value(&grant.metadata).unwrap_or_else(|_| serde_json::json!({})))
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_grant(&row))
    }

    async fn find_by_department(
        &self,
        department_id: &str,
        at_time: Option<DateTime<Utc>>,
        user_ids: &[String],
        include_inactive: bool,
    ) -> Result<Vec<QualificationGrant>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, user_id, department_id, qualification_code, level_code, valid_from, valid_to, status, source_team_id, metadata, created_at, updated_at FROM qualification_grants WHERE department_id = ",
        );
        builder.push_bind(department_id);
        if !user_ids.is_empty() {
            builder.push(" AND user_id IN (");
            let mut separated = builder.separated(", ");
            for user_id in user_ids {
                separated.push_bind(user_id);
            }
            separated.push_unseparated(")");
        }
        if !include_inactive {
            builder.push(" AND status = ").push_bind("active");
        }
        if let Some(value) = at_time {
            builder
                .push(" AND (valid_from IS NULL OR valid_from <= ")
                .push_bind(value)
                .push(") AND (valid_to IS NULL OR valid_to >= ")
                .push_bind(value)
                .push(")");
        }
        builder.push(" ORDER BY user_id ASC, qualification_code ASC, level_code ASC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_grant).collect())
    }
}

pub struct PgDepartmentTaskTypeRequirementRepository {
    pool: PgPool,
}

impl PgDepartmentTaskTypeRequirementRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DepartmentTaskTypeRequirementRepository for PgDepartmentTaskTypeRequirementRepository {
    async fn next_version_no(&self, department_id: &str, task_type: &str) -> Result<i32, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(MAX(version_no), 0) AS max_version
            FROM department_task_type_requirement_versions
            WHERE department_id = $1 AND task_type = $2
            "#,
        )
        .bind(department_id)
        .bind(task_type)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.get::<i32, _>("max_version") + 1)
    }

    async fn save(
        &self,
        version: &DepartmentTaskTypeRequirementVersion,
    ) -> Result<DepartmentTaskTypeRequirementVersion, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO department_task_type_requirement_versions (
                id, department_id, task_type, version_no, status, requirements_json, notes, published_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status,
                requirements_json = EXCLUDED.requirements_json,
                notes = EXCLUDED.notes,
                published_at = EXCLUDED.published_at,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, department_id, task_type, version_no, status, requirements_json,
                      notes, published_at, created_at, updated_at
            "#,
        )
        .bind(&version.id)
        .bind(&version.department_id)
        .bind(&version.task_type)
        .bind(version.version_no)
        .bind(department_rule_status_value(version.status))
        .bind(requirement_version_to_json(version))
        .bind(&version.notes)
        .bind(version.published_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_requirement_version(&row))
    }

    async fn list_versions(
        &self,
        department_id: &str,
        task_type: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<DepartmentTaskTypeRequirementVersion>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, department_id, task_type, version_no, status, requirements_json, notes, published_at, created_at, updated_at FROM department_task_type_requirement_versions WHERE department_id = ",
        );
        builder.push_bind(department_id);
        if let Some(value) = task_type {
            builder.push(" AND task_type = ").push_bind(value);
        }
        if let Some(value) = status {
            builder.push(" AND status = ").push_bind(value);
        }
        builder.push(" ORDER BY task_type ASC, version_no DESC, created_at DESC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_requirement_version).collect())
    }

    async fn find_by_id(&self, version_id: &str) -> Result<Option<DepartmentTaskTypeRequirementVersion>, DomainError> {
        let row = sqlx::query(
            "SELECT id, department_id, task_type, version_no, status, requirements_json, notes, published_at, created_at, updated_at FROM department_task_type_requirement_versions WHERE id = $1",
        )
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.as_ref().map(row_to_requirement_version))
    }

    async fn find_latest_draft(
        &self,
        department_id: &str,
        task_type: &str,
    ) -> Result<Option<DepartmentTaskTypeRequirementVersion>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, department_id, task_type, version_no, status, requirements_json, notes, published_at, created_at, updated_at
            FROM department_task_type_requirement_versions
            WHERE department_id = $1 AND task_type = $2 AND status = $3
            ORDER BY version_no DESC, created_at DESC
            LIMIT 1
            "#,
        )
        .bind(department_id)
        .bind(task_type)
        .bind("draft")
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.as_ref().map(row_to_requirement_version))
    }

    async fn find_published(
        &self,
        department_id: &str,
        task_type: &str,
    ) -> Result<Option<DepartmentTaskTypeRequirementVersion>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, department_id, task_type, version_no, status, requirements_json, notes, published_at, created_at, updated_at
            FROM department_task_type_requirement_versions
            WHERE department_id = $1 AND task_type = $2 AND status = $3
            ORDER BY version_no DESC, published_at DESC NULLS LAST, created_at DESC
            LIMIT 1
            "#,
        )
        .bind(department_id)
        .bind(task_type)
        .bind("published")
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.as_ref().map(row_to_requirement_version))
    }

    async fn archive_published(&self, department_id: &str, task_type: &str) -> Result<i64, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE department_task_type_requirement_versions
            SET status = $1,
                updated_at = CURRENT_TIMESTAMP
            WHERE department_id = $2 AND task_type = $3 AND status = $4
            "#,
        )
        .bind("archived")
        .bind(department_id)
        .bind(task_type)
        .bind("published")
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(result.rows_affected() as i64)
    }
}

fn row_to_catalog(row: &sqlx::postgres::PgRow) -> DepartmentQualificationCatalog {
    DepartmentQualificationCatalog {
        id: row.get("id"),
        department_id: row.get("department_id"),
        qualification_code: row.get("qualification_code"),
        qualification_name: row.get("qualification_name"),
        description: row.get("description"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_level(row: &sqlx::postgres::PgRow) -> DepartmentQualificationLevel {
    DepartmentQualificationLevel {
        id: row.get("id"),
        department_id: row.get("department_id"),
        qualification_code: row.get("qualification_code"),
        level_code: row.get("level_code"),
        level_name: row.get("level_name"),
        level_rank: row.get::<Option<i32>, _>("level_rank").unwrap_or(0),
        covered_level_codes: decode_string_vec(row.get::<Option<serde_json::Value>, _>("covered_level_codes")),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_grant(row: &sqlx::postgres::PgRow) -> QualificationGrant {
    QualificationGrant {
        id: row.get("id"),
        user_id: row.get("user_id"),
        department_id: row.get("department_id"),
        qualification_code: row.get("qualification_code"),
        level_code: row.get("level_code"),
        valid_from: row.get("valid_from"),
        valid_to: row.get("valid_to"),
        status: parse_qualification_grant_status(row.get::<Option<String>, _>("status").as_deref()),
        source_team_id: row.get("source_team_id"),
        metadata: decode_object_map(row.get::<Option<serde_json::Value>, _>("metadata")),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_requirement_version(row: &sqlx::postgres::PgRow) -> DepartmentTaskTypeRequirementVersion {
    let (requirements, crew_requirements, equipment_requirements, turnaround_continuity_rules) =
        decode_requirement_version(row.get::<Option<serde_json::Value>, _>("requirements_json"));
    DepartmentTaskTypeRequirementVersion {
        id: row.get("id"),
        department_id: row.get("department_id"),
        task_type: row.get("task_type"),
        version_no: row.get::<Option<i32>, _>("version_no").unwrap_or(1),
        status: parse_department_rule_status(row.get::<Option<String>, _>("status").as_deref()),
        requirements,
        crew_requirements,
        equipment_requirements,
        turnaround_continuity_rules,
        notes: row.get("notes"),
        published_at: row.get("published_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn requirement_version_to_json(version: &DepartmentTaskTypeRequirementVersion) -> serde_json::Value {
    let crew_requirements = if version.crew_requirements.is_empty() {
        &version.requirements
    } else {
        &version.crew_requirements
    };
    serde_json::json!({
        "requirements": requirements_to_json(crew_requirements),
        "crew_requirements": requirements_to_json(crew_requirements),
        "equipment_requirements": equipment_requirements_to_json(&version.equipment_requirements),
        "turnaround_continuity_rules": turnaround_rules_to_json(&version.turnaround_continuity_rules),
    })
}

fn requirements_to_json(requirements: &[TaskTypeCrewSlotRequirement]) -> serde_json::Value {
    serde_json::Value::Array(
        requirements
            .iter()
            .map(|item| {
                serde_json::json!({
                    "slot_code": item.slot_code,
                    "qualification_code": item.qualification_code,
                    "min_level_code": item.min_level_code,
                    "required_count": item.required_count,
                    "must_be_distinct": item.must_be_distinct,
                    "exclusive_group": item.exclusive_group,
                    "remarks": item.remarks,
                })
            })
            .collect(),
    )
}

fn equipment_requirements_to_json(requirements: &[TaskTypeEquipmentRequirement]) -> serde_json::Value {
    serde_json::Value::Array(
        requirements
            .iter()
            .map(|item| {
                serde_json::json!({
                    "slot_code": item.slot_code,
                    "equipment_type_id": item.equipment_type_id,
                    "equipment_type_code": item.equipment_type_code,
                    "required_count": item.required_count,
                    "must_be_distinct": item.must_be_distinct,
                    "requires_driver": item.requires_driver,
                    "driver_qualification_code": item.driver_qualification_code,
                    "driver_min_level_code": item.driver_min_level_code,
                    "remarks": item.remarks,
                })
            })
            .collect(),
    )
}

fn turnaround_rules_to_json(rules: &[TurnaroundContinuityRule]) -> serde_json::Value {
    serde_json::Value::Array(
        rules
            .iter()
            .map(|item| {
                serde_json::json!({
                    "enabled": item.enabled,
                    "counterpart_leg_scope": leg_scope_value(item.counterpart_leg_scope),
                    "counterpart_task_type": item.counterpart_task_type,
                    "slot_pairs": item.slot_pairs.iter().map(|pair| serde_json::json!({
                        "inbound_slot_code": pair.inbound_slot_code,
                        "outbound_slot_code": pair.outbound_slot_code,
                    })).collect::<Vec<_>>(),
                    "constraint_mode": turnaround_constraint_mode_value(item.constraint_mode),
                    "tight_threshold_minutes": item.tight_threshold_minutes,
                    "relax_threshold_minutes": item.relax_threshold_minutes,
                    "flight_filters": item.flight_filters,
                    "aircraft_type_filters": item.aircraft_type_filters,
                    "notes": item.notes,
                })
            })
            .collect(),
    )
}

fn decode_requirement_version(
    value: Option<serde_json::Value>,
) -> (
    Vec<TaskTypeCrewSlotRequirement>,
    Vec<TaskTypeCrewSlotRequirement>,
    Vec<TaskTypeEquipmentRequirement>,
    Vec<TurnaroundContinuityRule>,
) {
    let payload = value.unwrap_or(serde_json::Value::Null);
    if let Some(array) = payload.as_array() {
        let requirements = decode_requirements_array(array);
        let cloned = requirements.clone();
        return (cloned, requirements, Vec::new(), Vec::new());
    }
    let object = payload.as_object();
    let crew_source = object
        .and_then(|item| item.get("crew_requirements").or_else(|| item.get("requirements")))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let equipment_source = object
        .and_then(|item| item.get("equipment_requirements"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let turnaround_source = object
        .and_then(|item| item.get("turnaround_continuity_rules"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let crew_requirements = decode_requirements_array(&crew_source);
    let cloned = crew_requirements.clone();
    (
        cloned,
        crew_requirements,
        decode_equipment_requirements_array(&equipment_source),
        decode_turnaround_rules_array(&turnaround_source),
    )
}

fn decode_requirements_array(items: &[serde_json::Value]) -> Vec<TaskTypeCrewSlotRequirement> {
    items
        .iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            Some(TaskTypeCrewSlotRequirement {
                slot_code: obj
                    .get("slot_code")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                qualification_code: obj
                    .get("qualification_code")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                min_level_code: obj.get("min_level_code").and_then(|v| v.as_str()).map(str::to_string),
                required_count: obj.get("required_count").and_then(|v| v.as_i64()).unwrap_or(1) as i32,
                must_be_distinct: obj.get("must_be_distinct").and_then(|v| v.as_bool()).unwrap_or(true),
                exclusive_group: obj.get("exclusive_group").and_then(|v| v.as_str()).map(str::to_string),
                remarks: obj.get("remarks").and_then(|v| v.as_str()).map(str::to_string),
            })
        })
        .collect()
}

fn decode_equipment_requirements_array(items: &[serde_json::Value]) -> Vec<TaskTypeEquipmentRequirement> {
    items
        .iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            Some(TaskTypeEquipmentRequirement {
                slot_code: obj
                    .get("slot_code")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                equipment_type_id: obj
                    .get("equipment_type_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                equipment_type_code: obj
                    .get("equipment_type_code")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                required_count: obj.get("required_count").and_then(|v| v.as_i64()).unwrap_or(1) as i32,
                must_be_distinct: obj.get("must_be_distinct").and_then(|v| v.as_bool()).unwrap_or(true),
                requires_driver: obj.get("requires_driver").and_then(|v| v.as_bool()).unwrap_or(false),
                driver_qualification_code: obj
                    .get("driver_qualification_code")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                driver_min_level_code: obj
                    .get("driver_min_level_code")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                remarks: obj.get("remarks").and_then(|v| v.as_str()).map(str::to_string),
            })
        })
        .collect()
}

fn decode_turnaround_rules_array(items: &[serde_json::Value]) -> Vec<TurnaroundContinuityRule> {
    items
        .iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            Some(TurnaroundContinuityRule {
                enabled: obj.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
                counterpart_leg_scope: parse_leg_scope(obj.get("counterpart_leg_scope").and_then(|v| v.as_str())),
                counterpart_task_type: obj
                    .get("counterpart_task_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                slot_pairs: obj
                    .get("slot_pairs")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|pair| {
                        let pair_obj = pair.as_object()?;
                        Some(TurnaroundSlotPair {
                            inbound_slot_code: pair_obj
                                .get("inbound_slot_code")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            outbound_slot_code: pair_obj
                                .get("outbound_slot_code")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                        })
                    })
                    .collect(),
                constraint_mode: parse_turnaround_constraint_mode(obj.get("constraint_mode").and_then(|v| v.as_str())),
                tight_threshold_minutes: obj
                    .get("tight_threshold_minutes")
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32),
                relax_threshold_minutes: obj
                    .get("relax_threshold_minutes")
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32),
                flight_filters: obj
                    .get("flight_filters")
                    .and_then(|v| v.as_object())
                    .map(|map| map.clone().into_iter().collect())
                    .unwrap_or_default(),
                aircraft_type_filters: obj
                    .get("aircraft_type_filters")
                    .and_then(|v| v.as_array())
                    .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
                notes: obj.get("notes").and_then(|v| v.as_str()).map(str::to_string),
            })
        })
        .collect()
}

pub struct PgFlightGenerationRuleRepository {
    pool: PgPool,
}

impl PgFlightGenerationRuleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn save_in_tx(
        tx: &mut sqlx::Transaction<'_, Postgres>,
        rule: &FlightGenerationRule,
    ) -> Result<FlightGenerationRule, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO department_flight_generation_rules (
                id, department_id, task_type, leg_scope, version_no, status, rule_name,
                conditions_json, generation_anchor_type, start_offset_minutes, duration_minutes,
                publication_state, publish_trigger_mode, publish_at, publish_offset_minutes,
                publish_event_code, notes, published_at, start_flex_minutes,
                duration_by_crew_size, completion_time_mode, completion_anchor_type,
                completion_offset_minutes, completion_warning_lead_minutes
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8, $9, $10, $11,
                $12, $13, $14, $15,
                $16, $17, $18, $19,
                $20, $21, $22,
                $23, $24
            )
            ON CONFLICT (id) DO UPDATE SET
                version_no = EXCLUDED.version_no,
                status = EXCLUDED.status,
                rule_name = EXCLUDED.rule_name,
                conditions_json = EXCLUDED.conditions_json,
                generation_anchor_type = EXCLUDED.generation_anchor_type,
                start_offset_minutes = EXCLUDED.start_offset_minutes,
                duration_minutes = EXCLUDED.duration_minutes,
                publication_state = EXCLUDED.publication_state,
                publish_trigger_mode = EXCLUDED.publish_trigger_mode,
                publish_at = EXCLUDED.publish_at,
                publish_offset_minutes = EXCLUDED.publish_offset_minutes,
                publish_event_code = EXCLUDED.publish_event_code,
                notes = EXCLUDED.notes,
                published_at = EXCLUDED.published_at,
                start_flex_minutes = EXCLUDED.start_flex_minutes,
                duration_by_crew_size = EXCLUDED.duration_by_crew_size,
                completion_time_mode = EXCLUDED.completion_time_mode,
                completion_anchor_type = EXCLUDED.completion_anchor_type,
                completion_offset_minutes = EXCLUDED.completion_offset_minutes,
                completion_warning_lead_minutes = EXCLUDED.completion_warning_lead_minutes,
                updated_at = CURRENT_TIMESTAMP
            RETURNING *
            "#,
        )
        .bind(&rule.id)
        .bind(&rule.department_id)
        .bind(&rule.task_type)
        .bind(leg_scope_value(rule.leg_scope))
        .bind(rule.version_no)
        .bind(department_rule_status_value(rule.status))
        .bind(&rule.rule_name)
        .bind(serde_json::to_value(&rule.conditions).unwrap_or_else(|_| serde_json::json!({})))
        .bind(&rule.generation_anchor_type)
        .bind(rule.start_offset_minutes)
        .bind(rule.duration_minutes)
        .bind(publication_state_value(rule.publication_state))
        .bind(publish_trigger_mode_value(rule.publish_trigger_mode))
        .bind(rule.publish_at)
        .bind(rule.publish_offset_minutes)
        .bind(&rule.publish_event_code)
        .bind(&rule.notes)
        .bind(rule.published_at)
        .bind(rule.start_flex_minutes)
        .bind(&rule.duration_by_crew_size)
        .bind(&rule.completion_time_mode)
        .bind(&rule.completion_anchor_type)
        .bind(rule.completion_offset_minutes)
        .bind(rule.completion_warning_lead_minutes)
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row_to_generation_rule(&row))
    }
}

#[async_trait]
impl FlightGenerationRuleRepository for PgFlightGenerationRuleRepository {
    async fn next_version_no(&self, department_id: &str, task_type: &str, leg_scope: &str) -> Result<i32, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(MAX(version_no), 0) AS max_version
            FROM department_flight_generation_rules
            WHERE department_id = $1 AND task_type = $2 AND leg_scope = $3
            "#,
        )
        .bind(department_id)
        .bind(task_type)
        .bind(leg_scope)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.get::<i32, _>("max_version") + 1)
    }

    async fn save(&self, rule: &FlightGenerationRule) -> Result<FlightGenerationRule, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        let saved = Self::save_in_tx(&mut tx, rule).await?;
        tx.commit()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(saved)
    }

    async fn save_replacing_published(
        &self,
        rule: &FlightGenerationRule,
        previous_rule_id: &str,
    ) -> Result<FlightGenerationRule, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        let version_lock_key = format!(
            "generation-rule:{}:{}:{}",
            rule.department_id,
            rule.task_type,
            leg_scope_value(rule.leg_scope)
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(version_lock_key)
            .execute(&mut *tx)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        let source_exists = sqlx::query_scalar::<_, bool>(
            "SELECT TRUE FROM department_flight_generation_rules \
             WHERE id = $1 AND department_id = $2 AND status = 'published' FOR UPDATE",
        )
        .bind(previous_rule_id)
        .bind(&rule.department_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?
        .unwrap_or(false);
        if !source_exists {
            return Err(DomainError::BusinessRuleViolation(
                "生成规则源版本已发生变化，请刷新后重试".to_string(),
            ));
        }
        sqlx::query(
            "UPDATE department_flight_generation_rules \
             SET status = 'archived', published_at = NULL, updated_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND department_id = $2 AND status = 'published'",
        )
        .bind(previous_rule_id)
        .bind(&rule.department_id)
        .execute(&mut *tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        let next_version = sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(MAX(version_no), 0) + 1 FROM department_flight_generation_rules \
             WHERE department_id = $1 AND task_type = $2 AND leg_scope = $3",
        )
        .bind(&rule.department_id)
        .bind(&rule.task_type)
        .bind(leg_scope_value(rule.leg_scope))
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        let mut new_version = rule.clone();
        new_version.version_no = next_version;
        let saved = Self::save_in_tx(&mut tx, &new_version).await?;
        tx.commit()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(saved)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<FlightGenerationRule>, DomainError> {
        let row = sqlx::query("SELECT * FROM department_flight_generation_rules WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(|row| row_to_generation_rule(&row)))
    }

    async fn list_rules(
        &self,
        department_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<FlightGenerationRule>, DomainError> {
        let mut builder =
            QueryBuilder::<Postgres>::new("SELECT * FROM department_flight_generation_rules WHERE department_id = ");
        builder.push_bind(department_id);
        if let Some(status) = status {
            builder.push(" AND status = ").push_bind(status);
        }
        builder.push(" ORDER BY task_type ASC, leg_scope ASC, version_no DESC, created_at DESC");
        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(rows.iter().map(row_to_generation_rule).collect())
    }
}

pub struct PgGenerationAdjustmentRuleRepository {
    pool: PgPool,
}

impl PgGenerationAdjustmentRuleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn save_in_tx(
        tx: &mut sqlx::Transaction<'_, Postgres>,
        rule: &GenerationAdjustmentRule,
    ) -> Result<GenerationAdjustmentRule, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO department_generation_adjustment_rules (
                id, department_id, task_type, version_no, status, rule_name,
                conditions_json, actions_json, notes, published_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO UPDATE SET
                version_no = EXCLUDED.version_no,
                status = EXCLUDED.status,
                rule_name = EXCLUDED.rule_name,
                conditions_json = EXCLUDED.conditions_json,
                actions_json = EXCLUDED.actions_json,
                notes = EXCLUDED.notes,
                published_at = EXCLUDED.published_at,
                updated_at = CURRENT_TIMESTAMP
            RETURNING *
            "#,
        )
        .bind(&rule.id)
        .bind(&rule.department_id)
        .bind(&rule.task_type)
        .bind(rule.version_no)
        .bind(department_rule_status_value(rule.status))
        .bind(&rule.rule_name)
        .bind(serde_json::to_value(&rule.conditions).unwrap_or_else(|_| serde_json::json!({})))
        .bind(serde_json::Value::Array(rule.actions.clone()))
        .bind(&rule.notes)
        .bind(rule.published_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row_to_adjustment_rule(&row))
    }
}

#[async_trait]
impl GenerationAdjustmentRuleRepository for PgGenerationAdjustmentRuleRepository {
    async fn next_version_no(&self, department_id: &str, task_type: &str) -> Result<i32, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(MAX(version_no), 0) AS max_version
            FROM department_generation_adjustment_rules
            WHERE department_id = $1 AND task_type = $2
            "#,
        )
        .bind(department_id)
        .bind(task_type)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.get::<i32, _>("max_version") + 1)
    }

    async fn save(&self, rule: &GenerationAdjustmentRule) -> Result<GenerationAdjustmentRule, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        let saved = Self::save_in_tx(&mut tx, rule).await?;
        tx.commit()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(saved)
    }

    async fn save_replacing_published(
        &self,
        rule: &GenerationAdjustmentRule,
        previous_rule_id: &str,
    ) -> Result<GenerationAdjustmentRule, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        let version_lock_key = format!("adjustment-rule:{}:{}", rule.department_id, rule.task_type);
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(version_lock_key)
            .execute(&mut *tx)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        let source_exists = sqlx::query_scalar::<_, bool>(
            "SELECT TRUE FROM department_generation_adjustment_rules \
             WHERE id = $1 AND department_id = $2 AND status = 'published' FOR UPDATE",
        )
        .bind(previous_rule_id)
        .bind(&rule.department_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?
        .unwrap_or(false);
        if !source_exists {
            return Err(DomainError::BusinessRuleViolation(
                "调整规则源版本已发生变化，请刷新后重试".to_string(),
            ));
        }
        sqlx::query(
            "UPDATE department_generation_adjustment_rules \
             SET status = 'archived', published_at = NULL, updated_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND department_id = $2 AND status = 'published'",
        )
        .bind(previous_rule_id)
        .bind(&rule.department_id)
        .execute(&mut *tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        let next_version = sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(MAX(version_no), 0) + 1 FROM department_generation_adjustment_rules \
             WHERE department_id = $1 AND task_type = $2",
        )
        .bind(&rule.department_id)
        .bind(&rule.task_type)
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        let mut new_version = rule.clone();
        new_version.version_no = next_version;
        let saved = Self::save_in_tx(&mut tx, &new_version).await?;
        tx.commit()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(saved)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<GenerationAdjustmentRule>, DomainError> {
        let row = sqlx::query("SELECT * FROM department_generation_adjustment_rules WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(|row| row_to_adjustment_rule(&row)))
    }

    async fn list_rules(
        &self,
        department_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<GenerationAdjustmentRule>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT * FROM department_generation_adjustment_rules WHERE department_id = ",
        );
        builder.push_bind(department_id);
        if let Some(status) = status {
            builder.push(" AND status = ").push_bind(status);
        }
        builder.push(" ORDER BY task_type ASC, version_no DESC, created_at DESC");
        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(rows.iter().map(row_to_adjustment_rule).collect())
    }
}

pub struct PgTemporaryTaskTemplateRepository {
    pool: PgPool,
}

impl PgTemporaryTaskTemplateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TemporaryTaskTemplateRepository for PgTemporaryTaskTemplateRepository {
    async fn save(&self, template: &TemporaryTaskTemplate) -> Result<TemporaryTaskTemplate, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO dispatch_temporary_task_templates (
                id, department_id, template_code, template_name, task_type,
                requirements_json, notes, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (department_id, template_code) DO UPDATE SET
                template_name = EXCLUDED.template_name,
                task_type = EXCLUDED.task_type,
                requirements_json = EXCLUDED.requirements_json,
                notes = EXCLUDED.notes,
                is_active = EXCLUDED.is_active,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, department_id, template_code, template_name, task_type,
                      requirements_json, notes, is_active, created_at, updated_at
            "#,
        )
        .bind(&template.id)
        .bind(&template.department_id)
        .bind(&template.template_code)
        .bind(&template.template_name)
        .bind(&template.task_type)
        .bind(temporary_task_template_to_json(template))
        .bind(&template.notes)
        .bind(template.is_active)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_temporary_task_template(&row))
    }

    async fn find_by_code(
        &self,
        department_id: &str,
        template_code: &str,
    ) -> Result<Option<TemporaryTaskTemplate>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, department_id, template_code, template_name, task_type,
                   requirements_json, notes, is_active, created_at, updated_at
            FROM dispatch_temporary_task_templates
            WHERE department_id = $1 AND template_code = $2
            LIMIT 1
            "#,
        )
        .bind(department_id)
        .bind(template_code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.as_ref().map(row_to_temporary_task_template))
    }

    async fn list_templates(
        &self,
        department_id: &str,
        include_inactive: bool,
    ) -> Result<Vec<TemporaryTaskTemplate>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, department_id, template_code, template_name, task_type, requirements_json, notes, is_active, created_at, updated_at FROM dispatch_temporary_task_templates WHERE department_id = ",
        );
        builder.push_bind(department_id);
        if !include_inactive {
            builder.push(" AND is_active = TRUE");
        }
        builder.push(" ORDER BY template_code ASC, created_at ASC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_temporary_task_template).collect())
    }
}

fn row_to_generation_rule(row: &sqlx::postgres::PgRow) -> FlightGenerationRule {
    FlightGenerationRule {
        id: row.get("id"),
        department_id: row.get("department_id"),
        task_type: row.get("task_type"),
        leg_scope: parse_leg_scope(row.get::<Option<String>, _>("leg_scope").as_deref()),
        version_no: row.get::<Option<i32>, _>("version_no").unwrap_or(1),
        status: parse_department_rule_status(row.get::<Option<String>, _>("status").as_deref()),
        rule_name: row.get("rule_name"),
        conditions: decode_object_map(row.get::<Option<serde_json::Value>, _>("conditions_json")),
        generation_anchor_type: row
            .get::<Option<String>, _>("generation_anchor_type")
            .unwrap_or_else(|| "scheduled_time".to_string()),
        start_offset_minutes: row.get::<Option<i32>, _>("start_offset_minutes").unwrap_or(0),
        completion_time_mode: row
            .get::<Option<String>, _>("completion_time_mode")
            .unwrap_or_else(|| "start_plus_duration".to_string()),
        completion_anchor_type: row.get("completion_anchor_type"),
        completion_offset_minutes: row.get("completion_offset_minutes"),
        duration_minutes: row.get("duration_minutes"),
        start_flex_minutes: row.get("start_flex_minutes"),
        duration_by_crew_size: row.get("duration_by_crew_size"),
        completion_warning_lead_minutes: row.get("completion_warning_lead_minutes"),
        publication_state: parse_publication_state(row.get::<Option<String>, _>("publication_state").as_deref()),
        publish_trigger_mode: parse_publish_trigger_mode(
            row.get::<Option<String>, _>("publish_trigger_mode").as_deref(),
        ),
        publish_at: row.get("publish_at"),
        publish_offset_minutes: row.get("publish_offset_minutes"),
        publish_event_code: row.get("publish_event_code"),
        notes: row.get("notes"),
        published_at: row.get("published_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_adjustment_rule(row: &sqlx::postgres::PgRow) -> GenerationAdjustmentRule {
    GenerationAdjustmentRule {
        id: row.get("id"),
        department_id: row.get("department_id"),
        task_type: row.get("task_type"),
        version_no: row.get::<Option<i32>, _>("version_no").unwrap_or(1),
        status: parse_department_rule_status(row.get::<Option<String>, _>("status").as_deref()),
        rule_name: row.get("rule_name"),
        conditions: decode_object_map(row.get::<Option<serde_json::Value>, _>("conditions_json")),
        actions: row
            .get::<Option<serde_json::Value>, _>("actions_json")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default(),
        notes: row.get("notes"),
        published_at: row.get("published_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_temporary_task_template(row: &sqlx::postgres::PgRow) -> TemporaryTaskTemplate {
    let payload = row
        .get::<Option<serde_json::Value>, _>("requirements_json")
        .unwrap_or(serde_json::Value::Null);
    let object = payload.as_object();
    let crew_requirements = object
        .and_then(|item| item.get("crew_requirements"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let equipment_requirements = object
        .and_then(|item| item.get("equipment_requirements"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    TemporaryTaskTemplate {
        id: row.get("id"),
        department_id: row.get("department_id"),
        template_code: row.get("template_code"),
        template_name: row.get("template_name"),
        task_type: row.get("task_type"),
        crew_requirements: decode_requirements_array(&crew_requirements),
        equipment_requirements: decode_equipment_requirements_array(&equipment_requirements),
        notes: row.get("notes"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn temporary_task_template_to_json(template: &TemporaryTaskTemplate) -> serde_json::Value {
    serde_json::json!({
        "crew_requirements": requirements_to_json(&template.crew_requirements),
        "equipment_requirements": equipment_requirements_to_json(&template.equipment_requirements),
    })
}

fn decode_string_vec(value: Option<serde_json::Value>) -> Vec<String> {
    value
        .and_then(|item| item.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

fn decode_object_map(value: Option<serde_json::Value>) -> std::collections::HashMap<String, serde_json::Value> {
    value
        .and_then(|item| item.as_object().cloned())
        .map(|map| map.into_iter().collect())
        .unwrap_or_default()
}

fn parse_department_rule_status(value: Option<&str>) -> DepartmentRuleStatus {
    match value.unwrap_or("draft") {
        "published" => DepartmentRuleStatus::Published,
        "archived" => DepartmentRuleStatus::Archived,
        _ => DepartmentRuleStatus::Draft,
    }
}

fn parse_leg_scope(value: Option<&str>) -> LegScope {
    match value.unwrap_or("none") {
        "inbound" => LegScope::Inbound,
        "outbound" => LegScope::Outbound,
        _ => LegScope::None,
    }
}

fn leg_scope_value(value: LegScope) -> &'static str {
    match value {
        LegScope::Inbound => "inbound",
        LegScope::Outbound => "outbound",
        LegScope::None => "none",
    }
}

fn parse_publication_state(value: Option<&str>) -> DispatchPublicationState {
    match value.unwrap_or("prepublished") {
        "published" => DispatchPublicationState::Published,
        "cancelled" => DispatchPublicationState::Cancelled,
        _ => DispatchPublicationState::Prepublished,
    }
}

fn publication_state_value(value: DispatchPublicationState) -> &'static str {
    match value {
        DispatchPublicationState::Prepublished => "prepublished",
        DispatchPublicationState::Published => "published",
        DispatchPublicationState::Cancelled => "cancelled",
    }
}

fn parse_publish_trigger_mode(value: Option<&str>) -> PublishTriggerMode {
    match value.unwrap_or("time") {
        "event" => PublishTriggerMode::Event,
        "either" => PublishTriggerMode::Either,
        "both_required" => PublishTriggerMode::BothRequired,
        _ => PublishTriggerMode::Time,
    }
}

fn publish_trigger_mode_value(value: PublishTriggerMode) -> &'static str {
    match value {
        PublishTriggerMode::Time => "time",
        PublishTriggerMode::Event => "event",
        PublishTriggerMode::Either => "either",
        PublishTriggerMode::BothRequired => "both_required",
    }
}

fn parse_turnaround_constraint_mode(value: Option<&str>) -> TurnaroundConstraintMode {
    match value.unwrap_or("disabled") {
        "same_person" => TurnaroundConstraintMode::SamePerson,
        "soft_prefer_same_person" => TurnaroundConstraintMode::SoftPreferSamePerson,
        "handover_required" => TurnaroundConstraintMode::HandoverRequired,
        _ => TurnaroundConstraintMode::Disabled,
    }
}

fn turnaround_constraint_mode_value(value: TurnaroundConstraintMode) -> &'static str {
    match value {
        TurnaroundConstraintMode::SamePerson => "same_person",
        TurnaroundConstraintMode::SoftPreferSamePerson => "soft_prefer_same_person",
        TurnaroundConstraintMode::HandoverRequired => "handover_required",
        TurnaroundConstraintMode::Disabled => "disabled",
    }
}

fn department_rule_status_value(status: DepartmentRuleStatus) -> &'static str {
    match status {
        DepartmentRuleStatus::Draft => "draft",
        DepartmentRuleStatus::Published => "published",
        DepartmentRuleStatus::Archived => "archived",
    }
}

fn parse_qualification_grant_status(value: Option<&str>) -> QualificationGrantStatus {
    match value.unwrap_or("active") {
        "expired" => QualificationGrantStatus::Expired,
        "suspended" => QualificationGrantStatus::Suspended,
        _ => QualificationGrantStatus::Active,
    }
}

fn qualification_grant_status_value(status: QualificationGrantStatus) -> &'static str {
    match status {
        QualificationGrantStatus::Active => "active",
        QualificationGrantStatus::Expired => "expired",
        QualificationGrantStatus::Suspended => "suspended",
    }
}
