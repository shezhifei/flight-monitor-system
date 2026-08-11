//! Database metadata port.
//!
//! Abstracts metadata queries (relation existence / current database name)
//! so application services do not issue raw `sqlx::query_scalar` against the
//! catalog. Implemented by `PgDatabaseMetadataAdapter` in infrastructure.

use async_trait::async_trait;

use crate::error::DomainError;

/// Read-only metadata access port for catalog introspection.
#[async_trait]
pub trait DatabaseMetadataPort: Send + Sync {
    /// Returns `true` if a relation (table/view) named `qualified_name`
    /// (e.g. `public.ai_action_proposals`) exists in the current database.
    async fn relation_exists(&self, qualified_name: &str) -> Result<bool, DomainError>;

    /// Returns the name of the current database.
    ///
    /// Used by smoke-cleanup guards to verify the connected database is not a
    /// production instance.
    async fn current_database_name(&self) -> Result<String, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn assert_object_safe(_: &dyn DatabaseMetadataPort) {}
        struct Stub;
        #[async_trait]
        impl DatabaseMetadataPort for Stub {
            async fn relation_exists(&self, _: &str) -> Result<bool, DomainError> {
                Ok(true)
            }
            async fn current_database_name(&self) -> Result<String, DomainError> {
                Ok("test_db".to_string())
            }
        }
        assert_object_safe(&Stub);
    }
}
