use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::field_overlay::FieldOverlay;
use fms_domain::models::metadata_catalog::{CatalogEntrySource, MetadataCatalog, MetadataCatalogEntry};
use fms_domain::ports::field_overlay_repository::FieldOverlayRepository;
use fms_domain::ports::metadata_catalog_repository::MetadataCatalogRepository;

pub struct PgMetadataCatalogRepository {
    pool: PgPool,
}

impl PgMetadataCatalogRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn catalog_from_row(row: &sqlx::postgres::PgRow) -> MetadataCatalog {
    MetadataCatalog {
        code: row.get("code"),
        name: row.get("name"),
        description: row.get("description"),
        is_open: row.get("is_open"),
        is_ordered: row.get("is_ordered"),
        system_owned: row.get("system_owned"),
        is_active: row.get("is_active"),
        created_at: row.get::<Option<DateTime<Utc>>, _>("created_at"),
        updated_at: row.get::<Option<DateTime<Utc>>, _>("updated_at"),
    }
}

fn entry_from_row(row: &sqlx::postgres::PgRow) -> MetadataCatalogEntry {
    let source: String = row.get("source");
    MetadataCatalogEntry {
        catalog_code: row.get("catalog_code"),
        code: row.get("code"),
        name: row.get("name"),
        rank: row.get("rank"),
        payload: row
            .try_get::<serde_json::Value, _>("payload")
            .unwrap_or_else(|_| serde_json::json!({})),
        is_active: row.get("is_active"),
        source: CatalogEntrySource::parse(&source),
        created_at: row.get::<Option<DateTime<Utc>>, _>("created_at"),
        updated_at: row.get::<Option<DateTime<Utc>>, _>("updated_at"),
    }
}

#[async_trait]
impl MetadataCatalogRepository for PgMetadataCatalogRepository {
    async fn save_catalog(&self, catalog: &MetadataCatalog) -> Result<MetadataCatalog, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO metadata_catalogs (
                code, name, description, is_open, is_ordered, system_owned, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (code) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                is_open = EXCLUDED.is_open,
                is_ordered = EXCLUDED.is_ordered,
                is_active = EXCLUDED.is_active,
                updated_at = NOW()
            "#,
        )
        .bind(&catalog.code)
        .bind(&catalog.name)
        .bind(&catalog.description)
        .bind(catalog.is_open)
        .bind(catalog.is_ordered)
        .bind(catalog.system_owned)
        .bind(catalog.is_active)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_catalog(&catalog.code)
            .await?
            .ok_or_else(|| DomainError::Internal("catalog save returned no row".into()))
    }

    async fn find_catalog(&self, code: &str) -> Result<Option<MetadataCatalog>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT code, name, description, is_open, is_ordered, system_owned, is_active, created_at, updated_at
            FROM metadata_catalogs
            WHERE code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.as_ref().map(catalog_from_row))
    }

    async fn list_catalogs(&self, include_inactive: bool) -> Result<Vec<MetadataCatalog>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT code, name, description, is_open, is_ordered, system_owned, is_active, created_at, updated_at
            FROM metadata_catalogs
            WHERE ($1 OR is_active = TRUE)
            ORDER BY code ASC
            "#,
        )
        .bind(include_inactive)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(rows.iter().map(catalog_from_row).collect())
    }

    async fn set_catalog_active(&self, code: &str, is_active: bool) -> Result<Option<MetadataCatalog>, DomainError> {
        sqlx::query(
            r#"
            UPDATE metadata_catalogs
            SET is_active = $2, updated_at = NOW()
            WHERE code = $1
            "#,
        )
        .bind(code)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        self.find_catalog(code).await
    }

    async fn save_entry(&self, entry: &MetadataCatalogEntry) -> Result<MetadataCatalogEntry, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO metadata_catalog_entries (
                catalog_code, code, name, rank, payload, is_active, source
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (catalog_code, code) DO UPDATE SET
                name = EXCLUDED.name,
                rank = EXCLUDED.rank,
                payload = EXCLUDED.payload,
                is_active = EXCLUDED.is_active,
                source = EXCLUDED.source,
                updated_at = NOW()
            "#,
        )
        .bind(&entry.catalog_code)
        .bind(&entry.code)
        .bind(&entry.name)
        .bind(entry.rank)
        .bind(&entry.payload)
        .bind(entry.is_active)
        .bind(entry.source.as_str())
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_entry(&entry.catalog_code, &entry.code)
            .await?
            .ok_or_else(|| DomainError::Internal("catalog entry save returned no row".into()))
    }

    async fn find_entry(&self, catalog_code: &str, code: &str) -> Result<Option<MetadataCatalogEntry>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT catalog_code, code, name, rank, payload, is_active, source, created_at, updated_at
            FROM metadata_catalog_entries
            WHERE catalog_code = $1 AND code = $2
            "#,
        )
        .bind(catalog_code)
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.as_ref().map(entry_from_row))
    }

    async fn list_entries(
        &self,
        catalog_code: &str,
        include_inactive: bool,
    ) -> Result<Vec<MetadataCatalogEntry>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT catalog_code, code, name, rank, payload, is_active, source, created_at, updated_at
            FROM metadata_catalog_entries
            WHERE catalog_code = $1 AND ($2 OR is_active = TRUE)
            ORDER BY rank NULLS LAST, code ASC
            "#,
        )
        .bind(catalog_code)
        .bind(include_inactive)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(rows.iter().map(entry_from_row).collect())
    }

    async fn set_entry_active(
        &self,
        catalog_code: &str,
        code: &str,
        is_active: bool,
    ) -> Result<Option<MetadataCatalogEntry>, DomainError> {
        sqlx::query(
            r#"
            UPDATE metadata_catalog_entries
            SET is_active = $3, updated_at = NOW()
            WHERE catalog_code = $1 AND code = $2
            "#,
        )
        .bind(catalog_code)
        .bind(code)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        self.find_entry(catalog_code, code).await
    }

    async fn upsert_ingest_entry(
        &self,
        catalog_code: &str,
        code: &str,
        name: &str,
    ) -> Result<MetadataCatalogEntry, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO metadata_catalog_entries (
                catalog_code, code, name, rank, payload, is_active, source
            ) VALUES ($1, $2, $3, NULL, '{}'::jsonb, TRUE, 'ingest')
            ON CONFLICT (catalog_code, code) DO UPDATE SET
                updated_at = NOW()
            "#,
        )
        .bind(catalog_code)
        .bind(code)
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_entry(catalog_code, code)
            .await?
            .ok_or_else(|| DomainError::Internal("ingest upsert returned no row".into()))
    }
}

fn field_overlay_from_row(row: &sqlx::postgres::PgRow) -> FieldOverlay {
    FieldOverlay {
        object_name: row.get("object_name"),
        field_name: row.get("field_name"),
        field_type: row.get("field_type"),
        catalog_code: row.get("catalog_code"),
        object_name_target: row.get("object_name_target"),
        required: row.get("required"),
        list_visible: row.get("list_visible"),
        filterable: row.get("filterable"),
        widget: row.get("widget"),
        description: row.get("description"),
        visible_when: row.get("visible_when"),
        max_length: row.get("max_length"),
        min: row.get("min_value"),
        max: row.get("max_value"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[async_trait]
impl FieldOverlayRepository for PgMetadataCatalogRepository {
    async fn list(&self, object_name: Option<&str>, include_inactive: bool) -> Result<Vec<FieldOverlay>, DomainError> {
        let rows = sqlx::query("SELECT object_name, field_name, field_type, catalog_code, object_name_target, required, list_visible, filterable, widget, description, visible_when, max_length, min_value, max_value, is_active, created_at, updated_at FROM ontology_field_overlays WHERE ($1::text IS NULL OR object_name = $1) AND ($2 OR is_active) ORDER BY object_name, field_name")
            .bind(object_name).bind(include_inactive).fetch_all(&self.pool).await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(field_overlay_from_row).collect())
    }
    async fn find(&self, object_name: &str, field_name: &str) -> Result<Option<FieldOverlay>, DomainError> {
        let row = sqlx::query("SELECT object_name, field_name, field_type, catalog_code, object_name_target, required, list_visible, filterable, widget, description, visible_when, max_length, min_value, max_value, is_active, created_at, updated_at FROM ontology_field_overlays WHERE object_name=$1 AND field_name=$2")
            .bind(object_name).bind(field_name).fetch_optional(&self.pool).await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.as_ref().map(field_overlay_from_row))
    }
    async fn save(&self, overlay: &FieldOverlay) -> Result<FieldOverlay, DomainError> {
        sqlx::query("INSERT INTO ontology_field_overlays (object_name, field_name, field_type, catalog_code, object_name_target, required, list_visible, filterable, widget, description, visible_when, max_length, min_value, max_value, is_active) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) ON CONFLICT (object_name,field_name) DO UPDATE SET field_type=EXCLUDED.field_type,catalog_code=EXCLUDED.catalog_code,object_name_target=EXCLUDED.object_name_target,required=EXCLUDED.required,list_visible=EXCLUDED.list_visible,filterable=EXCLUDED.filterable,widget=EXCLUDED.widget,description=EXCLUDED.description,visible_when=EXCLUDED.visible_when,max_length=EXCLUDED.max_length,min_value=EXCLUDED.min_value,max_value=EXCLUDED.max_value,is_active=EXCLUDED.is_active,updated_at=NOW()")
            .bind(&overlay.object_name).bind(&overlay.field_name).bind(&overlay.field_type).bind(&overlay.catalog_code).bind(&overlay.object_name_target)
            .bind(overlay.required).bind(overlay.list_visible).bind(overlay.filterable).bind(&overlay.widget).bind(&overlay.description).bind(&overlay.visible_when)
            .bind(overlay.max_length).bind(overlay.min).bind(overlay.max).bind(overlay.is_active).execute(&self.pool).await.map_err(|e| DomainError::Internal(e.to_string()))?;
        self.find(&overlay.object_name, &overlay.field_name)
            .await?
            .ok_or_else(|| DomainError::Internal("field overlay save returned no row".into()))
    }
    async fn set_active(
        &self,
        object_name: &str,
        field_name: &str,
        is_active: bool,
    ) -> Result<Option<FieldOverlay>, DomainError> {
        sqlx::query(
            "UPDATE ontology_field_overlays SET is_active=$3, updated_at=NOW() WHERE object_name=$1 AND field_name=$2",
        )
        .bind(object_name)
        .bind(field_name)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        self.find(object_name, field_name).await
    }

    async fn catalog_entry_is_active(&self, catalog_code: &str, entry_code: &str) -> Result<bool, DomainError> {
        let active = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM metadata_catalog_entries e JOIN metadata_catalogs c ON c.code = e.catalog_code WHERE e.catalog_code = $1 AND e.code = $2 AND e.is_active AND c.is_active)",
        )
        .bind(catalog_code)
        .bind(entry_code)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(active)
    }
}
