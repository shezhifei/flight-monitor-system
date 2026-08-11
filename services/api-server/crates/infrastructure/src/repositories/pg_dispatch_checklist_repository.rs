//! PostgreSQL 安全检查清单仓储实现

use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use fms_domain::error::DomainError;
use fms_domain::ports::dispatch_repository::DispatchChecklistRepository;

pub struct PgDispatchChecklistRepository {
    pool: PgPool,
}

impl PgDispatchChecklistRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DispatchChecklistRepository for PgDispatchChecklistRepository {
    async fn get_template(&self, task_type: &str) -> Result<Option<serde_json::Value>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT template_id, task_type, checklist_version, checklist_items, is_active,
                   created_by, updated_by, created_at, updated_at
            FROM dispatch_safety_checklist_templates
            WHERE task_type = $1
              AND is_active = TRUE
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .bind(task_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        match row {
            Some(r) => {
                let id: String = r.try_get("template_id").unwrap_or_default();
                let sc: String = r.try_get("task_type").unwrap_or_default();
                let version: String = r.try_get("checklist_version").unwrap_or_default();
                let items: serde_json::Value = r.try_get("checklist_items").unwrap_or(serde_json::json!([]));
                let is_active: bool = r.try_get("is_active").unwrap_or(true);
                Ok(Some(serde_json::json!({
                    "template_id": id,
                    "task_type": sc,
                    "checklist_version": version,
                    "checklist_items": items,
                    "is_active": is_active,
                    "created_by": r.try_get::<Option<String>, _>("created_by").unwrap_or(None),
                    "updated_by": r.try_get::<Option<String>, _>("updated_by").unwrap_or(None),
                    "created_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at").unwrap_or(None),
                    "updated_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("updated_at").unwrap_or(None),
                })))
            }
            None => Ok(None),
        }
    }

    async fn upsert_template(
        &self,
        template_id: &str,
        task_type: &str,
        checklist_version: &str,
        checklist_items: &[serde_json::Value],
        is_active: bool,
        actor_user_id: Option<&str>,
    ) -> Result<serde_json::Value, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        if is_active {
            sqlx::query(
                r#"
                UPDATE dispatch_safety_checklist_templates
                SET is_active = FALSE,
                    updated_by = $2,
                    updated_at = CURRENT_TIMESTAMP
                WHERE task_type = $1
                  AND is_active = TRUE
                  AND checklist_version <> $3
                "#,
            )
            .bind(task_type)
            .bind(actor_user_id)
            .bind(checklist_version)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        let row = sqlx::query(
            r#"
            INSERT INTO dispatch_safety_checklist_templates (
                template_id,
                task_type,
                checklist_version,
                checklist_items,
                is_active,
                created_by,
                updated_by,
                created_at,
                updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $6, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT (task_type, checklist_version) DO UPDATE SET
                checklist_items = EXCLUDED.checklist_items,
                is_active = EXCLUDED.is_active,
                updated_by = EXCLUDED.updated_by,
                updated_at = CURRENT_TIMESTAMP
            RETURNING template_id, task_type, checklist_version, checklist_items, is_active,
                      created_by, updated_by, created_at, updated_at
            "#,
        )
        .bind(template_id)
        .bind(task_type)
        .bind(checklist_version)
        .bind(serde_json::Value::Array(checklist_items.to_vec()))
        .bind(is_active)
        .bind(actor_user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(serde_json::json!({
            "template_id": row.try_get::<String, _>("template_id").unwrap_or_else(|_| template_id.to_string()),
            "task_type": row.try_get::<String, _>("task_type").unwrap_or_else(|_| task_type.to_string()),
            "checklist_version": row.try_get::<String, _>("checklist_version").unwrap_or_else(|_| checklist_version.to_string()),
            "checklist_items": row.try_get::<serde_json::Value, _>("checklist_items").unwrap_or_else(|_| serde_json::json!(checklist_items)),
            "is_active": row.try_get::<bool, _>("is_active").unwrap_or(is_active),
            "created_by": row.try_get::<Option<String>, _>("created_by").unwrap_or_else(|_| actor_user_id.map(str::to_string)),
            "updated_by": row.try_get::<Option<String>, _>("updated_by").unwrap_or_else(|_| actor_user_id.map(str::to_string)),
            "created_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at").unwrap_or(None),
            "updated_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("updated_at").unwrap_or(None),
        }))
    }

    async fn list_records(&self, dispatch_order_id: &str) -> Result<Vec<serde_json::Value>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT record_id, dispatch_order_id, item_code, result, checked_by,
                   checked_at, note, template_version, created_at, updated_at
            FROM dispatch_safety_checklist_records
            WHERE dispatch_order_id = $1
            ORDER BY checked_at DESC, updated_at DESC
            "#,
        )
        .bind(dispatch_order_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "record_id": r.try_get::<String, _>("record_id").unwrap_or_default(),
                    "dispatch_order_id": r.try_get::<String, _>("dispatch_order_id").unwrap_or_default(),
                    "item_code": r.try_get::<String, _>("item_code").unwrap_or_default(),
                    "result": r.try_get::<String, _>("result").unwrap_or_default(),
                    "checked_by": r.try_get::<Option<String>, _>("checked_by").unwrap_or(None),
                    "checked_by_username": serde_json::Value::Null,
                    "checked_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("checked_at").unwrap_or(None),
                    "note": r.try_get::<Option<String>, _>("note").unwrap_or(None),
                    "template_version": r.try_get::<Option<String>, _>("template_version").unwrap_or(None),
                    "created_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at").unwrap_or(None),
                    "updated_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("updated_at").unwrap_or(None),
                })
            })
            .collect())
    }

    async fn submit_item_result(
        &self,
        dispatch_order_id: &str,
        task_type: &str,
        item_code: &str,
        result: Option<&str>,
        note: Option<&str>,
        checked_by: &str,
    ) -> Result<serde_json::Value, DomainError> {
        let record_id = Uuid::new_v4().to_string();

        let template_version: String = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
                (SELECT checklist_version FROM dispatch_safety_checklist_templates
                 WHERE task_type = $1 AND is_active = TRUE ORDER BY updated_at DESC LIMIT 1),
                'v1'
            )
            "#,
        )
        .bind(task_type)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let row = sqlx::query(
            r#"
            INSERT INTO dispatch_safety_checklist_records (
                record_id, dispatch_order_id, item_code,
                result, note, checked_by, template_version, checked_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
            ON CONFLICT (dispatch_order_id, item_code) DO UPDATE SET
                result            = EXCLUDED.result,
                note              = EXCLUDED.note,
                checked_by        = EXCLUDED.checked_by,
                template_version  = EXCLUDED.template_version,
                checked_at        = NOW(),
                updated_at        = NOW()
            RETURNING record_id, dispatch_order_id, item_code, result, checked_by,
                      checked_at, note, template_version, created_at, updated_at
            "#,
        )
        .bind(&record_id)
        .bind(dispatch_order_id)
        .bind(item_code)
        .bind(result)
        .bind(note)
        .bind(checked_by)
        .bind(&template_version)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(serde_json::json!({
            "record_id": row.try_get::<String, _>("record_id").unwrap_or(record_id),
            "dispatch_order_id": row.try_get::<String, _>("dispatch_order_id").unwrap_or_else(|_| dispatch_order_id.to_string()),
            "item_code": row.try_get::<String, _>("item_code").unwrap_or_else(|_| item_code.to_string()),
            "result": row.try_get::<String, _>("result").unwrap_or_else(|_| result.unwrap_or_default().to_string()),
            "note": row.try_get::<Option<String>, _>("note").unwrap_or_else(|_| note.map(str::to_string)),
            "checked_by": row.try_get::<Option<String>, _>("checked_by").unwrap_or_else(|_| Some(checked_by.to_string())),
            "checked_by_username": serde_json::Value::Null,
            "checked_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("checked_at").unwrap_or(None),
            "template_version": row.try_get::<Option<String>, _>("template_version").unwrap_or_else(|_| Some(template_version.clone())),
            "created_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at").unwrap_or(None),
            "updated_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("updated_at").unwrap_or(None),
        }))
    }

    async fn evaluate_completion_gate(&self, dispatch_order_id: &str, task_type: &str) -> Result<bool, DomainError> {
        let template = self.get_template(task_type).await?;
        let Some(template) = template else {
            return Ok(true);
        };

        let template_items = template
            .get("checklist_items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if template_items.is_empty() {
            return Ok(true);
        }

        let records = self.list_records(dispatch_order_id).await?;
        let record_map: std::collections::HashMap<String, serde_json::Value> = records
            .into_iter()
            .filter_map(|record| {
                let item_code = record.get("item_code").and_then(|v| v.as_str()).map(str::to_string);
                item_code.map(|code| (code, record))
            })
            .collect();

        for item in template_items {
            let required = item.get("required").and_then(|v| v.as_bool()).unwrap_or(true);
            if !required {
                continue;
            }

            let Some(item_code) = item.get("item_code").and_then(|v| v.as_str()) else {
                continue;
            };
            let allow_na = item.get("allow_na").and_then(|v| v.as_bool()).unwrap_or(false);
            let result = record_map
                .get(item_code)
                .and_then(|record| record.get("result"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            let ready = result == "pass" || (result == "na" && allow_na);
            if !ready {
                return Ok(false);
            }
        }

        Ok(true)
    }
}
