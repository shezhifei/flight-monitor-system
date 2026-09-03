//! PostgreSQL AI 实体配置仓储

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::ai_entity_config::AiEntityConfigRecord;
use fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository;

use super::soft_delete_audit::record_soft_delete;
use crate::security::ai_config_crypto::AiConfigCrypto;

pub struct PgAiEntityConfigRepository {
    pool: PgPool,
    crypto: AiConfigCrypto,
}

impl PgAiEntityConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            crypto: AiConfigCrypto::from_env(),
        }
    }

    async fn ensure_schema(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ai_entities (
                id TEXT PRIMARY KEY,
                config JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                deleted_at TIMESTAMPTZ
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        // 幂等播种（ai_entities 单写者收口，ADR-0004）："default" 与
        // "todo_graph_pilot" 均由此处负责；种子 api_key 为空，无需加密。
        for (entity_id, seed) in [
            ("default", default_config_value()),
            ("todo_graph_pilot", pilot_seed_config_value()),
        ] {
            sqlx::query(
                r#"
                INSERT INTO ai_entities (id, config)
                VALUES ($1, $2::jsonb)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(entity_id)
            .bind(seed.to_string())
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        }

        Ok(())
    }

    fn row_to_record(&self, row: &sqlx::postgres::PgRow) -> Result<AiEntityConfigRecord, DomainError> {
        let mut config: serde_json::Value = row
            .try_get("config")
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        // 读出即解密并剥离内部标记，上层（application/api）只接触明文配置。
        self.crypto.decrypt_config(&mut config);
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
            WHERE deleted_at IS NULL
            ORDER BY id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        rows.iter().map(|row| self.row_to_record(row)).collect()
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<AiEntityConfigRecord>, DomainError> {
        self.ensure_schema().await?;
        let row = sqlx::query(
            r#"
            SELECT id, config, created_at, updated_at
            FROM ai_entities
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        row.as_ref().map(|row| self.row_to_record(row)).transpose()
    }

    async fn save(&self, id: &str, config: &serde_json::Value) -> Result<AiEntityConfigRecord, DomainError> {
        self.ensure_schema().await?;
        // 写入前加密 api_key（与 Python ConfigEncryptor 格式兼容的 fernet_v1）；
        // 未配置密钥且含 api_key 时 fail-closed，避免明文落库。
        let mut stored = config.clone();
        self.crypto.encrypt_config(&mut stored)?;
        let now = Utc::now();
        let row = sqlx::query(
            r#"
            INSERT INTO ai_entities (id, config, updated_at)
            VALUES ($1, $2::jsonb, $3)
            ON CONFLICT (id) DO UPDATE SET
                config = EXCLUDED.config,
                deleted_at = NULL,
                updated_at = EXCLUDED.updated_at
            RETURNING id, config, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(stored.to_string())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        self.row_to_record(&row)
    }

    async fn delete(&self, id: &str) -> Result<bool, DomainError> {
        self.ensure_schema().await?;
        // 审计要求软删除：仅标记 deleted_at，行保留
        let result = sqlx::query(
            "UPDATE ai_entities SET deleted_at = NOW(), updated_at = NOW() \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        let deleted = result.rows_affected() > 0;
        if deleted {
            record_soft_delete(&self.pool, "ai_entity", id, "soft_delete").await;
        }
        Ok(deleted)
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

/// `todo_graph_pilot` 实体种子，与 Python sidecar 历史播种
/// （`config_normalizer.default_entity_document()`）保持一致。
/// 仅含空 `api_key`，加密器对其为 no-op。
fn pilot_seed_config_value() -> serde_json::Value {
    serde_json::json!({
        "config_version": 2,
        "providers": {
            "default": {
                "type": "openai_compatible",
                "base_url": "https://api.openai.com/v1",
                "api_key": "",
                "api_format": "chat_completions",
                "timeout": 30.0,
                "max_retries": 3,
                "retry_delay": 0.5
            }
        },
        "model_routing": {"default": "gpt-4o", "chat": "gpt-4o"},
        "models": {},
        "temperature": 0.7,
        "max_tokens": 2000,
        "top_p": 0.95,
        "frequency_penalty": 0.0,
        "presence_penalty": 0.0,
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
        "media": {
            "asr": {"model": "whisper-1", "language": null, "response_format": "json"},
            "tts": {"model": "tts-1", "voice": "alloy", "response_format": "mp3", "speed": 1.0},
            "realtime": {
                "enabled": false,
                "provider": null,
                "asr_streaming_model": null,
                "tts_streaming_model": null,
                "input_sample_rate_hz": 16000,
                "output_sample_rate_hz": 24000,
                "chunk_ms": 40,
                "latency_budget_ms": 800,
                "vad_enabled": true,
                "barge_in_enabled": true,
                "max_session_seconds": 300,
                "max_frame_bytes": 65536
            }
        },
        "endpoints": {"chat": null, "vision": null, "asr": null, "tts": null},
        "tooling": {
            "enabled": true,
            "max_rounds": 5,
            "allow_parallel": false,
            "allowed_tool_sources": ["builtin"],
            "allowed_tool_categories": [
                "flight", "flight_event", "query", "anomaly", "todo", "business_case", "ontology"
            ],
            "allowed_tools": null,
            "denied_tools": ["sql_query_readonly"],
            "write_action_policy": "proposal_only"
        },
        "mcp": {"enabled": false, "servers": []},
        "skills": {"enabled": false, "allowlist": [], "bindings": []},
        "subagents": {"enabled": false, "allowed_entity_ids": []},
        "context_policy": {
            "strategy": "hybrid",
            "max_context_tokens": 64000,
            "compression_threshold_tokens": 48000,
            "preserve_recent_messages": 12
        },
        "cache_policy": {
            "enabled": true,
            "provider_prompt_cache": {
                "enabled": false,
                "retention": null,
                "key_namespace": "flight_monitor"
            }
        },
        "security": {"mask_sensitive": true, "log_prompts": false},
        "todo_agent_graph_enabled": false,
        "todo_agent_graph_runtime_enabled": false,
        "graph_runtime_enabled": false,
        "system_prompt": "你是一个航班监控系统的AI助手，可以帮助用户查询航班信息、管理航班事件和待办事项。",
        "task_template": null
    })
}
