//! PostgreSQL 派工单仓储实现

use async_trait::async_trait;
use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::ports::dispatch_repository::{
    CreateDispatchOrderCommand, DispatchOrderRepository, DispatchOrderTransactionalRepository,
};

pub struct PgDispatchOrderRepository {
    pool: PgPool,
}

impl PgDispatchOrderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn new_dispatch_record_id() -> String {
        ulid::Ulid::new().to_string()
    }

    async fn save_order_in_tx(tx: &mut Transaction<'_, Postgres>, order: &DispatchOrder) -> Result<(), DomainError> {
        // FK 移除后的应用层兜底校验（spec §3.2.6）：父航班必须存在且未软删
        let flight_id = order.flight_id.as_str();
        if flight_id.is_empty() {
            return Err(DomainError::ValidationError(
                "无法创建派工单：flight_id 不能为空".into(),
            ));
        }
        let flight_active: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM flights WHERE flight_id = $1 AND deleted_at IS NULL)")
                .bind(flight_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        if !flight_active {
            return Err(DomainError::ValidationError(format!(
                "无法创建派工单：父航班 {flight_id} 不存在或已删除"
            )));
        }
        sqlx::query(
            r#"
                INSERT INTO dispatch_orders (
                    id, flight_id, task_type, stand_id,
                    individual_user_id,
                    driver_type, driver_user_id,
                    planned_start_time, planned_end_time,
                    actual_start_time, actual_end_time,
                    estimated_completion_time,
                    estimated_completion_reported_by,
                    estimated_completion_reported_at,
                    estimated_completion_note,
                    status, dispatch_type, dispatched_at, dispatched_by,
                    snapshot_assignee_position, snapshot_equipment_positions,
                    estimated_arrival_minutes,
                    process_instance_id, process_task_id,
                    workflow_context, workflow_status, source,
                    schedule_source, lock_level,
                    publication_state, source_type, department_id, leg_scope,
                    generation_rule_id, generation_rule_version, generation_anchor_type, generation_anchor_time,
                    publish_trigger_mode, publish_at, turnaround_pair_key, turnaround_constraint_mode,
                    availability_reason, department_rule_version,
                    crew_requirement_snapshot, equipment_requirement_snapshot, task_crew, equipment_assignment,
                    qualification_gap, equipment_gap, score_breakdown, conflict_reason,
                    recommended_assignees, recommendation_score,
                    supervisor_notified, supervisor_notified_at,
                    assignment_deadline, completed_by, completion_notes,
                    created_at, updated_at,
                    completion_time_mode, completion_anchor_type, completion_anchor_time,
                    completion_offset_minutes, completion_warning_lead_minutes
                ) VALUES (
                    $1, $2, $3, $4,
                    $5,
                    $6, $7,
                    $8, $9,
                    $10, $11,
                    $12, $13, $14, $15,
                    $16, $17, $18, $19,
                    $20, $21,
                    $22,
                    $23, $24,
                    $25, $26, $27,
                    $28, $29,
                    $30, $31, $32, $33,
                    $34, $35, $36, $37,
                    $38, $39, $40, $41,
                    $42, $43,
                    $44, $45, $46, $47,
                    $48, $49, $50, $51,
                    $52, $53,
                    $54, $55,
                    $56, $57, $58,
                    $59, $60,
                    $61, $62, $63,
                    $64, $65
                )
                ON CONFLICT (id) DO UPDATE SET
                    stand_id = EXCLUDED.stand_id,
                    individual_user_id = EXCLUDED.individual_user_id,
                    driver_type = EXCLUDED.driver_type,
                    driver_user_id = EXCLUDED.driver_user_id,
                    planned_start_time = EXCLUDED.planned_start_time,
                    planned_end_time = EXCLUDED.planned_end_time,
                    actual_start_time = EXCLUDED.actual_start_time,
                    actual_end_time = EXCLUDED.actual_end_time,
                    estimated_completion_time = EXCLUDED.estimated_completion_time,
                    estimated_completion_reported_by = EXCLUDED.estimated_completion_reported_by,
                    estimated_completion_reported_at = EXCLUDED.estimated_completion_reported_at,
                    estimated_completion_note = EXCLUDED.estimated_completion_note,
                    status = EXCLUDED.status,
                    dispatch_type = EXCLUDED.dispatch_type,
                    dispatched_at = EXCLUDED.dispatched_at,
                    dispatched_by = EXCLUDED.dispatched_by,
                    snapshot_assignee_position = EXCLUDED.snapshot_assignee_position,
                    snapshot_equipment_positions = EXCLUDED.snapshot_equipment_positions,
                    estimated_arrival_minutes = EXCLUDED.estimated_arrival_minutes,
                    process_instance_id = EXCLUDED.process_instance_id,
                    process_task_id = EXCLUDED.process_task_id,
                    workflow_context = EXCLUDED.workflow_context,
                    workflow_status = EXCLUDED.workflow_status,
                    source = EXCLUDED.source,
                    schedule_source = EXCLUDED.schedule_source,
                    lock_level = EXCLUDED.lock_level,
                    publication_state = EXCLUDED.publication_state,
                    source_type = EXCLUDED.source_type,
                    department_id = EXCLUDED.department_id,
                    leg_scope = EXCLUDED.leg_scope,
                    generation_rule_id = EXCLUDED.generation_rule_id,
                    generation_rule_version = EXCLUDED.generation_rule_version,
                    generation_anchor_type = EXCLUDED.generation_anchor_type,
                    generation_anchor_time = EXCLUDED.generation_anchor_time,
                    publish_trigger_mode = EXCLUDED.publish_trigger_mode,
                    publish_at = EXCLUDED.publish_at,
                    turnaround_pair_key = EXCLUDED.turnaround_pair_key,
                    turnaround_constraint_mode = EXCLUDED.turnaround_constraint_mode,
                    availability_reason = EXCLUDED.availability_reason,
                    department_rule_version = EXCLUDED.department_rule_version,
                    crew_requirement_snapshot = EXCLUDED.crew_requirement_snapshot,
                    equipment_requirement_snapshot = EXCLUDED.equipment_requirement_snapshot,
                    task_crew = EXCLUDED.task_crew,
                    equipment_assignment = EXCLUDED.equipment_assignment,
                    qualification_gap = EXCLUDED.qualification_gap,
                    equipment_gap = EXCLUDED.equipment_gap,
                    score_breakdown = EXCLUDED.score_breakdown,
                    conflict_reason = EXCLUDED.conflict_reason,
                    recommended_assignees = EXCLUDED.recommended_assignees,
                    recommendation_score = EXCLUDED.recommendation_score,
                    supervisor_notified = EXCLUDED.supervisor_notified,
                    supervisor_notified_at = EXCLUDED.supervisor_notified_at,
                    assignment_deadline = EXCLUDED.assignment_deadline,
                    completed_by = EXCLUDED.completed_by,
                    completion_notes = EXCLUDED.completion_notes,
                    completion_time_mode = EXCLUDED.completion_time_mode,
                    completion_anchor_type = EXCLUDED.completion_anchor_type,
                    completion_anchor_time = EXCLUDED.completion_anchor_time,
                    completion_offset_minutes = EXCLUDED.completion_offset_minutes,
                    completion_warning_lead_minutes = EXCLUDED.completion_warning_lead_minutes,
                    created_at = COALESCE(dispatch_orders.created_at, EXCLUDED.created_at),
                    updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(&order.id)
        .bind(&order.flight_id)
        .bind(&order.task_type)
        .bind(&order.stand_id)
        .bind(&order.individual_user_id)
        .bind(order.driver_type.map(assignee_type_value))
        .bind(&order.driver_user_id)
        .bind(order.planned_start_time)
        .bind(order.planned_end_time)
        .bind(order.actual_start_time)
        .bind(order.actual_end_time)
        .bind(order.estimated_completion_time)
        .bind(&order.estimated_completion_reported_by)
        .bind(order.estimated_completion_reported_at)
        .bind(&order.estimated_completion_note)
        .bind(dispatch_order_status_value(order.status))
        .bind(dispatch_type_value(order.dispatch_type))
        .bind(order.dispatched_at)
        .bind(&order.dispatched_by)
        .bind(order.snapshot_assignee_position.as_ref())
        .bind(order.snapshot_equipment_positions.clone().map(serde_json::Value::Array))
        .bind(order.estimated_arrival_minutes)
        .bind(&order.process_instance_id)
        .bind(&order.process_task_id)
        .bind(&order.workflow_context)
        .bind(&order.workflow_status)
        .bind(&order.source)
        .bind(schedule_source_value(order.schedule_source))
        .bind(lock_level_value(order.lock_level))
        .bind(&order.publication_state)
        .bind(&order.source_type)
        .bind(&order.department_id)
        .bind(&order.leg_scope)
        .bind(&order.generation_rule_id)
        .bind(order.generation_rule_version)
        .bind(&order.generation_anchor_type)
        .bind(order.generation_anchor_time)
        .bind(&order.publish_trigger_mode)
        .bind(order.publish_at)
        .bind(&order.turnaround_pair_key)
        .bind(&order.turnaround_constraint_mode)
        .bind(&order.availability_reason)
        .bind(&order.department_rule_version)
        .bind(serde_json::Value::Array(order.crew_requirement_snapshot.clone()))
        .bind(serde_json::Value::Array(order.equipment_requirement_snapshot.clone()))
        .bind(&order.task_crew)
        .bind(serde_json::Value::Array(order.equipment_assignment.clone()))
        .bind(serde_json::Value::Array(order.qualification_gap.clone()))
        .bind(serde_json::Value::Array(order.equipment_gap.clone()))
        .bind(&order.score_breakdown)
        .bind(&order.conflict_reason)
        .bind(if order.recommended_assignees.is_empty() {
            None
        } else {
            Some(serde_json::Value::Array(order.recommended_assignees.clone()))
        })
        .bind(order.recommendation_score)
        .bind(order.supervisor_notified)
        .bind(order.supervisor_notified_at)
        .bind(order.assignment_deadline)
        .bind(&order.completed_by)
        .bind(&order.completion_notes)
        .bind(order.created_at)
        .bind(order.updated_at)
        .bind(&order.completion_time_mode)
        .bind(&order.completion_anchor_type)
        .bind(order.completion_anchor_time)
        .bind(order.completion_offset_minutes)
        .bind(order.completion_warning_lead_minutes)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    #[allow(dead_code)]
    async fn save_member_in_tx(
        tx: &mut Transaction<'_, Postgres>,
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

    async fn save_members_batch_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        members: &[DispatchOrderMember],
    ) -> Result<(), DomainError> {
        if members.is_empty() {
            return Ok(());
        }
        let mut builder = QueryBuilder::<Postgres>::new(
            "INSERT INTO dispatch_order_members (\
             id, dispatch_order_id, user_id, role, source_type, \
             source_team_id, slot_code, qualification_code, qualification_level_code, \
             assigned_at, check_in_time, check_out_time, is_active\
             ) ",
        );
        builder.push_values(members, |mut b, member| {
            b.push_bind(&member.id)
                .push_bind(&member.dispatch_order_id)
                .push_bind(&member.user_id)
                .push_bind(member.role.as_ref())
                .push_bind(member.source_type.as_ref())
                .push_bind(&member.source_team_id)
                .push_bind(&member.slot_code)
                .push_bind(&member.qualification_code)
                .push_bind(&member.qualification_level_code)
                .push_bind(member.assigned_at.unwrap_or_else(Utc::now))
                .push_bind(member.check_in_time)
                .push_bind(member.check_out_time)
                .push_bind(member.is_active);
        });
        builder.push(
            " ON CONFLICT (dispatch_order_id, user_id) DO UPDATE SET \
            role = EXCLUDED.role, source_type = EXCLUDED.source_type, \
            source_team_id = EXCLUDED.source_team_id, slot_code = EXCLUDED.slot_code, \
            qualification_code = EXCLUDED.qualification_code, \
            qualification_level_code = EXCLUDED.qualification_level_code, \
            is_active = EXCLUDED.is_active, \
            check_in_time = COALESCE(EXCLUDED.check_in_time, dispatch_order_members.check_in_time), \
            check_out_time = COALESCE(EXCLUDED.check_out_time, dispatch_order_members.check_out_time)",
        );
        builder
            .build()
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn append_log_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) -> Result<(), DomainError> {
        let touch_result = sqlx::query("UPDATE dispatch_orders SET updated_at = NOW() WHERE id = $1")
            .bind(dispatch_order_id)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        if touch_result.rows_affected() == 0 {
            return Err(DomainError::Internal(format!(
                "dispatch order not found while appending log: {dispatch_order_id}"
            )));
        }

        sqlx::query(
            "INSERT INTO dispatch_order_logs (id, dispatch_order_id, action, actor_id, details, created_at) \
             VALUES ($1, $2, $3, $4, $5, NOW())",
        )
        .bind(Self::new_dispatch_record_id())
        .bind(dispatch_order_id)
        .bind(action)
        .bind(actor_id)
        .bind(details)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn replace_order_equipment_assignments_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        id: &str,
        equipment_ids: &[String],
    ) -> Result<(), DomainError> {
        let mut seen_equipment_ids = HashSet::new();
        let mut unique_equipment_ids = Vec::with_capacity(equipment_ids.len());
        for equipment_id in equipment_ids {
            if !seen_equipment_ids.insert(equipment_id.as_str()) {
                return Err(DomainError::ValidationError(
                    "派工单设备分配不能包含重复设备".to_string(),
                ));
            }
            unique_equipment_ids.push(equipment_id.clone());
        }
        let equipment_ids = unique_equipment_ids;
        sqlx::query(
            r#"
            UPDATE equipment
            SET current_dispatch_id = NULL,
                status = CASE WHEN status = 'in_use' THEN 'available' ELSE status END,
                updated_at = NOW()
            WHERE current_dispatch_id = $1
              AND NOT (id = ANY($2))
            "#,
        )
        .bind(id)
        .bind(&equipment_ids)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        sqlx::query(
            r#"
            UPDATE dispatch_order_equipment
            SET released_at = NOW()
            WHERE dispatch_order_id = $1
              AND released_at IS NULL
              AND NOT (equipment_id = ANY($2))
            "#,
        )
        .bind(id)
        .bind(&equipment_ids)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if !equipment_ids.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO dispatch_order_equipment (
                    dispatch_order_id, equipment_id, assigned_at, released_at
                )
                SELECT $1, unnest($2::text[]), NOW(), NULL
                ON CONFLICT (dispatch_order_id, equipment_id) DO UPDATE SET
                    assigned_at = EXCLUDED.assigned_at,
                    released_at = NULL
                "#,
            )
            .bind(id)
            .bind(&equipment_ids)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

            let claimed = sqlx::query(
                r#"
                UPDATE equipment
                SET current_dispatch_id = $1,
                    status = 'in_use',
                    updated_at = NOW()
                WHERE id = ANY($2)
                  AND (current_dispatch_id IS NULL OR current_dispatch_id = $1)
                  AND (status = 'available' OR current_dispatch_id = $1)
                "#,
            )
            .bind(id)
            .bind(&equipment_ids)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
            if claimed.rows_affected() != equipment_ids.len() as u64 {
                return Err(DomainError::BusinessRuleViolation(
                    "设备状态已发生变化，无法完成分配，请刷新后重试".to_string(),
                ));
            }
        }

        Ok(())
    }

    async fn release_order_equipment_assignments_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        id: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE dispatch_order_equipment
            SET released_at = NOW()
            WHERE dispatch_order_id = $1
              AND released_at IS NULL
            "#,
        )
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        sqlx::query(
            r#"
            UPDATE equipment
            SET current_dispatch_id = NULL,
                status = CASE WHEN status = 'in_use' THEN 'available' ELSE status END,
                updated_at = NOW()
            WHERE current_dispatch_id = $1
            "#,
        )
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn persist_order_command_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        command: CreateDispatchOrderCommand,
    ) -> Result<(), DomainError> {
        let CreateDispatchOrderCommand {
            order,
            members,
            persist_equipment_assignments,
            equipment_ids,
            log_action,
            log_actor_id,
            log_details,
        } = command;

        Self::save_order_in_tx(tx, &order).await?;

        let active_user_ids = members.iter().map(|member| member.user_id.clone()).collect::<Vec<_>>();
        sqlx::query(
            "UPDATE dispatch_order_members SET is_active = FALSE \
             WHERE dispatch_order_id = $1 AND is_active = TRUE AND NOT (user_id = ANY($2))",
        )
        .bind(&order.id)
        .bind(&active_user_ids)
        .execute(&mut **tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Self::save_members_batch_in_tx(tx, &members).await?;

        if persist_equipment_assignments {
            Self::replace_order_equipment_assignments_in_tx(tx, &order.id, &equipment_ids).await?;
        }

        Self::append_log_in_tx(tx, &order.id, &log_action, log_actor_id.as_deref(), log_details).await
    }

    fn base_order_select() -> &'static str {
        r#"
            SELECT
                d.id, d.flight_id, d.task_type, d.stand_id,
                d.individual_user_id,
                d.driver_type, d.driver_user_id,
                d.planned_start_time, d.planned_end_time,
                d.actual_start_time, d.actual_end_time,
                d.estimated_completion_time,
                d.estimated_completion_reported_by,
                d.estimated_completion_reported_at,
                d.estimated_completion_note,
                d.status, d.dispatch_type, d.dispatched_at, d.dispatched_by,
                d.snapshot_assignee_position, d.snapshot_equipment_positions,
                d.estimated_arrival_minutes,
                d.process_instance_id, d.process_task_id,
                d.workflow_context, d.workflow_status, d.source,
                d.schedule_source, d.lock_level,
                d.publication_state, d.source_type, d.department_id, d.leg_scope,
                d.generation_rule_id, d.generation_rule_version,
                d.generation_anchor_type, d.generation_anchor_time,
                d.publish_trigger_mode, d.publish_at,
                d.turnaround_pair_key, d.turnaround_constraint_mode,
                d.availability_reason, d.department_rule_version,
                d.crew_requirement_snapshot, d.equipment_requirement_snapshot,
                d.task_crew, d.equipment_assignment,
                d.qualification_gap, d.equipment_gap,
                d.score_breakdown, d.conflict_reason,
                d.recommended_assignees, d.recommendation_score,
                d.supervisor_notified, d.supervisor_notified_at,
                d.assignment_deadline, d.completed_by, d.completion_notes,
                d.created_at, d.updated_at,
                COALESCE(
                    d.workflow_context->>'target_department',
                    iu.department
                ) AS department,
                iu.username AS individual_username,
                s.code AS stand_code,
                st.name AS task_type_name,
                COALESCE(f.gate, d.workflow_context->>'gate') AS gate,
                COALESCE(
                    s.terminal,
                    f.terminal,
                    d.workflow_context->>'terminal'
                ) AS terminal,
                COALESCE(f.flight_number, fl.flight_no) AS flight_no
            FROM dispatch_orders d
            LEFT JOIN users iu ON d.individual_user_id = iu.id
            LEFT JOIN stands s ON d.stand_id = s.id
            LEFT JOIN task_types st ON d.task_type = st.code
            LEFT JOIN flights f ON f.flight_id = d.flight_id
            LEFT JOIN LATERAL (
                SELECT leg.flight_no
                FROM flight_legs leg
                WHERE leg.flight_id = d.flight_id
                ORDER BY
                    CASE WHEN leg.leg_type = 'outbound' THEN 0 ELSE 1 END,
                    leg.updated_at DESC NULLS LAST,
                    leg.created_at DESC NULLS LAST
                LIMIT 1
            ) fl ON TRUE
        "#
    }

    async fn fetch_orders(
        &self,
        mut builder: QueryBuilder<'_, Postgres>,
        load_members: bool,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut orders: Vec<DispatchOrder> = rows.iter().map(row_to_order).collect();
        if load_members {
            self.hydrate_orders(&mut orders).await?;
        }
        Ok(orders)
    }

    async fn hydrate_orders(&self, orders: &mut [DispatchOrder]) -> Result<(), DomainError> {
        let order_ids: Vec<String> = orders.iter().map(|item| item.id.clone()).collect();
        if order_ids.is_empty() {
            return Ok(());
        }

        let members_by_order_id = self.load_members_by_order_ids(&order_ids).await?;
        let equipment_by_order_id = self.load_equipment_by_order_ids(&order_ids).await?;

        for order in orders.iter_mut() {
            order.members = members_by_order_id.get(&order.id).cloned().unwrap_or_default();
            order.equipment_list = equipment_by_order_id.get(&order.id).cloned().unwrap_or_default();
        }

        Ok(())
    }

    async fn load_members_by_order_ids(
        &self,
        order_ids: &[String],
    ) -> Result<HashMap<String, Vec<DispatchOrderMember>>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
                SELECT
                    dom.*,
                    u.username
                FROM dispatch_order_members dom
                LEFT JOIN users u ON u.id = dom.user_id
                WHERE dom.is_active = TRUE
                  AND dom.dispatch_order_id IN (
            "#,
        );
        let mut separated = builder.separated(", ");
        for order_id in order_ids {
            separated.push_bind(order_id);
        }
        separated.push_unseparated(")");
        builder.push(" ORDER BY dom.dispatch_order_id ASC, dom.assigned_at ASC NULLS LAST");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut result: HashMap<String, Vec<DispatchOrderMember>> = HashMap::new();
        for row in rows {
            let order_id: String = row
                .try_get("dispatch_order_id")
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            result.entry(order_id).or_default().push(row_to_member(&row));
        }
        Ok(result)
    }

    async fn load_equipment_by_order_ids(
        &self,
        order_ids: &[String],
    ) -> Result<HashMap<String, Vec<Equipment>>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
                SELECT
                    doe.dispatch_order_id,
                    e.*
                FROM dispatch_order_equipment doe
                JOIN equipment e ON e.id = doe.equipment_id
                WHERE doe.released_at IS NULL
                  AND doe.dispatch_order_id IN (
            "#,
        );
        let mut separated = builder.separated(", ");
        for order_id in order_ids {
            separated.push_bind(order_id);
        }
        separated.push_unseparated(")");
        builder.push(" ORDER BY doe.dispatch_order_id ASC, doe.assigned_at ASC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut result: HashMap<String, Vec<Equipment>> = HashMap::new();
        for row in rows {
            let order_id: String = row
                .try_get("dispatch_order_id")
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            result.entry(order_id).or_default().push(row_to_equipment(&row));
        }
        Ok(result)
    }

    fn apply_department_filter<'a>(
        mut builder: QueryBuilder<'a, Postgres>,
        department: Option<&'a str>,
    ) -> QueryBuilder<'a, Postgres> {
        if let Some(department) = department {
            builder.push(" AND COALESCE(d.workflow_context->>'target_department', tu.department, iu.department) = ");
            builder.push_bind(department.to_owned());
        }
        builder
    }

    fn order_window_start_expr() -> &'static str {
        "COALESCE(d.actual_start_time, d.planned_start_time, d.created_at)"
    }

    fn order_window_end_expr() -> &'static str {
        "COALESCE(d.actual_end_time, d.planned_end_time, d.planned_start_time, d.created_at)"
    }
}

#[async_trait]
impl DispatchOrderRepository for PgDispatchOrderRepository {
    async fn save(&self, order: &DispatchOrder) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Self::save_order_in_tx(&mut tx, order).await?;
        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn create_order_atomic(&self, command: CreateDispatchOrderCommand) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Self::persist_order_command_in_tx(&mut tx, command).await?;
        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn save_orders_atomic(&self, commands: Vec<CreateDispatchOrderCommand>) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        for command in commands {
            Self::persist_order_command_in_tx(&mut tx, command).await?;
        }
        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: &str,
        load_members: bool,
        department: Option<&str>,
    ) -> Result<Option<DispatchOrder>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(Self::base_order_select());
        builder.push(" WHERE d.id = ");
        builder.push_bind(id);
        builder = Self::apply_department_filter(builder, department);

        let row = builder
            .build()
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let mut order = row_to_order(&row);
        if load_members {
            let mut orders = vec![order];
            self.hydrate_orders(&mut orders).await?;
            order = orders.remove(0);
        }
        Ok(Some(order))
    }

    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
        self.find_by_flight_with_filters(flight_id, None, None, None, 200, 0)
            .await
    }

    async fn find_by_flight_with_filters(
        &self,
        flight_id: &str,
        status: Option<&str>,
        source: Option<&str>,
        department: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(Self::base_order_select());
        builder.push(" WHERE d.flight_id = ");
        builder.push_bind(flight_id);
        if let Some(status) = status {
            builder.push(" AND d.status = ");
            builder.push_bind(status);
        }
        if let Some(source) = source {
            builder.push(" AND d.source = ");
            builder.push_bind(source);
        }
        builder = Self::apply_department_filter(builder, department);
        builder.push(" ORDER BY d.created_at DESC");
        builder.push(" LIMIT ");
        builder.push_bind(limit.max(1));
        builder.push(" OFFSET ");
        builder.push_bind(offset.max(0));

        self.fetch_orders(builder, true).await
    }

    async fn find_by_user(&self, user_id: &str, status: Option<&str>) -> Result<Vec<DispatchOrder>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(Self::base_order_select());
        builder.push(
            " LEFT JOIN dispatch_order_members dom_u ON dom_u.dispatch_order_id = d.id \
             WHERE (d.individual_user_id = ",
        );
        builder.push_bind(user_id);
        builder.push(" OR dom_u.user_id = ");
        builder.push_bind(user_id);
        builder.push(")");
        if let Some(status) = status {
            builder.push(" AND d.status = ");
            builder.push_bind(status);
        }
        builder.push(" ORDER BY d.planned_start_time DESC NULLS LAST, d.created_at DESC");
        builder.push(" LIMIT 1000");

        self.fetch_orders(builder, true).await
    }

    async fn find_all(
        &self,
        status: Option<&str>,
        department: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        self.find_all_filtered(status, None, department, limit, offset).await
    }

    async fn find_all_filtered(
        &self,
        status: Option<&str>,
        source: Option<&str>,
        department: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(Self::base_order_select());
        builder.push(" WHERE 1=1");
        if let Some(status) = status {
            builder.push(" AND d.status = ");
            builder.push_bind(status);
        }
        if let Some(source) = source {
            builder.push(" AND d.source = ");
            builder.push_bind(source);
        }
        builder = Self::apply_department_filter(builder, department);
        builder.push(" ORDER BY d.created_at DESC");
        builder.push(" LIMIT ");
        builder.push_bind(limit.max(1));
        builder.push(" OFFSET ");
        builder.push_bind(offset.max(0));

        self.fetch_orders(builder, true).await
    }

    async fn find_orders_in_window(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        statuses: &[&str],
        source: Option<&str>,
        department: Option<&str>,
        terminal: Option<&str>,
        include_cancelled: bool,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(Self::base_order_select());
        builder.push(" WHERE ");
        builder.push(Self::order_window_end_expr());
        builder.push(" >= ");
        builder.push_bind(window_start);
        builder.push(" AND ");
        builder.push(Self::order_window_start_expr());
        builder.push(" <= ");
        builder.push_bind(window_end);

        let normalized_statuses = statuses
            .iter()
            .map(|status| status.trim())
            .filter(|status| !status.is_empty())
            .collect::<Vec<_>>();

        if !include_cancelled && !normalized_statuses.iter().any(|status| *status == "cancelled") {
            builder.push(" AND d.status != 'cancelled'");
        }
        if !normalized_statuses.is_empty() {
            builder.push(" AND d.status IN (");
            let mut sep = builder.separated(", ");
            for status in normalized_statuses {
                sep.push_bind(status.to_string());
            }
            sep.push_unseparated(")");
        }
        if let Some(source) = source {
            builder.push(" AND d.source = ");
            builder.push_bind(source);
        }
        if let Some(terminal) = terminal {
            builder.push(" AND COALESCE(s.terminal, t.terminal, f.terminal, d.workflow_context->>'terminal') = ");
            builder.push_bind(terminal);
        }
        builder = Self::apply_department_filter(builder, department);
        builder.push(" ORDER BY ");
        builder.push(Self::order_window_start_expr());
        builder.push(" LIMIT 5000");

        self.fetch_orders(builder, true).await
    }

    async fn find_overlapping_orders(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        individual_user_id: Option<&str>,
        stand_id: Option<&str>,
        exclude_order_id: Option<&str>,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(Self::base_order_select());
        builder.push(" WHERE d.planned_start_time < ");
        builder.push_bind(window_end);
        builder.push(" AND d.planned_end_time > ");
        builder.push_bind(window_start);
        builder.push(" AND d.status NOT IN ('cancelled', 'completed')");
        if let Some(uid) = individual_user_id {
            builder.push(" AND d.individual_user_id = ");
            builder.push_bind(uid);
        }
        if let Some(sid) = stand_id {
            builder.push(" AND d.stand_id = ");
            builder.push_bind(sid);
        }
        if let Some(eid) = exclude_order_id {
            builder.push(" AND d.id != ");
            builder.push_bind(eid);
        }
        builder.push(" LIMIT 1000");

        self.fetch_orders(builder, false).await
    }

    async fn find_equipment_conflicts(
        &self,
        equipment_ids: &[String],
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        exclude_order_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, DomainError> {
        if equipment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT doe.equipment_id, doe.dispatch_order_id, d.status, \
             d.planned_start_time, d.planned_end_time \
             FROM dispatch_order_equipment doe \
             JOIN dispatch_orders d ON d.id = doe.dispatch_order_id \
             WHERE d.status NOT IN ('cancelled', 'completed') \
             AND d.planned_start_time < ",
        );
        builder.push_bind(window_end);
        builder.push(" AND d.planned_end_time > ");
        builder.push_bind(window_start);
        builder.push(" AND doe.equipment_id IN (");
        let mut sep = builder.separated(", ");
        for eid in equipment_ids {
            sep.push_bind(eid);
        }
        sep.push_unseparated(")");
        if let Some(eid) = exclude_order_id {
            builder.push(" AND d.id != ");
            builder.push_bind(eid);
        }

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "equipment_id": r.get::<Option<String>, _>("equipment_id"),
                    "dispatch_order_id": r.get::<Option<String>, _>("dispatch_order_id"),
                    "status": r.get::<Option<String>, _>("status"),
                    "planned_start_time": r.get::<Option<DateTime<Utc>>, _>("planned_start_time"),
                    "planned_end_time": r.get::<Option<DateTime<Utc>>, _>("planned_end_time"),
                })
            })
            .collect())
    }

    async fn list_logs(&self, dispatch_order_id: &str, limit: i64) -> Result<Vec<serde_json::Value>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, dispatch_order_id, action, actor_id, details, created_at \
             FROM dispatch_order_logs WHERE dispatch_order_id = $1 \
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(dispatch_order_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.get::<Option<String>, _>("id"),
                    "dispatch_order_id": r.get::<Option<String>, _>("dispatch_order_id"),
                    "action": r.get::<Option<String>, _>("action"),
                    "actor_id": r.get::<Option<String>, _>("actor_id"),
                    "details": r.get::<Option<serde_json::Value>, _>("details"),
                    "created_at": r.get::<Option<DateTime<Utc>>, _>("created_at"),
                })
            })
            .collect())
    }

    async fn find_pending_for_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(Self::base_order_select());
        builder.push(" WHERE d.flight_id = ");
        builder.push_bind(flight_id);
        builder.push(" AND d.status = 'pending'");
        builder.push(" ORDER BY d.planned_start_time");
        builder.push(" LIMIT 200");

        self.fetch_orders(builder, false).await
    }

    async fn find_publishable_orders(
        &self,
        as_of: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(Self::base_order_select());
        builder.push(
            " WHERE d.publication_state = 'prepublished' \
              AND d.status <> 'cancelled' \
              AND ( \
                    (d.publish_trigger_mode = 'time' AND d.publish_at IS NOT NULL AND d.publish_at <= ",
        );
        builder.push_bind(as_of);
        builder.push(
            ") \
                 OR (d.publish_trigger_mode = 'either' AND d.publish_at IS NOT NULL AND d.publish_at <= ",
        );
        builder.push_bind(as_of);
        builder.push(
            ") \
              ) \
              ORDER BY d.publish_at ASC NULLS LAST, d.planned_start_time ASC NULLS LAST, d.created_at ASC \
              LIMIT ",
        );
        builder.push_bind(limit.max(1));

        self.fetch_orders(builder, false).await
    }

    async fn update_status(
        &self,
        id: &str,
        status: &str,
        actor_id: Option<&str>,
        enforce_actor_assignment: bool,
    ) -> Result<bool, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let result = if enforce_actor_assignment {
            sqlx::query(
                "UPDATE dispatch_orders d \
                 SET status = $1, updated_at = NOW() \
                 WHERE d.id = $2 \
                   AND d.status NOT IN ('completed', 'cancelled') \
                   AND ( \
                        d.individual_user_id = $3 \
                        OR EXISTS ( \
                            SELECT 1 \
                            FROM dispatch_order_members dom \
                            WHERE dom.dispatch_order_id = d.id \
                              AND dom.user_id = $4 \
                              AND dom.is_active = TRUE \
                        ) \
                   )",
            )
            .bind(status)
            .bind(id)
            .bind(actor_id)
            .bind(actor_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
        } else {
            sqlx::query(
                "UPDATE dispatch_orders \
                 SET status = $1, updated_at = NOW() \
                 WHERE id = $2 AND status NOT IN ('completed', 'cancelled')",
            )
            .bind(status)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
        };

        if result.rows_affected() > 0 {
            if matches!(status, "cancelled" | "completed") {
                Self::release_order_equipment_assignments_in_tx(&mut tx, id).await?;
            }
            let log_id = Self::new_dispatch_record_id();
            let event_id = Self::new_dispatch_record_id();
            sqlx::query(
                "INSERT INTO dispatch_order_logs (id, dispatch_order_id, action, actor_id, details, created_at) \
                 VALUES ($1, $2, $3, $4, $5, NOW())",
            )
            .bind(&log_id)
            .bind(id)
            .bind(format!("status_changed_to_{}", status))
            .bind(actor_id)
            .bind(serde_json::json!({ "event_id": event_id }))
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn start_order(&self, id: &str, actual_start: DateTime<Utc>, actor_id: &str) -> Result<bool, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE dispatch_orders \
             SET status = 'in_progress', actual_start_time = $1, updated_at = NOW() \
             WHERE id = $2 AND status = 'assigned'",
        )
        .bind(actual_start)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if result.rows_affected() > 0 {
            let log_id = Self::new_dispatch_record_id();
            let event_id = Self::new_dispatch_record_id();
            let correlation_id = Self::new_dispatch_record_id();
            sqlx::query(
                "INSERT INTO dispatch_order_logs (id, dispatch_order_id, action, actor_id, details, created_at) \
                 VALUES ($1, $2, 'started', $3, $4, NOW())",
            )
            .bind(&log_id)
            .bind(id)
            .bind(actor_id)
            .bind(serde_json::json!({
                "event_id": event_id,
                "correlation_id": correlation_id,
                "actual_start_time": actual_start,
            }))
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn complete_order(
        &self,
        id: &str,
        actual_end: DateTime<Utc>,
        actor_id: &str,
        notes: Option<&str>,
    ) -> Result<bool, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE dispatch_orders SET status = 'completed', actual_end_time = $1, \
             completed_by = $2, completion_notes = $3, updated_at = NOW() \
             WHERE id = $4 AND status = 'in_progress'",
        )
        .bind(actual_end)
        .bind(actor_id)
        .bind(notes)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if result.rows_affected() > 0 {
            Self::release_order_equipment_assignments_in_tx(&mut tx, id).await?;
            let log_id = Self::new_dispatch_record_id();
            let event_id = Self::new_dispatch_record_id();
            let correlation_id = Self::new_dispatch_record_id();
            sqlx::query(
                "INSERT INTO dispatch_order_logs (id, dispatch_order_id, action, actor_id, details, created_at) \
                 VALUES ($1, $2, 'completed', $3, $4, NOW())",
            )
            .bind(&log_id)
            .bind(id)
            .bind(actor_id)
            .bind(serde_json::json!({
                "event_id": event_id,
                "correlation_id": correlation_id,
                "actual_end_time": actual_end,
                "completion_notes": notes,
            }))
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn append_log(
        &self,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Self::append_log_in_tx(&mut tx, dispatch_order_id, action, actor_id, details).await?;
        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn append_log_once(
        &self,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        details: serde_json::Value,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "INSERT INTO dispatch_order_logs (id, dispatch_order_id, action, actor_id, details, created_at) \
             VALUES ($1, $2, $3, $4, $5, NOW()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(Self::new_dispatch_record_id())
        .bind(dispatch_order_id)
        .bind(action)
        .bind(actor_id)
        .bind(details)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn has_logged_action(
        &self,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        client_action_id: Option<&str>,
    ) -> Result<bool, DomainError> {
        let row: (bool,) = sqlx::query_as(
            "SELECT EXISTS(\
               SELECT 1 FROM dispatch_order_logs \
               WHERE dispatch_order_id = $1 AND action = $2 \
                 AND ($3::text IS NULL OR details->>'client_action_id' = $3) \
                 AND ($4::text IS NULL OR actor_id = $4)\
             )",
        )
        .bind(dispatch_order_id)
        .bind(action)
        .bind(client_action_id)
        .bind(actor_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.0)
    }

    async fn report_estimated_completion(
        &self,
        id: &str,
        estimated_time: DateTime<Utc>,
        actor_id: &str,
        note: Option<&str>,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE dispatch_orders SET \
             estimated_completion_time = $1, \
             estimated_completion_reported_by = $2, \
             estimated_completion_reported_at = NOW(), \
             estimated_completion_note = $3, \
             supervisor_notified = TRUE, \
             supervisor_notified_at = NOW(), \
             updated_at = NOW() \
             WHERE id = $4 AND status = 'in_progress'",
        )
        .bind(estimated_time)
        .bind(actor_id)
        .bind(note)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_planned_times(
        &self,
        id: &str,
        planned_start: DateTime<Utc>,
        planned_end: DateTime<Utc>,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"UPDATE dispatch_orders
               SET planned_start_time = $2,
                   planned_end_time   = $3,
                   updated_at         = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(planned_start)
        .bind(planned_end)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn replace_order_equipment_assignments(&self, id: &str, equipment_ids: &[String]) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Self::replace_order_equipment_assignments_in_tx(&mut tx, id, equipment_ids).await?;
        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))
    }
}

#[async_trait]
impl<'tx> DispatchOrderTransactionalRepository<Transaction<'tx, Postgres>> for PgDispatchOrderRepository {
    async fn save_in_tx(&self, tx: &mut Transaction<'tx, Postgres>, order: &DispatchOrder) -> Result<(), DomainError> {
        Self::save_order_in_tx(&mut *tx, order).await
    }

    async fn append_log_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) -> Result<(), DomainError> {
        PgDispatchOrderRepository::append_log_in_tx(&mut *tx, dispatch_order_id, action, actor_id, details).await
    }

    async fn replace_order_equipment_assignments_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        id: &str,
        equipment_ids: &[String],
    ) -> Result<(), DomainError> {
        PgDispatchOrderRepository::replace_order_equipment_assignments_in_tx(&mut *tx, id, equipment_ids).await
    }
}

// ---------------------------------------------------------------------------
// Row mapping helpers
// ---------------------------------------------------------------------------

fn row_to_order(row: &PgRow) -> DispatchOrder {
    DispatchOrder {
        id: row.get("id"),
        flight_id: row.get("flight_id"),
        task_type: row.get("task_type"),
        stand_id: row.get("stand_id"),
        task_type_name: row.try_get("task_type_name").ok().flatten(),
        stand_code: row.try_get("stand_code").ok().flatten(),
        terminal: row.try_get("terminal").ok().flatten(),
        department: row.try_get("department").ok().flatten(),
        individual_user_id: row.get("individual_user_id"),
        individual_username: row.try_get("individual_username").ok().flatten(),
        driver_type: row
            .get::<Option<String>, _>("driver_type")
            .as_deref()
            .map(|value| parse_assignee_type(Some(value))),
        driver_user_id: row.get("driver_user_id"),
        planned_start_time: row.get("planned_start_time"),
        planned_end_time: row.get("planned_end_time"),
        actual_start_time: row.get("actual_start_time"),
        actual_end_time: row.get("actual_end_time"),
        estimated_completion_time: row.get("estimated_completion_time"),
        estimated_completion_reported_by: row.get("estimated_completion_reported_by"),
        estimated_completion_reported_at: row.get("estimated_completion_reported_at"),
        estimated_completion_note: row.get("estimated_completion_note"),
        status: parse_order_status(row.get::<Option<String>, _>("status").as_deref()),
        dispatch_type: parse_dispatch_type(row.get::<Option<String>, _>("dispatch_type").as_deref()),
        dispatched_at: row.get("dispatched_at"),
        dispatched_by: row.get("dispatched_by"),
        snapshot_assignee_position: row.get("snapshot_assignee_position"),
        snapshot_equipment_positions: row
            .try_get::<Option<serde_json::Value>, _>("snapshot_equipment_positions")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok()),
        estimated_arrival_minutes: row.get("estimated_arrival_minutes"),
        process_instance_id: row.get("process_instance_id"),
        process_task_id: row.get("process_task_id"),
        workflow_context: row
            .try_get::<Option<serde_json::Value>, _>("workflow_context")
            .ok()
            .flatten()
            .unwrap_or_else(|| serde_json::Value::Object(Default::default())),
        workflow_status: row
            .get::<Option<String>, _>("workflow_status")
            .unwrap_or_else(|| "pending_assignment".into()),
        source: row
            .get::<Option<String>, _>("source")
            .unwrap_or_else(|| "system".into()),
        schedule_source: parse_schedule_source(row.get::<Option<String>, _>("schedule_source").as_deref()),
        lock_level: parse_lock_level(row.get::<Option<String>, _>("lock_level").as_deref()),
        publication_state: row
            .get::<Option<String>, _>("publication_state")
            .unwrap_or_else(|| "published".into()),
        source_type: row
            .get::<Option<String>, _>("source_type")
            .unwrap_or_else(|| "manual".into()),
        department_id: row.get("department_id"),
        leg_scope: row
            .get::<Option<String>, _>("leg_scope")
            .unwrap_or_else(|| "none".into()),
        generation_rule_id: row.get("generation_rule_id"),
        generation_rule_version: row.get("generation_rule_version"),
        generation_anchor_type: row.get("generation_anchor_type"),
        generation_anchor_time: row.get("generation_anchor_time"),
        completion_time_mode: row.try_get("completion_time_mode").ok().flatten(),
        completion_anchor_type: row.try_get("completion_anchor_type").ok().flatten(),
        completion_anchor_time: row.try_get("completion_anchor_time").ok().flatten(),
        completion_offset_minutes: row.try_get("completion_offset_minutes").ok().flatten(),
        completion_warning_lead_minutes: row.try_get("completion_warning_lead_minutes").ok().flatten(),
        publish_trigger_mode: row.get("publish_trigger_mode"),
        publish_at: row.get("publish_at"),
        turnaround_pair_key: row.get("turnaround_pair_key"),
        turnaround_constraint_mode: row.get("turnaround_constraint_mode"),
        department_rule_version: row.get("department_rule_version"),
        crew_requirement_snapshot: row
            .try_get::<Option<serde_json::Value>, _>("crew_requirement_snapshot")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        equipment_requirement_snapshot: row
            .try_get::<Option<serde_json::Value>, _>("equipment_requirement_snapshot")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        task_crew: row
            .try_get::<Option<serde_json::Value>, _>("task_crew")
            .ok()
            .flatten()
            .unwrap_or_else(|| serde_json::Value::Object(Default::default())),
        equipment_assignment: row
            .try_get::<Option<serde_json::Value>, _>("equipment_assignment")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        qualification_gap: row
            .try_get::<Option<serde_json::Value>, _>("qualification_gap")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        equipment_gap: row
            .try_get::<Option<serde_json::Value>, _>("equipment_gap")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        availability_reason: row.get("availability_reason"),
        score_breakdown: row
            .try_get::<Option<serde_json::Value>, _>("score_breakdown")
            .ok()
            .flatten()
            .unwrap_or_else(|| serde_json::Value::Object(Default::default())),
        conflict_reason: row.get("conflict_reason"),
        recommended_assignees: row
            .try_get::<Option<serde_json::Value>, _>("recommended_assignees")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        recommendation_score: row.get("recommendation_score"),
        supervisor_notified: row.get::<Option<bool>, _>("supervisor_notified").unwrap_or(false),
        supervisor_notified_at: row.get("supervisor_notified_at"),
        assignment_deadline: row.get("assignment_deadline"),
        completed_by: row.get("completed_by"),
        completion_notes: row.get("completion_notes"),
        gate: row.try_get("gate").ok().flatten(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        members: Vec::new(),
        equipment_list: Vec::new(),
    }
}

fn row_to_member(row: &PgRow) -> DispatchOrderMember {
    DispatchOrderMember {
        id: row.get("id"),
        dispatch_order_id: row.get("dispatch_order_id"),
        user_id: row.get("user_id"),
        role: match row.get::<Option<String>, _>("role").as_deref() {
            Some("leader") => MemberRole::Leader,
            Some("driver") => MemberRole::Driver,
            _ => MemberRole::Member,
        },
        source_type: parse_assignee_type(row.get::<Option<String>, _>("source_type").as_deref()),
        source_team_id: row.get("source_team_id"),
        slot_code: row.get("slot_code"),
        qualification_code: row.get("qualification_code"),
        qualification_level_code: row.get("qualification_level_code"),
        assigned_at: row.get("assigned_at"),
        check_in_time: row.get("check_in_time"),
        check_out_time: row.get("check_out_time"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        username: row.try_get("username").ok().flatten(),
    }
}

fn row_to_equipment(row: &PgRow) -> Equipment {
    Equipment {
        id: row.get("id"),
        code: row.get::<Option<String>, _>("code").unwrap_or_default(),
        equipment_type_id: row.get("equipment_type_id"),
        department_id: row.try_get("department_id").ok().flatten(),
        name: row.get("name"),
        license_plate: row.try_get("license_plate").ok().flatten(),
        status: match row.get::<Option<String>, _>("status").as_deref() {
            Some("in_use") => EquipmentStatus::InUse,
            Some("maintenance") => EquipmentStatus::Maintenance,
            Some("retired") => EquipmentStatus::Retired,
            _ => EquipmentStatus::Available,
        },
        current_position_lat: row.try_get("current_position_lat").ok().flatten(),
        current_position_lng: row.try_get("current_position_lng").ok().flatten(),
        current_stand_id: row.try_get("current_stand_id").ok().flatten(),
        last_position_update: row.try_get("last_position_update").ok().flatten(),
        current_dispatch_id: row.try_get("current_dispatch_id").ok().flatten(),
        last_maintenance_date: row.try_get("last_maintenance_date").ok().flatten(),
        next_maintenance_date: row.try_get("next_maintenance_date").ok().flatten(),
        metadata: row
            .try_get::<Option<serde_json::Value>, _>("metadata")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok()),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        equipment_type: None,
    }
}

fn parse_assignee_type(val: Option<&str>) -> AssigneeType {
    match val {
        Some("individual") => AssigneeType::Individual,
        _ => AssigneeType::Team,
    }
}

fn parse_order_status(val: Option<&str>) -> DispatchOrderStatus {
    match val {
        Some("assigned") => DispatchOrderStatus::Assigned,
        Some("in_progress") => DispatchOrderStatus::InProgress,
        Some("completed") => DispatchOrderStatus::Completed,
        Some("cancelled") => DispatchOrderStatus::Cancelled,
        _ => DispatchOrderStatus::Pending,
    }
}

fn parse_dispatch_type(val: Option<&str>) -> DispatchType {
    match val {
        Some("manual") => DispatchType::Manual,
        _ => DispatchType::Auto,
    }
}

fn parse_schedule_source(val: Option<&str>) -> ScheduleSource {
    match val {
        Some("shift_instance") => ScheduleSource::ShiftInstance,
        _ => ScheduleSource::CurrentStatusFallback,
    }
}

fn parse_lock_level(val: Option<&str>) -> DispatchLockLevel {
    match val {
        Some("active") => DispatchLockLevel::Active,
        Some("frozen") => DispatchLockLevel::Frozen,
        Some("manual_lock") => DispatchLockLevel::ManualLock,
        _ => DispatchLockLevel::Optimizable,
    }
}

fn assignee_type_value(at: AssigneeType) -> &'static str {
    match at {
        AssigneeType::Individual => "individual",
        AssigneeType::Team => "team",
    }
}

fn schedule_source_value(value: ScheduleSource) -> &'static str {
    match value {
        ScheduleSource::ShiftInstance => "shift_instance",
        ScheduleSource::CurrentStatusFallback => "current_status_fallback",
    }
}

fn lock_level_value(value: DispatchLockLevel) -> &'static str {
    match value {
        DispatchLockLevel::Active => "active",
        DispatchLockLevel::Frozen => "frozen",
        DispatchLockLevel::ManualLock => "manual_lock",
        DispatchLockLevel::Optimizable => "optimizable",
    }
}

fn dispatch_order_status_value(s: DispatchOrderStatus) -> &'static str {
    match s {
        DispatchOrderStatus::Pending => "pending",
        DispatchOrderStatus::Assigned => "assigned",
        DispatchOrderStatus::InProgress => "in_progress",
        DispatchOrderStatus::Completed => "completed",
        DispatchOrderStatus::Cancelled => "cancelled",
    }
}

fn dispatch_type_value(dt: DispatchType) -> &'static str {
    match dt {
        DispatchType::Manual => "manual",
        DispatchType::Auto => "auto",
    }
}

#[cfg(test)]
mod tests {
    use super::PgDispatchOrderRepository;
    use chrono::{DateTime, Duration, Utc};
    use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{PgPool, Row};
    use ulid::Ulid;

    async fn repository_from_test_database() -> (PgDispatchOrderRepository, PgPool) {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect TEST_DATABASE_URL");

        sqlx::raw_sql(
            r#"
            DROP TABLE IF EXISTS dispatch_order_logs;
            DROP TABLE IF EXISTS dispatch_order_equipment;
            DROP TABLE IF EXISTS equipment;
            DROP TABLE IF EXISTS dispatch_orders;

            CREATE TEMP TABLE dispatch_orders (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL DEFAULT 'pending',
                actual_start_time TIMESTAMPTZ,
                actual_end_time TIMESTAMPTZ,
                completed_by TEXT,
                completion_notes TEXT,
                updated_at TIMESTAMPTZ NOT NULL
            ) ON COMMIT PRESERVE ROWS;

            CREATE TEMP TABLE equipment (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                current_dispatch_id TEXT,
                updated_at TIMESTAMPTZ NOT NULL
            ) ON COMMIT PRESERVE ROWS;

            CREATE TEMP TABLE dispatch_order_equipment (
                dispatch_order_id TEXT NOT NULL,
                equipment_id TEXT NOT NULL,
                assigned_at TIMESTAMPTZ NOT NULL,
                released_at TIMESTAMPTZ,
                PRIMARY KEY (dispatch_order_id, equipment_id)
            ) ON COMMIT PRESERVE ROWS;

            CREATE TEMP TABLE dispatch_order_logs (
                id TEXT PRIMARY KEY,
                dispatch_order_id TEXT NOT NULL,
                action TEXT NOT NULL,
                actor_id TEXT,
                details JSONB,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            ) ON COMMIT PRESERVE ROWS;
            "#,
        )
        .execute(&pool)
        .await
        .expect("create temporary dispatch tables");

        (PgDispatchOrderRepository::new(pool.clone()), pool)
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL with PostgreSQL"]
    async fn equipment_stays_claimed_while_in_progress_and_releases_on_completion() {
        let (repo, pool) = repository_from_test_database().await;
        let order_id = format!("order-{}", Ulid::new());
        let equipment_id = format!("equipment-{}", Ulid::new());
        let now = Utc::now();
        sqlx::query("INSERT INTO dispatch_orders (id, status, updated_at) VALUES ($1, 'assigned', $2)")
            .bind(&order_id)
            .bind(now)
            .execute(&pool)
            .await
            .expect("insert assigned order");
        sqlx::query(
            "INSERT INTO equipment (id, status, current_dispatch_id, updated_at) \
             VALUES ($1, 'in_use', $2, $3)",
        )
        .bind(&equipment_id)
        .bind(&order_id)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert claimed equipment");
        sqlx::query(
            "INSERT INTO dispatch_order_equipment \
             (dispatch_order_id, equipment_id, assigned_at) VALUES ($1, $2, $3)",
        )
        .bind(&order_id)
        .bind(&equipment_id)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert active equipment assignment");

        repo.start_order(&order_id, now, "user-1").await.expect("start order");
        let started_equipment = sqlx::query("SELECT status, current_dispatch_id FROM equipment WHERE id = $1")
            .bind(&equipment_id)
            .fetch_one(&pool)
            .await
            .expect("load equipment after start");
        assert_eq!(started_equipment.get::<String, _>("status"), "in_use");
        assert_eq!(
            started_equipment.get::<Option<String>, _>("current_dispatch_id"),
            Some(order_id.clone())
        );

        repo.complete_order(&order_id, now + Duration::minutes(20), "user-1", None)
            .await
            .expect("complete order");
        let completed_equipment = sqlx::query("SELECT status, current_dispatch_id FROM equipment WHERE id = $1")
            .bind(&equipment_id)
            .fetch_one(&pool)
            .await
            .expect("load equipment after completion");
        assert_eq!(completed_equipment.get::<String, _>("status"), "available");
        assert_eq!(
            completed_equipment.get::<Option<String>, _>("current_dispatch_id"),
            None
        );
        let released_at: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT released_at FROM dispatch_order_equipment \
             WHERE dispatch_order_id = $1 AND equipment_id = $2",
        )
        .bind(&order_id)
        .bind(&equipment_id)
        .fetch_one(&pool)
        .await
        .expect("load assignment release time");
        assert!(
            released_at.is_some(),
            "completion must close the active equipment assignment"
        );
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL with PostgreSQL"]
    async fn append_log_touches_order_and_rolls_back_log_when_touch_fails() {
        let (repo, pool) = repository_from_test_database().await;
        let order_id = format!("order-{}", Ulid::new());
        let original_updated_at = Utc::now() - Duration::hours(1);

        sqlx::query("INSERT INTO dispatch_orders (id, updated_at) VALUES ($1, $2)")
            .bind(&order_id)
            .bind(original_updated_at)
            .execute(&pool)
            .await
            .expect("insert dispatch order");
        let original_updated_at: DateTime<Utc> =
            sqlx::query_scalar("SELECT updated_at FROM dispatch_orders WHERE id = $1")
                .bind(&order_id)
                .fetch_one(&pool)
                .await
                .expect("load persisted dispatch order timestamp");

        sqlx::raw_sql(
            r#"
            CREATE OR REPLACE FUNCTION pg_temp.reject_dispatch_order_touch()
            RETURNS trigger
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RAISE EXCEPTION 'dispatch order touch rejected';
            END;
            $$;

            CREATE TRIGGER dispatch_orders_reject_touch
            BEFORE UPDATE ON dispatch_orders
            FOR EACH ROW
            EXECUTE FUNCTION pg_temp.reject_dispatch_order_touch();
            "#,
        )
        .execute(&pool)
        .await
        .expect("install rejecting trigger");

        let rejected = repo
            .append_log(
                &order_id,
                "touch_rejected",
                Some("actor-1"),
                Some(json!({"step": "reject"})),
            )
            .await;
        assert!(rejected.is_err(), "append_log must fail when the order touch fails");

        let row = sqlx::query("SELECT updated_at FROM dispatch_orders WHERE id = $1")
            .bind(&order_id)
            .fetch_one(&pool)
            .await
            .expect("load dispatch order after rejected append");
        assert_eq!(
            row.get::<DateTime<Utc>, _>("updated_at"),
            original_updated_at,
            "order timestamp must remain unchanged when append_log rolls back"
        );

        let log_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dispatch_order_logs")
            .fetch_one(&pool)
            .await
            .expect("count logs after rejected append");
        assert_eq!(log_count, 0, "log insert must roll back with the failed order touch");

        sqlx::raw_sql("DROP TRIGGER dispatch_orders_reject_touch ON dispatch_orders;")
            .execute(&pool)
            .await
            .expect("drop rejecting trigger");

        repo.append_log(
            &order_id,
            "touch_accepted",
            Some("actor-1"),
            Some(json!({"step": "accept"})),
        )
        .await
        .expect("append log after touch succeeds");

        let row = sqlx::query(
            "SELECT d.updated_at, COUNT(l.id) AS log_count \
             FROM dispatch_orders d \
             LEFT JOIN dispatch_order_logs l ON l.dispatch_order_id = d.id \
             WHERE d.id = $1 \
             GROUP BY d.updated_at",
        )
        .bind(&order_id)
        .fetch_one(&pool)
        .await
        .expect("load dispatch order and logs after accepted append");

        assert!(
            row.get::<DateTime<Utc>, _>("updated_at") > original_updated_at,
            "append_log must advance the order updated_at timestamp"
        );
        assert_eq!(row.get::<i64, _>("log_count"), 1);
    }
}
