use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::metadata_catalog::{
    normalize_catalog_code, normalize_entry_code, CatalogEntrySource, MetadataCatalog, MetadataCatalogEntry,
    CATALOG_AIRCRAFT_TYPE,
};
use fms_domain::ports::metadata_catalog_repository::MetadataCatalogRepository;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataCatalogCreate {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub is_open: bool,
    #[serde(default)]
    pub is_ordered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataCatalogUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_open: Option<bool>,
    pub is_ordered: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataCatalogEntryCreate {
    pub code: String,
    pub name: String,
    pub rank: Option<i32>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataCatalogEntryUpdate {
    pub name: Option<String>,
    pub rank: Option<i32>,
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetadataCatalogWithEntries {
    #[serde(flatten)]
    pub catalog: MetadataCatalog,
    pub entries: Vec<MetadataCatalogEntry>,
}

pub struct MetadataCatalogService<R>
where
    R: MetadataCatalogRepository + ?Sized,
{
    repo: Arc<R>,
}

impl<R> MetadataCatalogService<R>
where
    R: MetadataCatalogRepository + ?Sized,
{
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    pub async fn list_catalogs(&self, include_inactive: bool) -> Result<Vec<MetadataCatalog>, DomainError> {
        self.repo.list_catalogs(include_inactive).await
    }

    pub async fn get_catalog(
        &self,
        code: &str,
        include_inactive_entries: bool,
    ) -> Result<MetadataCatalogWithEntries, DomainError> {
        let catalog = self.require_catalog(code).await?;
        let entries = self.repo.list_entries(&catalog.code, include_inactive_entries).await?;
        Ok(MetadataCatalogWithEntries { catalog, entries })
    }

    pub async fn create_catalog(&self, payload: MetadataCatalogCreate) -> Result<MetadataCatalog, DomainError> {
        let code = normalize_catalog_code(&payload.code).map_err(DomainError::ValidationError)?;
        if self.repo.find_catalog(&code).await?.is_some() {
            return Err(DomainError::Conflict(format!("码表已存在: {code}")));
        }
        let name = payload.name.trim().to_string();
        if name.is_empty() {
            return Err(DomainError::ValidationError("码表名称不能为空".into()));
        }
        let catalog = MetadataCatalog {
            code,
            name,
            description: payload.description.filter(|s| !s.trim().is_empty()),
            is_open: payload.is_open,
            is_ordered: payload.is_ordered,
            system_owned: false,
            is_active: true,
            created_at: None,
            updated_at: None,
        };
        self.repo.save_catalog(&catalog).await
    }

    pub async fn update_catalog(
        &self,
        code: &str,
        payload: MetadataCatalogUpdate,
    ) -> Result<MetadataCatalog, DomainError> {
        let mut catalog = self.require_catalog(code).await?;
        if let Some(name) = payload.name {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(DomainError::ValidationError("码表名称不能为空".into()));
            }
            catalog.name = name;
        }
        if let Some(description) = payload.description {
            catalog.description = Some(description).filter(|s| !s.trim().is_empty());
        }
        if let Some(is_open) = payload.is_open {
            catalog.is_open = is_open;
        }
        if let Some(is_ordered) = payload.is_ordered {
            catalog.is_ordered = is_ordered;
        }
        self.repo.save_catalog(&catalog).await
    }

    pub async fn set_catalog_active(&self, code: &str, is_active: bool) -> Result<MetadataCatalog, DomainError> {
        let catalog = self.require_catalog(code).await?;
        if catalog.system_owned && !is_active {
            return Err(DomainError::Conflict("系统码表不能停用".into()));
        }
        self.repo
            .set_catalog_active(&catalog.code, is_active)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "metadata_catalog",
                id: catalog.code,
            })
    }

    pub async fn create_entry(
        &self,
        catalog_code: &str,
        payload: MetadataCatalogEntryCreate,
    ) -> Result<MetadataCatalogEntry, DomainError> {
        let catalog = self.require_catalog(catalog_code).await?;
        if !catalog.is_active {
            return Err(DomainError::Conflict("码表已停用，不能新增项".into()));
        }
        let code = normalize_entry_code(&payload.code).map_err(DomainError::ValidationError)?;
        if self.repo.find_entry(&catalog.code, &code).await?.is_some() {
            return Err(DomainError::Conflict(format!("码表项已存在: {code}")));
        }
        let name = payload.name.trim().to_string();
        if name.is_empty() {
            return Err(DomainError::ValidationError("码表项名称不能为空".into()));
        }
        let entry = MetadataCatalogEntry {
            catalog_code: catalog.code,
            code,
            name,
            rank: payload.rank,
            payload: if payload.payload.is_null() {
                serde_json::json!({})
            } else {
                payload.payload
            },
            is_active: true,
            source: CatalogEntrySource::Manual,
            created_at: None,
            updated_at: None,
        };
        self.repo.save_entry(&entry).await
    }

    pub async fn update_entry(
        &self,
        catalog_code: &str,
        code: &str,
        payload: MetadataCatalogEntryUpdate,
    ) -> Result<MetadataCatalogEntry, DomainError> {
        let catalog = self.require_catalog(catalog_code).await?;
        let entry_code = normalize_entry_code(code).map_err(DomainError::ValidationError)?;
        let mut entry = self
            .repo
            .find_entry(&catalog.code, &entry_code)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "metadata_catalog_entry",
                id: format!("{}.{entry_code}", catalog.code),
            })?;
        if let Some(name) = payload.name {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(DomainError::ValidationError("码表项名称不能为空".into()));
            }
            entry.name = name;
        }
        if payload.rank.is_some() {
            entry.rank = payload.rank;
        }
        if let Some(json) = payload.payload {
            entry.payload = if json.is_null() { serde_json::json!({}) } else { json };
        }
        self.repo.save_entry(&entry).await
    }

    pub async fn set_entry_active(
        &self,
        catalog_code: &str,
        code: &str,
        is_active: bool,
    ) -> Result<MetadataCatalogEntry, DomainError> {
        let catalog = self.require_catalog(catalog_code).await?;
        let entry_code = normalize_entry_code(code).map_err(DomainError::ValidationError)?;
        self.repo
            .set_entry_active(&catalog.code, &entry_code, is_active)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "metadata_catalog_entry",
                id: format!("{}.{entry_code}", catalog.code),
            })
    }

    /// Telegram/import: write the raw type string as an open-catalog row. Closed catalogs no-op.
    pub async fn ingest_aircraft_type(&self, raw: &str) -> Result<Option<MetadataCatalogEntry>, DomainError> {
        let Ok(code) = normalize_entry_code(raw) else {
            return Ok(None);
        };
        let catalog = match self.repo.find_catalog(CATALOG_AIRCRAFT_TYPE).await? {
            Some(c) if c.is_active && c.is_open => c,
            _ => return Ok(None),
        };
        let saved = self.repo.upsert_ingest_entry(&catalog.code, &code, &code).await?;
        Ok(Some(saved))
    }

    async fn require_catalog(&self, code: &str) -> Result<MetadataCatalog, DomainError> {
        let code = normalize_catalog_code(code).map_err(DomainError::ValidationError)?;
        self.repo
            .find_catalog(&code)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "metadata_catalog",
                id: code,
            })
    }
}
