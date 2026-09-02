use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::ontology_attribute_reference_repository::{
    OntologyAttributeReferenceRepository, OntologyAttributeReferenceTransactionalRepository,
};

pub struct PgOntologyAttributeReferenceRepository {
    pool: PgPool,
}

impl PgOntologyAttributeReferenceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// crate 内共享的事务写入实现：供同 crate 仓储（如派工单原子创建）
    /// 把引用投影并入自己的事务。
    pub(crate) async fn replace_owner_references_in_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        owner_object_name: &str,
        owner_object_id: &str,
        references: &[OntologyAttributeReference],
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE ontology_attribute_references SET is_active = FALSE WHERE owner_object_name = $1 AND owner_object_id = $2 AND is_active",
        )
        .bind(owner_object_name)
        .bind(owner_object_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        for reference in references {
            sqlx::query(
                "INSERT INTO ontology_attribute_references (owner_object_name, owner_object_id, field_name, target_object_name, target_key, is_active) VALUES ($1, $2, $3, $4, $5, TRUE) ON CONFLICT DO NOTHING",
            )
            .bind(&reference.owner_object_name)
            .bind(&reference.owner_object_id)
            .bind(&reference.field_name)
            .bind(&reference.target_object_name)
            .bind(&reference.target_key)
            .execute(&mut **tx)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        }
        Ok(())
    }
}

#[async_trait]
impl OntologyAttributeReferenceRepository for PgOntologyAttributeReferenceRepository {
    async fn replace_owner_references(
        &self,
        owner_object_name: &str,
        owner_object_id: &str,
        references: &[OntologyAttributeReference],
    ) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Self::replace_owner_references_in_transaction(&mut tx, owner_object_name, owner_object_id, references).await?;
        tx.commit()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))
    }

    async fn find_by_target(
        &self,
        target_object_name: &str,
        target_key: &str,
    ) -> Result<Vec<OntologyAttributeReference>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, owner_object_name, owner_object_id, field_name, target_object_name, target_key, created_at FROM ontology_attribute_references WHERE target_object_name = $1 AND target_key = $2 AND is_active ORDER BY owner_object_name, owner_object_id, field_name",
        )
        .bind(target_object_name)
        .bind(target_key)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| OntologyAttributeReference {
                id: row.try_get("id").ok(),
                owner_object_name: row.get("owner_object_name"),
                owner_object_id: row.get("owner_object_id"),
                field_name: row.get("field_name"),
                target_object_name: row.get("target_object_name"),
                target_key: row.get("target_key"),
                created_at: row.try_get("created_at").ok(),
            })
            .collect())
    }
}

#[async_trait]
impl<'tx> OntologyAttributeReferenceTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>>
    for PgOntologyAttributeReferenceRepository
{
    async fn replace_owner_references_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        owner_object_name: &str,
        owner_object_id: &str,
        references: &[OntologyAttributeReference],
    ) -> Result<(), DomainError> {
        Self::replace_owner_references_in_transaction(tx, owner_object_name, owner_object_id, references).await
    }

    async fn find_by_target_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        target_object_name: &str,
        target_key: &str,
    ) -> Result<Vec<OntologyAttributeReference>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, owner_object_name, owner_object_id, field_name, target_object_name, target_key, created_at FROM ontology_attribute_references WHERE target_object_name = $1 AND target_key = $2 AND is_active ORDER BY owner_object_name, owner_object_id, field_name",
        )
        .bind(target_object_name)
        .bind(target_key)
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| OntologyAttributeReference {
                id: row.try_get("id").ok(),
                owner_object_name: row.get("owner_object_name"),
                owner_object_id: row.get("owner_object_id"),
                field_name: row.get("field_name"),
                target_object_name: row.get("target_object_name"),
                target_key: row.get("target_key"),
                created_at: row.try_get("created_at").ok(),
            })
            .collect())
    }
}
