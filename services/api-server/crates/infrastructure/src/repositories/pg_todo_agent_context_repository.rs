//! PostgreSQL Todo agent context 仓储实现。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::ports::todo_agent_context_repository::{TodoAgentContext, TodoAgentContextRepository};

#[derive(Default)]
struct QueryMetrics {
    get_calls: f64,
    get_context_hits: f64,
    get_misses: f64,
    get_duration_ms_total: f64,
    batch_get_calls: f64,
    batch_get_requested_ids_total: f64,
    batch_get_context_hits: f64,
    batch_get_duration_ms_total: f64,
    find_todo_ids_calls: f64,
    find_todo_ids_context_preferred_calls: f64,
    find_todo_ids_hybrid_calls: f64,
    find_todo_ids_duration_ms_total: f64,
}

pub struct PgTodoAgentContextRepository {
    pool: PgPool,
    metrics: Mutex<QueryMetrics>,
}

impl PgTodoAgentContextRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            metrics: Mutex::new(QueryMetrics::default()),
        }
    }

    fn inc_metric(&self, metric: fn(&mut QueryMetrics) -> &mut f64, value: f64) {
        if let Ok(mut metrics) = self.metrics.lock() {
            *metric(&mut metrics) += value;
        }
    }

    fn duration_ms(started: Instant) -> f64 {
        started.elapsed().as_secs_f64() * 1000.0
    }

    fn normalize_agent_entity_id(value: Option<&str>) -> String {
        let normalized = value.unwrap_or("").trim();
        if normalized.is_empty() {
            "default".to_string()
        } else {
            normalized.to_string()
        }
    }

    fn normalize_agent_status(value: Option<&str>) -> String {
        let normalized = value.unwrap_or("").trim();
        if normalized.is_empty() {
            "pending".to_string()
        } else {
            normalized.to_string()
        }
    }

    fn normalize_updated_by(value: &str) -> String {
        let normalized = value.trim();
        if normalized.is_empty() {
            "system".to_string()
        } else {
            normalized.to_string()
        }
    }

    fn row_to_context(row: &sqlx::postgres::PgRow) -> TodoAgentContext {
        TodoAgentContext {
            todo_id: row.get("todo_id"),
            agent_entity_id: Self::normalize_agent_entity_id(
                row.try_get::<Option<String>, _>("agent_entity_id")
                    .ok()
                    .flatten()
                    .as_deref(),
            ),
            agent_run_id: row.try_get("agent_run_id").ok().flatten(),
            agent_status: Self::normalize_agent_status(
                row.try_get::<Option<String>, _>("agent_status")
                    .ok()
                    .flatten()
                    .as_deref(),
            ),
            updated_by: Self::normalize_updated_by(
                row.try_get::<Option<String>, _>("updated_by")
                    .ok()
                    .flatten()
                    .as_deref()
                    .unwrap_or("system"),
            ),
            updated_at: row.try_get::<Option<DateTime<Utc>>, _>("updated_at").ok().flatten(),
            version: row.try_get::<i32, _>("version").unwrap_or(1),
        }
    }
}

#[async_trait]
impl TodoAgentContextRepository for PgTodoAgentContextRepository {
    async fn get(&self, todo_id: &str) -> Result<Option<TodoAgentContext>, DomainError> {
        let started = Instant::now();
        self.inc_metric(|metrics| &mut metrics.get_calls, 1.0);
        let normalized_todo_id = todo_id.trim();
        if normalized_todo_id.is_empty() {
            self.inc_metric(|metrics| &mut metrics.get_misses, 1.0);
            self.inc_metric(|metrics| &mut metrics.get_duration_ms_total, Self::duration_ms(started));
            return Ok(None);
        }

        let row = sqlx::query(
            r#"
            SELECT
                todo_id,
                agent_entity_id,
                agent_run_id,
                agent_status,
                updated_by,
                updated_at,
                version
            FROM todo_agent_context
            WHERE todo_id = $1
            "#,
        )
        .bind(normalized_todo_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        self.inc_metric(|metrics| &mut metrics.get_duration_ms_total, Self::duration_ms(started));

        if let Some(row) = row {
            self.inc_metric(|metrics| &mut metrics.get_context_hits, 1.0);
            Ok(Some(Self::row_to_context(&row)))
        } else {
            self.inc_metric(|metrics| &mut metrics.get_misses, 1.0);
            Ok(None)
        }
    }

    async fn batch_get(&self, todo_ids: &[String]) -> Result<HashMap<String, TodoAgentContext>, DomainError> {
        let started = Instant::now();
        self.inc_metric(|metrics| &mut metrics.batch_get_calls, 1.0);
        let normalized_ids: Vec<String> = todo_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect();
        self.inc_metric(
            |metrics| &mut metrics.batch_get_requested_ids_total,
            normalized_ids.len() as f64,
        );
        if normalized_ids.is_empty() {
            self.inc_metric(
                |metrics| &mut metrics.batch_get_duration_ms_total,
                Self::duration_ms(started),
            );
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT
                todo_id,
                agent_entity_id,
                agent_run_id,
                agent_status,
                updated_by,
                updated_at,
                version
            FROM todo_agent_context
            WHERE todo_id = ANY($1)
            "#,
        )
        .bind(&normalized_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        self.inc_metric(|metrics| &mut metrics.batch_get_context_hits, rows.len() as f64);
        self.inc_metric(
            |metrics| &mut metrics.batch_get_duration_ms_total,
            Self::duration_ms(started),
        );

        Ok(rows
            .iter()
            .map(Self::row_to_context)
            .map(|context| (context.todo_id.clone(), context))
            .collect())
    }

    async fn upsert_partial(
        &self,
        todo_id: &str,
        agent_entity_id: Option<&str>,
        agent_run_id: Option<&str>,
        agent_status: Option<&str>,
        updated_by: &str,
    ) -> Result<TodoAgentContext, DomainError> {
        let normalized_todo_id = todo_id.trim();
        if normalized_todo_id.is_empty() {
            return Err(DomainError::ValidationError(
                "todo_id is required for upsert_partial".into(),
            ));
        }

        let existing = self.get(normalized_todo_id).await?;
        let base_entity = existing
            .as_ref()
            .map(|item| item.agent_entity_id.as_str())
            .unwrap_or("default");
        let base_run_id = existing.as_ref().and_then(|item| item.agent_run_id.as_deref());
        let base_status = existing
            .as_ref()
            .map(|item| item.agent_status.as_str())
            .unwrap_or("pending");
        let base_version = existing.as_ref().map(|item| item.version).unwrap_or(1);

        let row = sqlx::query(
            r#"
            INSERT INTO todo_agent_context (
                todo_id,
                agent_entity_id,
                agent_run_id,
                agent_status,
                updated_by,
                version
            ) VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (todo_id) DO UPDATE SET
                agent_entity_id = EXCLUDED.agent_entity_id,
                agent_run_id = EXCLUDED.agent_run_id,
                agent_status = EXCLUDED.agent_status,
                updated_by = EXCLUDED.updated_by,
                updated_at = CURRENT_TIMESTAMP,
                version = todo_agent_context.version + 1
            RETURNING
                todo_id,
                agent_entity_id,
                agent_run_id,
                agent_status,
                updated_by,
                updated_at,
                version
            "#,
        )
        .bind(normalized_todo_id)
        .bind(Self::normalize_agent_entity_id(agent_entity_id.or(Some(base_entity))))
        .bind(
            agent_run_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or(base_run_id),
        )
        .bind(Self::normalize_agent_status(agent_status.or(Some(base_status))))
        .bind(Self::normalize_updated_by(updated_by))
        .bind(base_version.max(1))
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(Self::row_to_context(&row))
    }

    async fn find_todo_ids(
        &self,
        agent_status: Option<&str>,
        agent_entity_id: Option<&str>,
        agent_run_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<String>, DomainError> {
        let started = Instant::now();
        self.inc_metric(|metrics| &mut metrics.find_todo_ids_calls, 1.0);
        self.inc_metric(|metrics| &mut metrics.find_todo_ids_context_preferred_calls, 1.0);

        let mut query = String::from(
            r#"
            SELECT t.todo_id
            FROM todos t
            INNER JOIN todo_agent_context tac ON tac.todo_id = t.todo_id
            WHERE COALESCE(t.is_deleted, FALSE) = FALSE
            "#,
        );
        let mut bind_index = 1i32;
        let mut binds: Vec<String> = Vec::new();

        if let Some(status) = agent_status.map(str::trim).filter(|value| !value.is_empty()) {
            query.push_str(&format!(
                " AND COALESCE(NULLIF(BTRIM(tac.agent_status), ''), 'pending') = ${bind_index}"
            ));
            binds.push(status.to_string());
            bind_index += 1;
        }
        if let Some(entity_id) = agent_entity_id.map(str::trim).filter(|value| !value.is_empty()) {
            query.push_str(&format!(
                " AND COALESCE(NULLIF(BTRIM(tac.agent_entity_id), ''), 'default') = ${bind_index}"
            ));
            binds.push(entity_id.to_string());
            bind_index += 1;
        }
        if let Some(run_id) = agent_run_id.map(str::trim).filter(|value| !value.is_empty()) {
            query.push_str(&format!(" AND NULLIF(BTRIM(tac.agent_run_id), '') = ${bind_index}"));
            binds.push(run_id.to_string());
            bind_index += 1;
        }

        query.push_str(&format!(
            " ORDER BY COALESCE(tac.updated_at, CURRENT_TIMESTAMP) DESC, t.todo_id DESC LIMIT ${bind_index} OFFSET ${}",
            bind_index + 1
        ));

        let mut sql = sqlx::query(&query);
        for bind in binds {
            sql = sql.bind(bind);
        }
        let rows = sql
            .bind(limit.max(1))
            .bind(offset.max(0))
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        self.inc_metric(
            |metrics| &mut metrics.find_todo_ids_duration_ms_total,
            Self::duration_ms(started),
        );

        Ok(rows
            .into_iter()
            .filter_map(|row| row.try_get::<Option<String>, _>("todo_id").ok().flatten())
            .collect())
    }

    fn get_metrics_snapshot(&self) -> HashMap<String, serde_json::Value> {
        let snapshot = self.metrics.lock().ok();
        let fallback = QueryMetrics::default();
        let metrics = snapshot.as_deref().unwrap_or(&fallback);

        let get_calls = metrics.get_calls;
        let batch_calls = metrics.batch_get_calls;
        let requested_total = metrics.batch_get_requested_ids_total;
        let find_calls = metrics.find_todo_ids_calls;

        let get_context_hit_ratio = if get_calls > 0.0 {
            metrics.get_context_hits / get_calls
        } else {
            0.0
        };
        let batch_get_context_hit_ratio = if requested_total > 0.0 {
            metrics.batch_get_context_hits / requested_total
        } else {
            0.0
        };
        HashMap::from([
            ("get_calls".into(), serde_json::json!(metrics.get_calls)),
            ("get_context_hits".into(), serde_json::json!(metrics.get_context_hits)),
            ("get_misses".into(), serde_json::json!(metrics.get_misses)),
            (
                "get_duration_ms_total".into(),
                serde_json::json!(metrics.get_duration_ms_total),
            ),
            ("get_context_hit_ratio".into(), serde_json::json!(get_context_hit_ratio)),
            (
                "get_avg_duration_ms".into(),
                serde_json::json!(if get_calls > 0.0 {
                    metrics.get_duration_ms_total / get_calls
                } else {
                    0.0
                }),
            ),
            ("batch_get_calls".into(), serde_json::json!(metrics.batch_get_calls)),
            (
                "batch_get_requested_ids_total".into(),
                serde_json::json!(metrics.batch_get_requested_ids_total),
            ),
            (
                "batch_get_context_hits".into(),
                serde_json::json!(metrics.batch_get_context_hits),
            ),
            (
                "batch_get_duration_ms_total".into(),
                serde_json::json!(metrics.batch_get_duration_ms_total),
            ),
            (
                "batch_get_context_hit_ratio".into(),
                serde_json::json!(batch_get_context_hit_ratio),
            ),
            (
                "batch_get_avg_duration_ms".into(),
                serde_json::json!(if batch_calls > 0.0 {
                    metrics.batch_get_duration_ms_total / batch_calls
                } else {
                    0.0
                }),
            ),
            (
                "find_todo_ids_calls".into(),
                serde_json::json!(metrics.find_todo_ids_calls),
            ),
            (
                "find_todo_ids_context_preferred_calls".into(),
                serde_json::json!(metrics.find_todo_ids_context_preferred_calls),
            ),
            (
                "find_todo_ids_hybrid_calls".into(),
                serde_json::json!(metrics.find_todo_ids_hybrid_calls),
            ),
            (
                "find_todo_ids_duration_ms_total".into(),
                serde_json::json!(metrics.find_todo_ids_duration_ms_total),
            ),
            (
                "find_todo_ids_avg_duration_ms".into(),
                serde_json::json!(if find_calls > 0.0 {
                    metrics.find_todo_ids_duration_ms_total / find_calls
                } else {
                    0.0
                }),
            ),
            (
                "find_todo_ids_context_preferred_ratio".into(),
                serde_json::json!(if find_calls > 0.0 {
                    metrics.find_todo_ids_context_preferred_calls / find_calls
                } else {
                    0.0
                }),
            ),
            (
                "find_todo_ids_hybrid_ratio".into(),
                serde_json::json!(if find_calls > 0.0 {
                    metrics.find_todo_ids_hybrid_calls / find_calls
                } else {
                    0.0
                }),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn no_legacy_metric_fields_remain() {
        let source = include_str!("pg_todo_agent_context_repository.rs");
        let test_marker = "#[cfg(test)]";
        let main_code = &source[..source.find(test_marker).unwrap_or(source.len())];
        assert!(
            !main_code.contains("fn get_legacy_hits"),
            "legacy metric code should be removed"
        );
        assert!(
            !main_code.contains("fn batch_get_legacy_hits"),
            "legacy metric code should be removed"
        );
        assert!(
            !main_code.contains("find_todo_ids_legacy_preferred_calls"),
            "legacy metric code should be removed"
        );
        assert!(
            !main_code.contains("legacy_retired"),
            "legacy_retired field should be removed"
        );
    }
}
