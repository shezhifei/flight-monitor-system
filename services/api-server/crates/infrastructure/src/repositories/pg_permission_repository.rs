//! PostgreSQL 权限仓储实现

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::user::Permission;
use fms_domain::ports::user_repository::PermissionRepository;

pub struct PgPermissionRepository {
    pool: PgPool,
}

impl PgPermissionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_permission(r: &sqlx::postgres::PgRow) -> Permission {
        Permission {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            is_active: r.get("is_active"),
        }
    }
}

#[async_trait]
impl PermissionRepository for PgPermissionRepository {
    async fn find_all(&self) -> Result<Vec<Permission>, DomainError> {
        let rows = sqlx::query("SELECT * FROM permissions WHERE is_active = TRUE ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(Self::row_to_permission).collect())
    }

    async fn find_by_role_id(&self, role_id: &str) -> Result<Vec<Permission>, DomainError> {
        let rows = sqlx::query(
            r#"SELECT p.* FROM permissions p
               JOIN role_permissions rp ON p.id = rp.permission_id
               WHERE rp.role_id = $1 AND p.is_active = TRUE"#,
        )
        .bind(role_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(Self::row_to_permission).collect())
    }
}
