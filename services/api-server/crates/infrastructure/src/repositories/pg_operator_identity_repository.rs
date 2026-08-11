//! PostgreSQL 操作员身份上下文仓储实现

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::operator_identity::OperatorIdentityContext;
use fms_domain::ports::operator_identity_repository::OperatorIdentityRepository;

pub struct PgOperatorIdentityRepository {
    pool: PgPool,
}

impl PgOperatorIdentityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_context(row: &sqlx::postgres::PgRow) -> OperatorIdentityContext {
        OperatorIdentityContext {
            user_id: row.get("user_id"),
            context_type: row.get("context_type"),
            context_id: row.get("context_id"),
            operator_name: row.get("operator_name"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }
}

#[async_trait]
impl OperatorIdentityRepository for PgOperatorIdentityRepository {
    async fn find_by_scope(
        &self,
        user_id: &str,
        context_type: &str,
        context_id: &str,
    ) -> Result<Option<OperatorIdentityContext>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT *
            FROM operator_identity_contexts
            WHERE user_id = $1 AND context_type = $2 AND context_id = $3
            "#,
        )
        .bind(user_id)
        .bind(context_type)
        .bind(context_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(row.as_ref().map(Self::row_to_context))
    }

    async fn upsert(&self, context: &OperatorIdentityContext) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO operator_identity_contexts (
                user_id, context_type, context_id, operator_name, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id, context_type, context_id) DO UPDATE SET
                operator_name = EXCLUDED.operator_name,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(&context.user_id)
        .bind(&context.context_type)
        .bind(&context.context_id)
        .bind(&context.operator_name)
        .bind(context.created_at)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn delete(&self, user_id: &str, context_type: &str, context_id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            DELETE FROM operator_identity_contexts
            WHERE user_id = $1 AND context_type = $2 AND context_id = $3
            "#,
        )
        .bind(user_id)
        .bind(context_type)
        .bind(context_id)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}
