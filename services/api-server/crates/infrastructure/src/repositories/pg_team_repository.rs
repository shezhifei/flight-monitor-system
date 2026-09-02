//! PostgreSQL 班组仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{MemberRole, Team, TeamMember, TeamStatus};
use fms_domain::ports::dispatch_repository::{TeamRepository, TeamTransactionalRepository};

pub struct PgTeamRepository {
    pool: PgPool,
}

impl PgTeamRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn load_members(&self, team_id: &str) -> Result<Vec<TeamMember>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT tm.id, tm.team_id, tm.user_id, tm.role, tm.can_drive, tm.joined_at, tm.left_at, tm.is_active,
                   u.username, COALESCE(u.display_name, u.username) AS user_display_name
            FROM team_members tm
            LEFT JOIN users u ON u.id = tm.user_id
            WHERE tm.team_id = $1 AND tm.is_active = TRUE
            ORDER BY tm.role, u.username
            "#,
        )
        .bind(team_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_team_member).collect())
    }
}

#[async_trait]
impl TeamRepository for PgTeamRepository {
    async fn save(&self, team: &Team) -> Result<Team, DomainError> {
        // PR2：不再写 team_type_id / terminal（保留列仅供历史读取，见迁移 141）。
        sqlx::query(
            r#"
            INSERT INTO teams (
                id, name, department_id, code, leader_id,
                current_status, current_position_lat, current_position_lng,
                current_stand_id, last_position_update, is_active, attributes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                department_id = EXCLUDED.department_id,
                code = EXCLUDED.code,
                leader_id = EXCLUDED.leader_id,
                current_status = EXCLUDED.current_status,
                current_position_lat = EXCLUDED.current_position_lat,
                current_position_lng = EXCLUDED.current_position_lng,
                current_stand_id = EXCLUDED.current_stand_id,
                last_position_update = EXCLUDED.last_position_update,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&team.id)
        .bind(&team.name)
        .bind(&team.department_id)
        .bind(&team.code)
        .bind(&team.leader_id)
        .bind(team_status_value(team.current_status))
        .bind(team.current_position_lat)
        .bind(team.current_position_lng)
        .bind(&team.current_stand_id)
        .bind(team.last_position_update)
        .bind(team.is_active)
        .bind(&team.attributes)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_by_id(&team.id, true)
            .await?
            .ok_or_else(|| DomainError::Internal("team save returned no row".into()))
    }

    async fn find_by_id(&self, id: &str, load_members: bool) -> Result<Option<Team>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, name, department_id, team_type_id, code, leader_id, current_status,
                   current_position_lat::double precision AS current_position_lat,
                   current_position_lng::double precision AS current_position_lng,
                   current_stand_id, last_position_update, created_at, updated_at, is_active, attributes
            FROM teams
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        match row {
            Some(item) => {
                let members = if load_members {
                    self.load_members(id).await?
                } else {
                    Vec::new()
                };
                Ok(Some(row_to_team(&item, members)))
            }
            None => Ok(None),
        }
    }

    async fn find_by_code(&self, code: &str) -> Result<Option<Team>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, name, department_id, team_type_id, code, leader_id, current_status,
                   current_position_lat::double precision AS current_position_lat,
                   current_position_lng::double precision AS current_position_lng,
                   current_stand_id, last_position_update, created_at, updated_at, is_active, attributes
            FROM teams
            WHERE code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(|item| row_to_team(&item, Vec::new())))
    }

    async fn find_available_for_dispatch(
        &self,
        team_type_id: Option<&str>,
        terminal: Option<&str>,
    ) -> Result<Vec<Team>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT id, name, department_id, team_type_id, code, leader_id, current_status,
                   current_position_lat::double precision AS current_position_lat,
                   current_position_lng::double precision AS current_position_lng,
                   current_stand_id, last_position_update, created_at, updated_at, is_active, attributes
            FROM teams
            WHERE is_active = TRUE AND current_status = 'on_duty'
            "#,
        );
        if let Some(value) = team_type_id {
            builder.push(" AND team_type_id = ").push_bind(value);
        }
        if let Some(value) = terminal {
            builder.push(" AND terminal = ").push_bind(value);
        }
        builder.push(" ORDER BY name");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(|row| row_to_team(row, Vec::new())).collect())
    }

    async fn find_all(
        &self,
        include_inactive: bool,
        team_type_id: Option<&str>,
        terminal: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Team>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT id, name, department_id, team_type_id, code, leader_id, current_status,
                   current_position_lat::double precision AS current_position_lat,
                   current_position_lng::double precision AS current_position_lng,
                   current_stand_id, last_position_update, created_at, updated_at, is_active, attributes
            FROM teams
            WHERE 1=1
            "#,
        );
        if !include_inactive {
            builder.push(" AND is_active = TRUE");
        }
        if let Some(value) = team_type_id {
            builder.push(" AND team_type_id = ").push_bind(value);
        }
        if let Some(value) = terminal {
            builder.push(" AND terminal = ").push_bind(value);
        }
        builder
            .push(" ORDER BY name LIMIT ")
            .push_bind(limit.max(1))
            .push(" OFFSET ")
            .push_bind(offset.max(0));

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(|row| row_to_team(row, Vec::new())).collect())
    }

    async fn update_position(&self, id: &str, lat: f64, lng: f64, stand_id: Option<&str>) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE teams
            SET current_position_lat = $1,
                current_position_lng = $2,
                current_stand_id = $3,
                last_position_update = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $4
            "#,
        )
        .bind(lat)
        .bind(lng)
        .bind(stand_id)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_status(&self, id: &str, status: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE teams
            SET current_status = $1,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $2
            "#,
        )
        .bind(status)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl<'tx> TeamTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>> for PgTeamRepository {
    async fn save_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        team: &Team,
    ) -> Result<Team, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO teams (
                id, name, department_id, code, leader_id,
                current_status, current_position_lat, current_position_lng,
                current_stand_id, last_position_update, is_active, attributes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                department_id = EXCLUDED.department_id,
                code = EXCLUDED.code,
                leader_id = EXCLUDED.leader_id,
                current_status = EXCLUDED.current_status,
                current_position_lat = EXCLUDED.current_position_lat,
                current_position_lng = EXCLUDED.current_position_lng,
                current_stand_id = EXCLUDED.current_stand_id,
                last_position_update = EXCLUDED.last_position_update,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&team.id)
        .bind(&team.name)
        .bind(&team.department_id)
        .bind(&team.code)
        .bind(&team.leader_id)
        .bind(team_status_value(team.current_status))
        .bind(team.current_position_lat)
        .bind(team.current_position_lng)
        .bind(&team.current_stand_id)
        .bind(team.last_position_update)
        .bind(team.is_active)
        .bind(&team.attributes)
        .execute(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        let row = sqlx::query(
            r#"
            SELECT id, name, department_id, team_type_id, code, leader_id, current_status,
                   current_position_lat::double precision AS current_position_lat,
                   current_position_lng::double precision AS current_position_lng,
                   current_stand_id, last_position_update, created_at, updated_at, is_active, attributes
            FROM teams WHERE id = $1
            "#,
        )
        .bind(&team.id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?
        .ok_or_else(|| DomainError::Internal("team transactional save returned no row".into()))?;
        Ok(row_to_team(&row, Vec::new()))
    }
}

fn row_to_team(row: &sqlx::postgres::PgRow, members: Vec<TeamMember>) -> Team {
    Team {
        id: row.get("id"),
        name: row.get("name"),
        department_id: row.get("department_id"),
        team_type_id: row.get("team_type_id"),
        code: row.get("code"),
        leader_id: row.get("leader_id"),
        current_status: parse_team_status(row.get::<Option<String>, _>("current_status").as_deref()),
        current_position_lat: row.get("current_position_lat"),
        current_position_lng: row.get("current_position_lng"),
        current_stand_id: row.get("current_stand_id"),
        last_position_update: row.get("last_position_update"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        team_type: None,
        members,
        attributes: row.try_get("attributes").unwrap_or_else(|_| serde_json::json!({})),
    }
}

fn row_to_team_member(row: &sqlx::postgres::PgRow) -> TeamMember {
    TeamMember {
        id: row.get("id"),
        team_id: row.get("team_id"),
        user_id: row.get("user_id"),
        role: parse_member_role(row.get::<Option<String>, _>("role").as_deref()),
        can_drive: row.get::<Option<bool>, _>("can_drive").unwrap_or(false),
        joined_at: row.get("joined_at"),
        left_at: row.get("left_at"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        username: row.get("username"),
        user_display_name: row.get("user_display_name"),
    }
}

fn parse_team_status(value: Option<&str>) -> TeamStatus {
    match value.unwrap_or("off_duty") {
        "on_duty" => TeamStatus::OnDuty,
        "break" => TeamStatus::Break,
        _ => TeamStatus::OffDuty,
    }
}

fn team_status_value(status: TeamStatus) -> &'static str {
    match status {
        TeamStatus::OnDuty => "on_duty",
        TeamStatus::OffDuty => "off_duty",
        TeamStatus::Break => "break",
    }
}

fn parse_member_role(value: Option<&str>) -> MemberRole {
    match value.unwrap_or("member") {
        "leader" => MemberRole::Leader,
        "driver" => MemberRole::Driver,
        _ => MemberRole::Member,
    }
}
