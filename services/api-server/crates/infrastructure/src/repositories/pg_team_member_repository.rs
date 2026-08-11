//! PostgreSQL 班组成员仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{MemberRole, TeamMember};
use fms_domain::ports::dispatch_repository::TeamMemberRepository;

pub struct PgTeamMemberRepository {
    pool: PgPool,
}

impl PgTeamMemberRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TeamMemberRepository for PgTeamMemberRepository {
    async fn save(&self, member: &TeamMember) -> Result<TeamMember, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO team_members (
                id, team_id, user_id, role, can_drive, is_active, left_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (team_id, user_id) DO UPDATE SET
                role = EXCLUDED.role,
                can_drive = EXCLUDED.can_drive,
                is_active = EXCLUDED.is_active,
                left_at = EXCLUDED.left_at
            "#,
        )
        .bind(&member.id)
        .bind(&member.team_id)
        .bind(&member.user_id)
        .bind(member_role_value(member.role))
        .bind(member.can_drive)
        .bind(member.is_active)
        .bind(member.left_at)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        let mut items = self.find_by_team(&member.team_id, true).await?;
        items.retain(|item| item.user_id == member.user_id);
        items
            .into_iter()
            .next()
            .ok_or_else(|| DomainError::Internal("team member save returned no row".into()))
    }

    async fn find_by_team(&self, team_id: &str, include_inactive: bool) -> Result<Vec<TeamMember>, DomainError> {
        let sql = if include_inactive {
            r#"
            SELECT tm.id, tm.team_id, tm.user_id, tm.role, tm.can_drive, tm.joined_at, tm.left_at, tm.is_active,
                   u.username, COALESCE(u.display_name, u.username) AS user_display_name
            FROM team_members tm
            LEFT JOIN users u ON u.id = tm.user_id
            WHERE tm.team_id = $1
            ORDER BY tm.role, u.username
            "#
        } else {
            r#"
            SELECT tm.id, tm.team_id, tm.user_id, tm.role, tm.can_drive, tm.joined_at, tm.left_at, tm.is_active,
                   u.username, COALESCE(u.display_name, u.username) AS user_display_name
            FROM team_members tm
            LEFT JOIN users u ON u.id = tm.user_id
            WHERE tm.team_id = $1 AND tm.is_active = TRUE
            ORDER BY tm.role, u.username
            "#
        };

        let rows = sqlx::query(sql)
            .bind(team_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_team_member).collect())
    }

    async fn find_by_user(&self, user_id: &str) -> Result<Vec<TeamMember>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT tm.id, tm.team_id, tm.user_id, tm.role, tm.can_drive, tm.joined_at, tm.left_at, tm.is_active,
                   u.username, COALESCE(u.display_name, u.username) AS user_display_name
            FROM team_members tm
            LEFT JOIN users u ON u.id = tm.user_id
            WHERE tm.user_id = $1 AND tm.is_active = TRUE
            ORDER BY tm.joined_at DESC NULLS LAST, tm.id DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_team_member).collect())
    }

    async fn list_active_users(&self) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT tm.user_id, u.username
            FROM team_members tm
            LEFT JOIN users u ON u.id = tm.user_id
            WHERE tm.is_active = TRUE
            ORDER BY u.username NULLS LAST, tm.user_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows
            .iter()
            .filter_map(|row| row.get::<Option<String>, _>("user_id"))
            .map(|user_id| user_id.trim().to_string())
            .filter(|user_id| !user_id.is_empty())
            .collect())
    }

    async fn remove_from_team(&self, team_id: &str, user_id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE team_members
            SET is_active = FALSE, left_at = CURRENT_TIMESTAMP
            WHERE team_id = $1 AND user_id = $2 AND is_active = TRUE
            "#,
        )
        .bind(team_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(result.rows_affected() > 0)
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

fn parse_member_role(value: Option<&str>) -> MemberRole {
    match value.unwrap_or("member") {
        "leader" => MemberRole::Leader,
        "driver" => MemberRole::Driver,
        _ => MemberRole::Member,
    }
}

fn member_role_value(role: MemberRole) -> &'static str {
    match role {
        MemberRole::Leader => "leader",
        MemberRole::Member => "member",
        MemberRole::Driver => "driver",
    }
}
