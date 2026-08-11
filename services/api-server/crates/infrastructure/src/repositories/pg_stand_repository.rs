//! PostgreSQL 机位仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::Stand;
use fms_domain::ports::dispatch_repository::StandRepository;

pub struct PgStandRepository {
    pool: PgPool,
}

impl PgStandRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StandRepository for PgStandRepository {
    async fn save(&self, stand: &Stand) -> Result<Stand, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO stands (
                id, code, name, terminal, area, position_lat, position_lng,
                stand_type, size_category, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO UPDATE SET
                code = EXCLUDED.code,
                name = EXCLUDED.name,
                terminal = EXCLUDED.terminal,
                area = EXCLUDED.area,
                position_lat = EXCLUDED.position_lat,
                position_lng = EXCLUDED.position_lng,
                stand_type = EXCLUDED.stand_type,
                size_category = EXCLUDED.size_category,
                is_active = EXCLUDED.is_active
            "#,
        )
        .bind(&stand.id)
        .bind(&stand.code)
        .bind(&stand.name)
        .bind(&stand.terminal)
        .bind(&stand.area)
        .bind(stand.position_lat)
        .bind(stand.position_lng)
        .bind(&stand.stand_type)
        .bind(&stand.size_category)
        .bind(stand.is_active)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_by_id(&stand.id)
            .await?
            .ok_or_else(|| DomainError::Internal("stand save returned no row".into()))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Stand>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, code, name, terminal, area,
                   position_lat::double precision AS position_lat,
                   position_lng::double precision AS position_lng,
                   stand_type, size_category, is_active, created_at
            FROM stands
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(row_to_stand))
    }

    async fn find_by_code(&self, code: &str) -> Result<Option<Stand>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, code, name, terminal, area,
                   position_lat::double precision AS position_lat,
                   position_lng::double precision AS position_lng,
                   stand_type, size_category, is_active, created_at
            FROM stands
            WHERE code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(row_to_stand))
    }

    async fn find_all(
        &self,
        terminal: Option<&str>,
        include_inactive: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Stand>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT id, code, name, terminal, area,
                   position_lat::double precision AS position_lat,
                   position_lng::double precision AS position_lng,
                   stand_type, size_category, is_active, created_at
            FROM stands
            WHERE 1=1
            "#,
        );
        if !include_inactive {
            builder.push(" AND is_active = TRUE");
        }
        if let Some(value) = terminal {
            builder.push(" AND terminal = ").push_bind(value);
        }
        builder
            .push(" ORDER BY code LIMIT ")
            .push_bind(limit.max(1))
            .push(" OFFSET ")
            .push_bind(offset.max(0));

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.into_iter().map(row_to_stand).collect())
    }

    async fn is_active(&self, id_or_code: &str) -> Result<bool, DomainError> {
        let row: Option<(bool,)> = sqlx::query_as("SELECT is_active FROM stands WHERE id = $1 OR code = $1")
            .bind(id_or_code)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        match row {
            Some((is_active,)) => Ok(is_active),
            None => Err(DomainError::NotFound {
                entity_type: "stand",
                id: id_or_code.to_string(),
            }),
        }
    }
}

fn row_to_stand(row: sqlx::postgres::PgRow) -> Stand {
    Stand {
        id: row.get("id"),
        code: row.get("code"),
        name: row.get("name"),
        terminal: row.get("terminal"),
        area: row.get("area"),
        position_lat: row.get::<Option<f64>, _>("position_lat").unwrap_or(0.0),
        position_lng: row.get::<Option<f64>, _>("position_lng").unwrap_or(0.0),
        stand_type: row.get("stand_type"),
        size_category: row.get("size_category"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        created_at: row.get("created_at"),
    }
}
