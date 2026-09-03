//! PostgreSQL 派工单成员仓储实现

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::ports::dispatch_repository::{
    DispatchOrderMemberRepository, DispatchOrderMemberTransactionalRepository,
};

pub struct PgDispatchOrderMemberRepository {
    pool: PgPool,
}

impl PgDispatchOrderMemberRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DispatchOrderMemberRepository for PgDispatchOrderMemberRepository {
    async fn save(&self, member: &DispatchOrderMember) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO dispatch_order_members (
                id, dispatch_order_id, user_id, role, source_type,
                source_team_id, slot_code, qualification_code, qualification_level_code,
                assigned_at, check_in_time, check_out_time, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (dispatch_order_id, user_id) DO UPDATE SET
                role = EXCLUDED.role,
                source_type = EXCLUDED.source_type,
                source_team_id = EXCLUDED.source_team_id,
                slot_code = EXCLUDED.slot_code,
                qualification_code = EXCLUDED.qualification_code,
                qualification_level_code = EXCLUDED.qualification_level_code,
                is_active = EXCLUDED.is_active,
                check_in_time = COALESCE(EXCLUDED.check_in_time, dispatch_order_members.check_in_time),
                check_out_time = COALESCE(EXCLUDED.check_out_time, dispatch_order_members.check_out_time)
            "#,
        )
        .bind(&member.id)
        .bind(&member.dispatch_order_id)
        .bind(&member.user_id)
        .bind(member.role.as_ref())
        .bind(member.source_type.as_ref())
        .bind(&member.source_team_id)
        .bind(&member.slot_code)
        .bind(&member.qualification_code)
        .bind(&member.qualification_level_code)
        .bind(member.assigned_at.unwrap_or_else(Utc::now))
        .bind(member.check_in_time)
        .bind(member.check_out_time)
        .bind(member.is_active)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn find_by_order(&self, order_id: &str) -> Result<Vec<DispatchOrderMember>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT dom.*, u.username
            FROM dispatch_order_members dom
            LEFT JOIN users u ON u.id = dom.user_id
            WHERE dom.dispatch_order_id = $1
              AND dom.is_active = TRUE
            ORDER BY dom.assigned_at ASC NULLS LAST
            "#,
        )
        .bind(order_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.iter().map(row_to_member).collect())
    }

    async fn find_by_order_and_user(
        &self,
        order_id: &str,
        user_id: &str,
    ) -> Result<Option<DispatchOrderMember>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT dom.*, u.username
            FROM dispatch_order_members dom
            LEFT JOIN users u ON u.id = dom.user_id
            WHERE dom.dispatch_order_id = $1
              AND dom.user_id = $2
              AND dom.is_active = TRUE
            LIMIT 1
            "#,
        )
        .bind(order_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.as_ref().map(row_to_member))
    }

    async fn find_latest_checkout_for_user(
        &self,
        user_id: &str,
        before: DateTime<Utc>,
    ) -> Result<Option<serde_json::Value>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT dom.dispatch_order_id,
                   dom.check_out_time,
                   do2.stand_id,
                   s.code AS stand_code
            FROM dispatch_order_members dom
            JOIN dispatch_orders do2 ON do2.id = dom.dispatch_order_id
            LEFT JOIN stands s ON s.id = do2.stand_id
            WHERE dom.user_id = $1
              AND dom.check_out_time IS NOT NULL
              AND dom.check_out_time < $2
              AND dom.is_active = TRUE
            ORDER BY dom.check_out_time DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(before)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        match row {
            Some(r) => {
                let dispatch_order_id: String = r
                    .try_get("dispatch_order_id")
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                let check_out_time: DateTime<Utc> = r
                    .try_get("check_out_time")
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                let stand_id: Option<String> = r
                    .try_get("stand_id")
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                let stand_code: Option<String> = r
                    .try_get("stand_code")
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                Ok(Some(serde_json::json!({
                    "dispatch_order_id": dispatch_order_id,
                    "check_out_time": check_out_time,
                    "stand_id": stand_id,
                    "stand_code": stand_code,
                })))
            }
            None => Ok(None),
        }
    }

    async fn find_active_slots_for_users(&self, user_ids: &[String]) -> Result<Vec<serde_json::Value>, DomainError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"
            SELECT dom.user_id,
                   dom.dispatch_order_id AS order_id,
                   do2.flight_id,
                   f.flight_number AS flight_no,
                   do2.task_type,
                   do2.task_type_name,
                   dom.slot_code,
                   do2.crew_requirement_snapshot,
                   do2.planned_start_time,
                   do2.status
            FROM dispatch_order_members dom
            JOIN dispatch_orders do2 ON do2.id = dom.dispatch_order_id
            LEFT JOIN flights f ON f.id = do2.flight_id
            WHERE dom.user_id = ANY($1)
              AND dom.is_active = TRUE
              AND dom.slot_code IS NOT NULL
              AND do2.publication_state = 'published'
              AND do2.status IN ('assigned', 'in_progress')
            ORDER BY CASE WHEN do2.status = 'in_progress' THEN 0 ELSE 1 END,
                     do2.planned_start_time ASC NULLS LAST
            "#,
        )
        .bind(user_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let slot_code: Option<String> = row.try_get("slot_code").ok();
            let slot_name = Self::slot_name_from_snapshot(
                row.try_get::<Option<serde_json::Value>, _>("crew_requirement_snapshot")
                    .ok()
                    .flatten(),
                slot_code.as_deref(),
            );
            result.push(serde_json::json!({
                "user_id": row.try_get::<String, _>("user_id").unwrap_or_default(),
                "order_id": row.try_get::<String, _>("order_id").unwrap_or_default(),
                "flight_id": row.try_get::<String, _>("flight_id").unwrap_or_default(),
                "flight_no": row.try_get::<Option<String>, _>("flight_no").ok().flatten(),
                "task_type": row.try_get::<String, _>("task_type").unwrap_or_default(),
                "task_type_name": row.try_get::<Option<String>, _>("task_type_name").ok().flatten(),
                "slot_code": slot_code,
                "slot_name": slot_name,
                "status": row.try_get::<String, _>("status").unwrap_or_default(),
                "planned_start_time": row.try_get::<Option<DateTime<Utc>>, _>("planned_start_time")
                    .ok()
                    .flatten()
                    .map(|value| value.to_rfc3339()),
            }));
        }
        Ok(result)
    }
}

impl PgDispatchOrderMemberRepository {
    fn slot_name_from_snapshot(snapshot: Option<serde_json::Value>, slot_code: Option<&str>) -> Option<String> {
        let slot_code = slot_code?;
        let snapshot = snapshot?;
        let arr = snapshot.as_array()?;
        for item in arr {
            if item.get("slot_code").and_then(|value| value.as_str()) == Some(slot_code) {
                if let Some(name) = item
                    .get("slot_name")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    return Some(name.to_string());
                }
            }
        }
        None
    }
}

#[async_trait]
impl<'tx> DispatchOrderMemberTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>>
    for PgDispatchOrderMemberRepository
{
    async fn save_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        member: &DispatchOrderMember,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO dispatch_order_members (
                id, dispatch_order_id, user_id, role, source_type,
                source_team_id, slot_code, qualification_code, qualification_level_code,
                assigned_at, check_in_time, check_out_time, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (dispatch_order_id, user_id) DO UPDATE SET
                role = EXCLUDED.role,
                source_type = EXCLUDED.source_type,
                source_team_id = EXCLUDED.source_team_id,
                slot_code = EXCLUDED.slot_code,
                qualification_code = EXCLUDED.qualification_code,
                qualification_level_code = EXCLUDED.qualification_level_code,
                is_active = EXCLUDED.is_active,
                check_in_time = COALESCE(EXCLUDED.check_in_time, dispatch_order_members.check_in_time),
                check_out_time = COALESCE(EXCLUDED.check_out_time, dispatch_order_members.check_out_time)
            "#,
        )
        .bind(&member.id)
        .bind(&member.dispatch_order_id)
        .bind(&member.user_id)
        .bind(member.role.as_ref())
        .bind(member.source_type.as_ref())
        .bind(&member.source_team_id)
        .bind(&member.slot_code)
        .bind(&member.qualification_code)
        .bind(&member.qualification_level_code)
        .bind(member.assigned_at.unwrap_or_else(Utc::now))
        .bind(member.check_in_time)
        .bind(member.check_out_time)
        .bind(member.is_active)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }
}

fn row_to_member(row: &sqlx::postgres::PgRow) -> DispatchOrderMember {
    let role_str: String = row.try_get("role").unwrap_or_else(|_| "member".to_string());
    let role = match role_str.as_str() {
        "leader" => MemberRole::Leader,
        "driver" => MemberRole::Driver,
        _ => MemberRole::Member,
    };
    let source_str: String = row.try_get("source_type").unwrap_or_else(|_| "team".to_string());
    let source_type = match source_str.as_str() {
        "individual" => AssigneeType::Individual,
        _ => AssigneeType::Team,
    };

    DispatchOrderMember {
        id: row.try_get("id").unwrap_or_default(),
        dispatch_order_id: row.try_get("dispatch_order_id").unwrap_or_default(),
        user_id: row.try_get("user_id").unwrap_or_default(),
        role,
        source_type,
        source_team_id: row.try_get("source_team_id").ok(),
        slot_code: row.try_get("slot_code").ok(),
        qualification_code: row.try_get("qualification_code").ok(),
        qualification_level_code: row.try_get("qualification_level_code").ok(),
        assigned_at: row.try_get("assigned_at").ok(),
        check_in_time: row.try_get("check_in_time").ok(),
        check_out_time: row.try_get("check_out_time").ok(),
        is_active: row.try_get("is_active").unwrap_or(true),
        username: row.try_get("username").ok(),
    }
}
