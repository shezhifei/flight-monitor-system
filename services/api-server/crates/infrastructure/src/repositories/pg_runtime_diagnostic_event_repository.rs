use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::ports::runtime_diagnostic_repository::RuntimeDiagnosticRepository;
use fms_domain::ports::runtime_diagnostic_sink::RuntimeDiagnosticSink;
use serde_json::Value;
use sqlx::{types::Json, PgPool, Row};

pub struct PgRuntimeDiagnosticEventRepository {
    pool: PgPool,
}

impl PgRuntimeDiagnosticEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn from_pool_ref(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn insert(
        &self,
        topic: &str,
        event_type: &str,
        payload: Value,
        event_id: Option<String>,
    ) -> Result<(), sqlx::Error> {
        let id = event_id.unwrap_or_else(|| ulid::Ulid::new().to_string());
        sqlx::query(
            "INSERT INTO runtime_diagnostic_events (event_id, topic, event_type, payload) VALUES ($1,$2,$3,$4) ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(id)
        .bind(topic)
        .bind(event_type)
        .bind(Json(payload))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn fetch_recent(&self, topic: &str, limit: i64) -> Result<Vec<Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT payload FROM runtime_diagnostic_events WHERE topic=$1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(topic)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| r.try_get::<Json<Value>, _>("payload").map(|v| v.0))
            .collect()
    }

    pub async fn count_by_topic(&self, topic: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT COUNT(*) FROM runtime_diagnostic_events WHERE topic=$1")
            .bind(topic)
            .fetch_one(&self.pool)
            .await
    }

    pub async fn ping(&self) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map(|_| true)
    }
}

#[async_trait]
impl RuntimeDiagnosticRepository for PgRuntimeDiagnosticEventRepository {
    async fn fetch_recent(&self, topic: &str, limit: i64) -> Result<Vec<Value>, DomainError> {
        PgRuntimeDiagnosticEventRepository::fetch_recent(self, topic, limit)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))
    }

    async fn count_by_topic(&self, topic: &str) -> Result<i64, DomainError> {
        PgRuntimeDiagnosticEventRepository::count_by_topic(self, topic)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))
    }

    async fn ping(&self) -> Result<bool, DomainError> {
        PgRuntimeDiagnosticEventRepository::ping(self)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))
    }
}

#[async_trait]
impl RuntimeDiagnosticSink for PgRuntimeDiagnosticEventRepository {
    async fn insert(&self, topic: &str, event_type: &str, payload: Value, correlation_id: Option<String>) {
        let _ = PgRuntimeDiagnosticEventRepository::insert(self, topic, event_type, payload, correlation_id).await;
    }
}
