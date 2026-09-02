//! PostgreSQL 航站楼目录仓储实现。
//!
//! 维护 `terminals` / `gates` / `baggage_carousels` 目录行与楼成员表
//! `terminal_stands` / `terminal_gates` / `terminal_carousels`。目录与成员
//! 关系在应用层一起维护（无数据库外键，见 `docs/plans/2026-08-12-remove-foreign-keys-spec.md`）。

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    BaggageCarousel, Gate, Stand, Terminal, TerminalDirectory,
};
use fms_domain::ports::dispatch_repository::{
    FacilityLocale, TerminalRepository, TerminalResourceTransactionalRepository,
};

pub struct PgTerminalRepository {
    pool: PgPool,
}

impl PgTerminalRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn opaque_id_or_ulid(value: String) -> String {
    if value.is_empty() {
        ulid::Ulid::new().to_string()
    } else {
        value
    }
}

#[async_trait]
impl TerminalRepository for PgTerminalRepository {
    // ------------------------------------------------------------ Terminal --
    async fn save_terminal(&self, terminal: &Terminal) -> Result<Terminal, DomainError> {
        let id = opaque_id_or_ulid(terminal.terminal_id.clone());
        sqlx::query(
            r#"
            INSERT INTO terminals (terminal_id, code, name, is_active, attributes)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (terminal_id) DO UPDATE SET
                code = EXCLUDED.code,
                name = EXCLUDED.name,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&id)
        .bind(&terminal.code)
        .bind(&terminal.name)
        .bind(terminal.is_active)
        .bind(&terminal.attributes)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_terminal_by_id(&id)
            .await?
            .ok_or_else(|| DomainError::Internal("terminal save returned no row".into()))
    }

    async fn find_terminal_by_id(&self, terminal_id: &str) -> Result<Option<Terminal>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT terminal_id, code, name, is_active, attributes, created_at, updated_at
            FROM terminals
            WHERE terminal_id = $1
            "#,
        )
        .bind(terminal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(row_to_terminal))
    }

    async fn find_terminal_by_code(&self, code: &str) -> Result<Option<Terminal>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT terminal_id, code, name, is_active, attributes, created_at, updated_at
            FROM terminals
            WHERE code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(row_to_terminal))
    }

    async fn find_terminals(&self, include_inactive: bool) -> Result<Vec<Terminal>, DomainError> {
        let sql = if include_inactive {
            "SELECT terminal_id, code, name, is_active, attributes, created_at, updated_at FROM terminals ORDER BY code"
        } else {
            "SELECT terminal_id, code, name, is_active, attributes, created_at, updated_at FROM terminals \
             WHERE is_active = TRUE ORDER BY code"
        };
        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(rows.into_iter().map(row_to_terminal).collect())
    }

    async fn set_terminal_active(&self, terminal_id: &str, is_active: bool) -> Result<Option<Terminal>, DomainError> {
        let result = sqlx::query(
            "UPDATE terminals SET is_active = $1, updated_at = CURRENT_TIMESTAMP WHERE terminal_id = $2",
        )
        .bind(is_active)
        .bind(terminal_id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.find_terminal_by_id(terminal_id).await
    }

    // ---------------------------------------------------------------- Gate --
    async fn save_gate(&self, gate: &Gate) -> Result<Gate, DomainError> {
        let id = opaque_id_or_ulid(gate.gate_id.clone());
        sqlx::query(
            r#"
            INSERT INTO gates (gate_id, code, name, is_active, attributes)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (gate_id) DO UPDATE SET
                code = EXCLUDED.code,
                name = EXCLUDED.name,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&id)
        .bind(&gate.code)
        .bind(&gate.name)
        .bind(gate.is_active)
        .bind(&gate.attributes)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_gate_by_id(&id)
            .await?
            .ok_or_else(|| DomainError::Internal("gate save returned no row".into()))
    }

    async fn find_gate_by_id(&self, gate_id: &str) -> Result<Option<Gate>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT gate_id, code, name, is_active, attributes, created_at, updated_at
            FROM gates
            WHERE gate_id = $1
            "#,
        )
        .bind(gate_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(row_to_gate))
    }

    async fn find_gate_by_code(&self, code: &str) -> Result<Option<Gate>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT gate_id, code, name, is_active, attributes, created_at, updated_at
            FROM gates
            WHERE code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(row_to_gate))
    }

    async fn set_gate_active(&self, gate_id: &str, is_active: bool) -> Result<Option<Gate>, DomainError> {
        let result = sqlx::query("UPDATE gates SET is_active = $1, updated_at = CURRENT_TIMESTAMP WHERE gate_id = $2")
            .bind(is_active)
            .bind(gate_id)
            .execute(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.find_gate_by_id(gate_id).await
    }

    // ------------------------------------------------------------ Carousel --
    async fn save_carousel(&self, carousel: &BaggageCarousel) -> Result<BaggageCarousel, DomainError> {
        let id = opaque_id_or_ulid(carousel.carousel_id.clone());
        sqlx::query(
            r#"
            INSERT INTO baggage_carousels (carousel_id, code, name, is_active, attributes)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (carousel_id) DO UPDATE SET
                code = EXCLUDED.code,
                name = EXCLUDED.name,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&id)
        .bind(&carousel.code)
        .bind(&carousel.name)
        .bind(carousel.is_active)
        .bind(&carousel.attributes)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_carousel_by_id(&id)
            .await?
            .ok_or_else(|| DomainError::Internal("carousel save returned no row".into()))
    }

    async fn find_carousel_by_id(&self, carousel_id: &str) -> Result<Option<BaggageCarousel>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT carousel_id, code, name, is_active, attributes, created_at, updated_at
            FROM baggage_carousels
            WHERE carousel_id = $1
            "#,
        )
        .bind(carousel_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(row_to_carousel))
    }

    async fn find_carousel_by_code(&self, code: &str) -> Result<Option<BaggageCarousel>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT carousel_id, code, name, is_active, attributes, created_at, updated_at
            FROM baggage_carousels
            WHERE code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(row_to_carousel))
    }

    async fn set_carousel_active(
        &self,
        carousel_id: &str,
        is_active: bool,
    ) -> Result<Option<BaggageCarousel>, DomainError> {
        let result = sqlx::query(
            "UPDATE baggage_carousels SET is_active = $1, updated_at = CURRENT_TIMESTAMP WHERE carousel_id = $2",
        )
        .bind(is_active)
        .bind(carousel_id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.find_carousel_by_id(carousel_id).await
    }

    // ------------------------------------------------------------ members --
    async fn find_stand_by_id(&self, stand_id: &str) -> Result<Option<Stand>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, code, name, terminal, area,
                   position_lat::double precision AS position_lat,
                   position_lng::double precision AS position_lng,
                   stand_type, size_category, is_active, attributes, created_at
            FROM stands
            WHERE id = $1
            "#,
        )
        .bind(stand_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(row_to_stand))
    }

    async fn find_stand_by_code(&self, code: &str) -> Result<Option<Stand>, DomainError> {
        let row = sqlx::query(
            "SELECT id, code, name, terminal, area, position_lat, position_lng, stand_type, size_category, is_active, attributes, created_at FROM stands WHERE code = $1 LIMIT 1",
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row.map(row_to_stand))
    }

    async fn save_stand(&self, stand: &Stand) -> Result<Stand, DomainError> {
        let id = opaque_id_or_ulid(stand.id.clone());
        sqlx::query(
            r#"
            INSERT INTO stands (
                id, code, name, terminal, area, position_lat, position_lng,
                stand_type, size_category, is_active, attributes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (id) DO UPDATE SET
                code = EXCLUDED.code,
                name = EXCLUDED.name,
                terminal = EXCLUDED.terminal,
                area = EXCLUDED.area,
                position_lat = EXCLUDED.position_lat,
                position_lng = EXCLUDED.position_lng,
                stand_type = EXCLUDED.stand_type,
                size_category = EXCLUDED.size_category,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes
            "#,
        )
        .bind(&id)
        .bind(&stand.code)
        .bind(&stand.name)
        .bind(&stand.terminal)
        .bind(&stand.area)
        .bind(stand.position_lat)
        .bind(stand.position_lng)
        .bind(&stand.stand_type)
        .bind(&stand.size_category)
        .bind(stand.is_active)
        .bind(&stand.attributes)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_stand_by_id(&id)
            .await?
            .ok_or_else(|| DomainError::Internal("stand save returned no row".into()))
    }

    async fn set_stand_active(&self, stand_id: &str, is_active: bool) -> Result<Option<Stand>, DomainError> {
        let result = sqlx::query("UPDATE stands SET is_active = $1 WHERE id = $2")
            .bind(is_active)
            .bind(stand_id)
            .execute(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.find_stand_by_id(stand_id).await
    }

    async fn add_stand(&self, terminal_id: &str, stand_id: &str) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO terminal_stands (terminal_id, stand_id)
            VALUES ($1, $2)
            ON CONFLICT (stand_id) DO NOTHING
            "#,
        )
        .bind(terminal_id)
        .bind(stand_id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(())
    }

    async fn remove_stand(&self, stand_id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM terminal_stands WHERE stand_id = $1")
            .bind(stand_id)
            .execute(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(())
    }

    async fn add_gate(&self, terminal_id: &str, gate_id: &str) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO terminal_gates (terminal_id, gate_id)
            VALUES ($1, $2)
            ON CONFLICT (gate_id) DO NOTHING
            "#,
        )
        .bind(terminal_id)
        .bind(gate_id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(())
    }

    async fn remove_gate(&self, gate_id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM terminal_gates WHERE gate_id = $1")
            .bind(gate_id)
            .execute(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(())
    }

    async fn add_carousel(&self, terminal_id: &str, carousel_id: &str) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO terminal_carousels (terminal_id, carousel_id)
            VALUES ($1, $2)
            ON CONFLICT (carousel_id) DO NOTHING
            "#,
        )
        .bind(terminal_id)
        .bind(carousel_id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(())
    }

    async fn remove_carousel(&self, carousel_id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM terminal_carousels WHERE carousel_id = $1")
            .bind(carousel_id)
            .execute(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(())
    }

    // ------------------------------------------------------ occupation guard --
    async fn active_stand_occupations(&self, stand_code: &str) -> Result<Vec<serde_json::Value>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT id, registration, stand_code, flight_id, starts_at, ends_at, status
            FROM stand_occupations
            WHERE stand_code = $1 AND status = 'active' AND ends_at > NOW()
            "#,
        )
        .bind(stand_code)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "facility": "stand",
                    "code": row.get::<String, _>("stand_code"),
                    "aircraft": row.get::<Option<String>, _>("registration"),
                    "flight_id": row.get::<Option<String>, _>("flight_id"),
                    "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at"),
                    "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at"),
                })
            })
            .collect())
    }

    async fn active_gate_assignments(&self, gate_code: &str) -> Result<Vec<serde_json::Value>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT id, registration, gate_code, flight_id, starts_at, ends_at, status
            FROM gate_assignments
            WHERE gate_code = $1 AND status = 'active' AND ends_at > NOW()
            "#,
        )
        .bind(gate_code)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "facility": "gate",
                    "code": row.get::<String, _>("gate_code"),
                    "aircraft": row.get::<Option<String>, _>("registration"),
                    "flight_id": row.get::<Option<String>, _>("flight_id"),
                    "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at"),
                    "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at"),
                })
            })
            .collect())
    }

    async fn active_carousel_assignments(&self, carousel_code: &str) -> Result<Vec<serde_json::Value>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT id, carousel_code, flight_id, starts_at, ends_at, status
            FROM carousel_assignments
            WHERE carousel_code = $1 AND status = 'active' AND ends_at > NOW()
            "#,
        )
        .bind(carousel_code)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "facility": "carousel",
                    "code": row.get::<String, _>("carousel_code"),
                    "flight_id": row.get::<Option<String>, _>("flight_id"),
                    "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at"),
                    "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at"),
                })
            })
            .collect())
    }

    // ------------------------------------------------------ context --
    async fn terminal_directory(&self, terminal_id: &str) -> Result<Option<TerminalDirectory>, DomainError> {
        let Some(terminal) = self.find_terminal_by_id(terminal_id).await? else {
            return Ok(None);
        };

        let stand_rows = sqlx::query(
            r#"
            SELECT s.id, s.code, s.name, s.terminal, s.area,
                   s.position_lat::double precision AS position_lat,
                   s.position_lng::double precision AS position_lng,
                   s.stand_type, s.size_category, s.is_active, s.attributes, s.created_at
            FROM terminal_stands ts
            JOIN stands s ON s.id = ts.stand_id
            WHERE ts.terminal_id = $1
            ORDER BY s.code
            "#,
        )
        .bind(terminal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        let stands: Vec<Stand> = stand_rows.into_iter().map(row_to_stand).collect();

        let gate_rows = sqlx::query(
            r#"
            SELECT g.gate_id, g.code, g.name, g.is_active, g.attributes, g.created_at, g.updated_at
            FROM terminal_gates tg
            JOIN gates g ON g.gate_id = tg.gate_id
            WHERE tg.terminal_id = $1
            ORDER BY g.code
            "#,
        )
        .bind(terminal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        let gates: Vec<Gate> = gate_rows.into_iter().map(row_to_gate).collect();

        let carousel_rows = sqlx::query(
            r#"
            SELECT c.carousel_id, c.code, c.name, c.is_active, c.attributes, c.created_at, c.updated_at
            FROM terminal_carousels tc
            JOIN baggage_carousels c ON c.carousel_id = tc.carousel_id
            WHERE tc.terminal_id = $1
            ORDER BY c.code
            "#,
        )
        .bind(terminal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        let carousels: Vec<BaggageCarousel> = carousel_rows.into_iter().map(row_to_carousel).collect();

        Ok(Some(TerminalDirectory {
            terminal,
            stands,
            gates,
            carousels,
        }))
    }

    // ----------------------------------------- allocate 前校验落点（PR3）--

    async fn stand_locale_by_code(&self, code: &str) -> Result<FacilityLocale, DomainError> {
        self.locale_by_code(
            "stands",
            "id",
            "is_active",
            "terminal_stands",
            "stand_id",
            code,
        )
        .await
    }

    async fn gate_locale_by_code(&self, code: &str) -> Result<FacilityLocale, DomainError> {
        self.locale_by_code(
            "gates",
            "gate_id",
            "is_active",
            "terminal_gates",
            "gate_id",
            code,
        )
        .await
    }

    async fn carousel_locale_by_code(&self, code: &str) -> Result<FacilityLocale, DomainError> {
        self.locale_by_code(
            "baggage_carousels",
            "carousel_id",
            "is_active",
            "terminal_carousels",
            "carousel_id",
            code,
        )
        .await
    }
}

impl PgTerminalRepository {
    /// 通用落点：`dir_table` 目录表，`dir_pk` 目录主键列，`dir_active` 目录启用列，
    /// `member_table` 成员表，`member_fk` 成员表引用目录主键的列。
    async fn locale_by_code(
        &self,
        dir_table: &str,
        dir_pk: &str,
        dir_active: &str,
        member_table: &str,
        member_fk: &str,
        code: &str,
    ) -> Result<FacilityLocale, DomainError> {
        let row = sqlx::query(&format!(
            "SELECT d.{active_col} AS active, t_active.code AS terminal_code, t_active.is_active AS terminal_active \
             FROM {dir_table} d \
             LEFT JOIN {member_table} m ON m.{member_fk} = d.{dir_pk} \
             LEFT JOIN terminals t_active ON t_active.terminal_id = m.terminal_id \
             WHERE d.code = $1",
            active_col = dir_active,
        ))
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        match row {
            None => Ok(FacilityLocale::Unknown),
            Some(r) => {
                let active: bool = r.get("active");
                if !active {
                    return Ok(FacilityLocale::Inactive);
                }
                match r.get::<Option<String>, _>("terminal_code") {
                    Some(code) => Ok(FacilityLocale::Terminal {
                        code,
                        active: r.get::<Option<bool>, _>("terminal_active").unwrap_or(true),
                    }),
                    None => Ok(FacilityLocale::NoTerminal),
                }
            }
        }
    }
}

#[async_trait]
impl<'tx> TerminalResourceTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>>
    for PgTerminalRepository
{
    async fn save_terminal_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        terminal: &Terminal,
    ) -> Result<Terminal, DomainError> {
        let id = opaque_id_or_ulid(terminal.terminal_id.clone());
        let row = sqlx::query(
            r#"
            INSERT INTO terminals (terminal_id, code, name, is_active, attributes)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (terminal_id) DO UPDATE SET
                code = EXCLUDED.code, name = EXCLUDED.name,
                is_active = EXCLUDED.is_active, attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            RETURNING terminal_id, code, name, is_active, attributes, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&terminal.code)
        .bind(&terminal.name)
        .bind(terminal.is_active)
        .bind(&terminal.attributes)
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row_to_terminal(row))
    }

    async fn save_gate_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        gate: &Gate,
    ) -> Result<Gate, DomainError> {
        let id = opaque_id_or_ulid(gate.gate_id.clone());
        let row = sqlx::query(
            r#"
            INSERT INTO gates (gate_id, code, name, is_active, attributes)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (gate_id) DO UPDATE SET
                code = EXCLUDED.code, name = EXCLUDED.name,
                is_active = EXCLUDED.is_active, attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            RETURNING gate_id, code, name, is_active, attributes, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&gate.code)
        .bind(&gate.name)
        .bind(gate.is_active)
        .bind(&gate.attributes)
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row_to_gate(row))
    }

    async fn save_carousel_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        carousel: &BaggageCarousel,
    ) -> Result<BaggageCarousel, DomainError> {
        let id = opaque_id_or_ulid(carousel.carousel_id.clone());
        let row = sqlx::query(
            r#"
            INSERT INTO baggage_carousels (carousel_id, code, name, is_active, attributes)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (carousel_id) DO UPDATE SET
                code = EXCLUDED.code, name = EXCLUDED.name,
                is_active = EXCLUDED.is_active, attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            RETURNING carousel_id, code, name, is_active, attributes, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&carousel.code)
        .bind(&carousel.name)
        .bind(carousel.is_active)
        .bind(&carousel.attributes)
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row_to_carousel(row))
    }

    async fn save_stand_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        stand: &Stand,
    ) -> Result<Stand, DomainError> {
        let id = opaque_id_or_ulid(stand.id.clone());
        let row = sqlx::query(
            r#"
            INSERT INTO stands (
                id, code, name, terminal, area, position_lat, position_lng,
                stand_type, size_category, is_active, attributes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (id) DO UPDATE SET
                code = EXCLUDED.code, name = EXCLUDED.name, terminal = EXCLUDED.terminal,
                area = EXCLUDED.area, position_lat = EXCLUDED.position_lat,
                position_lng = EXCLUDED.position_lng, stand_type = EXCLUDED.stand_type,
                size_category = EXCLUDED.size_category, is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes
            RETURNING id, code, name, terminal, area,
                      position_lat::double precision AS position_lat,
                      position_lng::double precision AS position_lng,
                      stand_type, size_category, is_active, attributes, created_at
            "#,
        )
        .bind(&id)
        .bind(&stand.code)
        .bind(&stand.name)
        .bind(&stand.terminal)
        .bind(&stand.area)
        .bind(stand.position_lat)
        .bind(stand.position_lng)
        .bind(&stand.stand_type)
        .bind(&stand.size_category)
        .bind(stand.is_active)
        .bind(&stand.attributes)
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row_to_stand(row))
    }

    async fn save_gate_with_terminal_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        terminal_id: &str,
        gate: &Gate,
    ) -> Result<Gate, DomainError> {
        let saved = self.save_gate_in_tx(tx, gate).await?;
        sqlx::query(
            "INSERT INTO terminal_gates (terminal_id, gate_id) VALUES ($1, $2) ON CONFLICT (gate_id) DO NOTHING",
        )
        .bind(terminal_id)
        .bind(&saved.gate_id)
        .execute(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(saved)
    }

    async fn save_carousel_with_terminal_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        terminal_id: &str,
        carousel: &BaggageCarousel,
    ) -> Result<BaggageCarousel, DomainError> {
        let saved = self.save_carousel_in_tx(tx, carousel).await?;
        sqlx::query(
            "INSERT INTO terminal_carousels (terminal_id, carousel_id) VALUES ($1, $2) ON CONFLICT (carousel_id) DO NOTHING",
        )
        .bind(terminal_id)
        .bind(&saved.carousel_id)
        .execute(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(saved)
    }

    async fn save_stand_with_terminal_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        terminal_id: &str,
        stand: &Stand,
    ) -> Result<Stand, DomainError> {
        let saved = self.save_stand_in_tx(tx, stand).await?;
        sqlx::query(
            "INSERT INTO terminal_stands (terminal_id, stand_id) VALUES ($1, $2) ON CONFLICT (stand_id) DO NOTHING",
        )
        .bind(terminal_id)
        .bind(&saved.id)
        .execute(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(saved)
    }
}

fn row_to_terminal(row: sqlx::postgres::PgRow) -> Terminal {
    Terminal {
        terminal_id: row.get("terminal_id"),
        code: row.get("code"),
        name: row.get("name"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        attributes: row.try_get("attributes").unwrap_or_else(|_| serde_json::json!({})),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_gate(row: sqlx::postgres::PgRow) -> Gate {
    Gate {
        gate_id: row.get("gate_id"),
        code: row.get("code"),
        name: row.get("name"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        attributes: row.try_get("attributes").unwrap_or_else(|_| serde_json::json!({})),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_carousel(row: sqlx::postgres::PgRow) -> BaggageCarousel {
    BaggageCarousel {
        carousel_id: row.get("carousel_id"),
        code: row.get("code"),
        name: row.get("name"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        attributes: row.try_get("attributes").unwrap_or_else(|_| serde_json::json!({})),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
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
        attributes: row.try_get("attributes").unwrap_or_else(|_| serde_json::json!({})),
        created_at: row.get("created_at"),
    }
}
