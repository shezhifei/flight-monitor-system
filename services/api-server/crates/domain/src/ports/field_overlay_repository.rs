use crate::error::DomainError;
use crate::models::field_overlay::FieldOverlay;
use async_trait::async_trait;

#[async_trait]
pub trait FieldOverlayRepository: Send + Sync {
    async fn list(&self, object_name: Option<&str>, include_inactive: bool) -> Result<Vec<FieldOverlay>, DomainError>;
    async fn find(&self, object_name: &str, field_name: &str) -> Result<Option<FieldOverlay>, DomainError>;
    async fn save(&self, overlay: &FieldOverlay) -> Result<FieldOverlay, DomainError>;
    async fn set_active(
        &self,
        object_name: &str,
        field_name: &str,
        is_active: bool,
    ) -> Result<Option<FieldOverlay>, DomainError>;

    /// Validate a catalog reference without coupling the application layer to
    /// a concrete metadata-catalog repository. Test doubles may keep the
    /// default permissive behavior; the PostgreSQL adapter overrides it.
    async fn catalog_entry_is_active(&self, _catalog_code: &str, _entry_code: &str) -> Result<bool, DomainError> {
        Ok(true)
    }
}
