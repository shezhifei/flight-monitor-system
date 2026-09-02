use async_trait::async_trait;

use crate::error::DomainError;
use crate::models::metadata_catalog::{MetadataCatalog, MetadataCatalogEntry};

#[async_trait]
pub trait MetadataCatalogRepository: Send + Sync {
    async fn save_catalog(&self, catalog: &MetadataCatalog) -> Result<MetadataCatalog, DomainError>;
    async fn find_catalog(&self, code: &str) -> Result<Option<MetadataCatalog>, DomainError>;
    async fn list_catalogs(&self, include_inactive: bool) -> Result<Vec<MetadataCatalog>, DomainError>;
    async fn set_catalog_active(&self, code: &str, is_active: bool) -> Result<Option<MetadataCatalog>, DomainError>;

    async fn save_entry(&self, entry: &MetadataCatalogEntry) -> Result<MetadataCatalogEntry, DomainError>;
    async fn find_entry(&self, catalog_code: &str, code: &str) -> Result<Option<MetadataCatalogEntry>, DomainError>;
    async fn list_entries(
        &self,
        catalog_code: &str,
        include_inactive: bool,
    ) -> Result<Vec<MetadataCatalogEntry>, DomainError>;
    async fn set_entry_active(
        &self,
        catalog_code: &str,
        code: &str,
        is_active: bool,
    ) -> Result<Option<MetadataCatalogEntry>, DomainError>;

    /// Open-catalog ingest: insert if missing, keep existing rank/name if already present.
    async fn upsert_ingest_entry(
        &self,
        catalog_code: &str,
        code: &str,
        name: &str,
    ) -> Result<MetadataCatalogEntry, DomainError>;
}
