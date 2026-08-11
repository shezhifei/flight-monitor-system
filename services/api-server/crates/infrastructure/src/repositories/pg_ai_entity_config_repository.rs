//! PostgreSQL AI 实体配置仓储

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::ai_entity_config::AiEntityConfigRecord;
use fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository;

pub struct PgAiEntityConfigRepository {
    pool: PgPool,
}

impl PgAiEntityConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn ensure_schema(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ai_entities (
                id TEXT PRIMARY KEY,
                config JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO ai_entities (id, config)
            VALUES ($1, $2::jsonb)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind("default")
        .bind(default_config_value().to_string())
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(())
    }

    fn row_to_record(row: &sqlx::postgres::PgRow) -> Result<AiEntityConfigRecord, DomainError> {
        let config: serde_json::Value = row
            .try_get("config")
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(AiEntityConfigRecord {
            id: row
                .try_get("id")
                .map_err(|error| DomainError::Internal(error.to_string()))?,
            config,
            created_at: row
                .try_get("created_at")
                .map_err(|error| DomainError::Internal(error.to_string()))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|error| DomainError::Internal(error.to_string()))?,
        })
    }
}

#[async_trait]
impl AiEntityConfigRepository for PgAiEntityConfigRepository {
    async fn find_all(&self) -> Result<Vec<AiEntityConfigRecord>, DomainError> {
        self.ensure_schema().await?;
        let rows = sqlx::query(
            r#"
            SELECT id, config, created_at, updated_at
            FROM ai_entities
            ORDER BY id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        rows.iter().map(Self::row_to_record).collect()
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<AiEntityConfigRecord>, DomainError> {
        self.ensure_schema().await?;
        let row = sqlx::query(
            r#"
            SELECT id, config, created_at, updated_at
            FROM ai_entities
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        row.as_ref().map(Self::row_to_record).transpose()
    }

    async fn save(&self, id: &str, config: &serde_json::Value) -> Result<AiEntityConfigRecord, DomainError> {
        self.ensure_schema().await?;
        let now = Utc::now();
        let row = sqlx::query(
            r#"
            INSERT INTO ai_entities (id, config, updated_at)
            VALUES ($1, $2::jsonb, $3)
            ON CONFLICT (id) DO UPDATE SET
                config = EXCLUDED.config,
                updated_at = EXCLUDED.updated_at
            RETURNING id, config, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(config.to_string())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Self::row_to_record(&row)
    }

    async fn delete(&self, id: &str) -> Result<bool, DomainError> {
        self.ensure_schema().await?;
        let result = sqlx::query("DELETE FROM ai_entities WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}

fn default_config_value() -> serde_json::Value {
    serde_json::json!({
        "api_key": "",
        "base_url": "https://api.openai.com/v1",
        "default_model": "gpt-3.5-turbo",
        "api_format": "chat_completions",
        "temperature": 0.7,
        "max_tokens": 2000,
        "top_p": 0.95,
        "frequency_penalty": 0.0,
        "presence_penalty": 0.0,
        "timeout": 30.0,
        "max_retries": 3,
        "retry_delay": 0.5,
        "cost_per_1k_input": 0.0015,
        "cost_per_1k_output": 0.002,
        "context_window": 128000,
        "tools": {
            "timeout": 30,
            "max_retries": 3,
            "retry_delay": 1.0,
            "auto_execute": true
        },
        "monitoring": {
            "metrics_enabled": true,
            "trace_enabled": false,
            "log_prompts": false,
            "mask_sensitive": true
        },
        "endpoints": {
            "chat": null,
            "vision": null,
            "asr": null,
            "tts": null
        },
        "allowed_tool_categories": ["flight", "flight_event", "todo", "business_case"],
        "allowed_tools": null,
        "denied_tools": [],
        "system_prompt": "你是一个航班监控系统的AI助手，可以帮助用户查询航班信息、管理航班事件和待办事项。",
        "task_template": null
    })
}
