use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::business_case::{BusinessCaseType, VisibilityScope};
use fms_domain::ports::business_case_repository::BusinessCaseTypeRepository;

pub struct PgBusinessCaseTypeRepository {
    pool: PgPool,
}

impl PgBusinessCaseTypeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BusinessCaseTypeRepository for PgBusinessCaseTypeRepository {
    async fn find_all(&self, active_only: bool) -> Result<Vec<BusinessCaseType>, DomainError> {
        let sql = if active_only {
            "SELECT * FROM business_case_types WHERE is_active = TRUE ORDER BY created_at"
        } else {
            "SELECT * FROM business_case_types ORDER BY created_at"
        };

        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(rows.iter().map(row_to_case_type).collect())
    }

    async fn find_all_scoped(
        &self,
        active_only: bool,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<BusinessCaseType>, DomainError> {
        let mut conditions: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if active_only {
            conditions.push("is_active = TRUE".to_string());
        }

        let viewer_department_id = normalize_optional_scope_value(viewer_department_id);
        let viewer_department_name = normalize_optional_scope_value(viewer_department_name);

        let scope_conditions = if include_common {
            if viewer_department_id.is_some() || viewer_department_name.is_some() {
                let mut or_parts = Vec::new();
                or_parts.push("visibility_scope = 'COMMON'".to_string());
                if let Some(ref dept_id) = viewer_department_id {
                    binds.push(dept_id.clone());
                    or_parts.push(format!(
                        "(visibility_scope = 'DEPARTMENT' AND department_id = ${})",
                        binds.len()
                    ));
                }
                if let Some(ref dept_name) = viewer_department_name {
                    binds.push(dept_name.clone());
                    or_parts.push(format!(
                        "(visibility_scope = 'DEPARTMENT' AND department_name_snapshot = ${})",
                        binds.len()
                    ));
                }
                Some(format!("({})", or_parts.join(" OR ")))
            } else {
                Some("visibility_scope = 'COMMON'".to_string())
            }
        } else if viewer_department_id.is_some() || viewer_department_name.is_some() {
            let mut or_parts = Vec::new();
            if let Some(ref dept_id) = viewer_department_id {
                binds.push(dept_id.clone());
                or_parts.push(format!(
                    "(visibility_scope = 'DEPARTMENT' AND department_id = ${})",
                    binds.len()
                ));
            }
            if let Some(ref dept_name) = viewer_department_name {
                binds.push(dept_name.clone());
                or_parts.push(format!(
                    "(visibility_scope = 'DEPARTMENT' AND department_name_snapshot = ${})",
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

        let mut sql = String::from("SELECT * FROM business_case_types");
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at");

        let mut query = sqlx::query(&sql);
        for bind in &binds {
            query = query.bind(bind);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(rows.iter().map(row_to_case_type).collect())
    }

    async fn find_by_code(&self, code: &str) -> Result<Option<BusinessCaseType>, DomainError> {
        let row = sqlx::query("SELECT * FROM business_case_types WHERE code = $1")
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row.as_ref().map(row_to_case_type))
    }

    async fn find_by_code_scoped(
        &self,
        code: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        Ok(self
            .find_by_code(code)
            .await?
            .filter(|item| is_case_type_visible(item, viewer_department_id, viewer_department_name, include_common)))
    }

    async fn save(&self, entity: &BusinessCaseType) -> Result<BusinessCaseType, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO business_case_types (
                id, code, name, bpmn_xml, description, is_active, visibility_scope,
                department_id, department_name_snapshot, created_at, updated_at, ai_extraction_config, case_properties
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (code) DO UPDATE SET
                name = EXCLUDED.name,
                bpmn_xml = EXCLUDED.bpmn_xml,
                description = EXCLUDED.description,
                is_active = EXCLUDED.is_active,
                visibility_scope = EXCLUDED.visibility_scope,
                department_id = EXCLUDED.department_id,
                department_name_snapshot = EXCLUDED.department_name_snapshot,
                updated_at = EXCLUDED.updated_at,
                ai_extraction_config = EXCLUDED.ai_extraction_config,
                case_properties = EXCLUDED.case_properties
            RETURNING *
            "#,
        )
        .bind(&entity.id)
        .bind(&entity.code)
        .bind(&entity.name)
        .bind(&entity.bpmn_xml)
        .bind(&entity.description)
        .bind(entity.is_active)
        .bind(entity.visibility_scope.as_str())
        .bind(&entity.department_id)
        .bind(&entity.department_name_snapshot)
        .bind(entity.created_at)
        .bind(entity.updated_at)
        .bind(&entity.ai_extraction_config)
        .bind(&entity.case_properties)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row_to_case_type(&row))
    }

    async fn update_bpmn_xml(
        &self,
        code: &str,
        bpmn_xml: &str,
        description: Option<&str>,
    ) -> Result<bool, DomainError> {
        let result = if let Some(description) = description {
            sqlx::query(
                r#"
                UPDATE business_case_types
                SET bpmn_xml = $1, description = $2, updated_at = NOW()
                WHERE code = $3
                "#,
            )
            .bind(bpmn_xml)
            .bind(description)
            .bind(code)
            .execute(&self.pool)
            .await
        } else {
            sqlx::query(
                r#"
                UPDATE business_case_types
                SET bpmn_xml = $1, updated_at = NOW()
                WHERE code = $2
                "#,
            )
            .bind(bpmn_xml)
            .bind(code)
            .execute(&self.pool)
            .await
        }
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_status(&self, code: &str, is_active: bool) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE business_case_types
            SET is_active = $1, updated_at = NOW()
            WHERE code = $2
            "#,
        )
        .bind(is_active)
        .bind(code)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_ai_extraction_config(
        &self,
        code: &str,
        config: &serde_json::Value,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE business_case_types
            SET ai_extraction_config = $2, updated_at = NOW()
            WHERE code = $1
            RETURNING *
            "#,
        )
        .bind(code.trim())
        .bind(config)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row.as_ref().map(row_to_case_type))
    }

    async fn update_case_properties(
        &self,
        code: &str,
        properties: &serde_json::Value,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE business_case_types
            SET case_properties = $2, updated_at = NOW()
            WHERE code = $1
            RETURNING *
            "#,
        )
        .bind(code.trim())
        .bind(properties)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row.as_ref().map(row_to_case_type))
    }
}

fn row_to_case_type(row: &sqlx::postgres::PgRow) -> BusinessCaseType {
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
    let ai_extraction_config = row
        .try_get("ai_extraction_config")
        .unwrap_or_else(|_| serde_json::json!({}));
    let case_properties = row.try_get("case_properties").unwrap_or_else(|_| serde_json::json!({}));
    BusinessCaseType {
        id: row.get("id"),
        code: row.get("code"),
        name: row.get("name"),
        bpmn_xml: row.get("bpmn_xml"),
        description: row.get("description"),
        is_active: row.get("is_active"),
        visibility_scope,
        department_id,
        department_name_snapshot,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        ai_extraction_config,
        case_properties,
    }
}

fn normalize_optional_scope_value(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|item| !item.is_empty()).map(str::to_string)
}

fn is_case_type_visible(
    entity: &BusinessCaseType,
    viewer_department_id: Option<&str>,
    viewer_department_name: Option<&str>,
    include_common: bool,
) -> bool {
    match entity.visibility_scope {
        VisibilityScope::Common => include_common,
        VisibilityScope::Department => {
            let viewer_department_id = normalize_optional_scope_value(viewer_department_id);
            let viewer_department_name = normalize_optional_scope_value(viewer_department_name);

            entity.department_id.is_some() && entity.department_id == viewer_department_id
                || entity.department_name_snapshot.is_some()
                    && entity.department_name_snapshot == viewer_department_name
        }
    }
}
