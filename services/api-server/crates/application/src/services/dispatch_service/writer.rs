//! 派工单受控写入方。
//!
//! `DispatchService` 的非事务侧保持原样；本写入方只在调用方开好的事务里写
//! `DispatchOrder`（及其成员），与 `TodoWriter` / `BusinessCaseWriter` 同形：
//! 方法体把 `&mut Tx` 转发给本来就对 `Tx` 泛型的仓储端口，`Tx` 由适配层选定。
//! 重型领域逻辑（`prepare_order_for_publication` 等）仍在 `DispatchService` 上，
//! 写入方通过持有的 `Arc<DispatchService>` 复用它，不复制第二份。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::ports::dispatch_repository::{
    DepartmentQualificationRepository, DispatchOrderMemberRepository, DispatchOrderMemberTransactionalRepository,
    DispatchOrderRepository, DispatchOrderTransactionalRepository, EquipmentRepository, PersonnelRuntimeRepository,
    QualificationGrantRepository,
};
use fms_domain::ports::user_repository::UserRepository;

use crate::schemas::dispatch_schemas::*;

use super::helpers::order_to_response;
use super::DispatchService;

pub struct DispatchOrderWriter<Tx> {
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    order_tx_repo: Arc<dyn DispatchOrderTransactionalRepository<Tx> + Send + Sync>,
    member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
    member_tx_repo: Arc<dyn DispatchOrderMemberTransactionalRepository<Tx> + Send + Sync>,
    qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
    qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
    personnel_runtime_repo: Arc<dyn PersonnelRuntimeRepository + Send + Sync>,
    user_repo: Arc<dyn UserRepository + Send + Sync>,
    equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
    dispatch_service: Arc<DispatchService>,
}

impl<Tx> DispatchOrderWriter<Tx> {
    pub fn new(
        order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        order_tx_repo: Arc<dyn DispatchOrderTransactionalRepository<Tx> + Send + Sync>,
        member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
        member_tx_repo: Arc<dyn DispatchOrderMemberTransactionalRepository<Tx> + Send + Sync>,
        qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
        qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
        personnel_runtime_repo: Arc<dyn PersonnelRuntimeRepository + Send + Sync>,
        user_repo: Arc<dyn UserRepository + Send + Sync>,
        equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
        dispatch_service: Arc<DispatchService>,
    ) -> Self {
        Self {
            order_repo,
            order_tx_repo,
            member_repo,
            member_tx_repo,
            qualification_grant_repo,
            qualification_repo,
            personnel_runtime_repo,
            user_repo,
            equipment_repo,
            dispatch_service,
        }
    }
}

impl<Tx: Send> DispatchOrderWriter<Tx> {
    pub async fn reassign_order_in_tx(
        &self,
        tx: &mut Tx,
        order_id: &str,
        assignee_id: &str,
        assignee_type: Option<&str>,
        actor_id: &str,
        assignment_patch: Option<&Value>,
    ) -> Result<DispatchOrderResponse, DomainError> {
        DispatchService::ensure_actor(actor_id)?;
        let assignee_id = assignee_id.trim();
        if assignee_id.is_empty() {
            return Err(DomainError::ValidationError("assignee_id is required".into()));
        }

        let Some(mut order) = self.order_repo.find_by_id(order_id, true, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };

        let mut assignment = DispatchService::assignment_from_order(&order);
        match assignee_type
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("individual")
        {
            "team" => {
                return Err(DomainError::ValidationError(
                    "班组指派已废止：请改用 assign_slot 按槽挂人".to_string(),
                ));
            }
            "individual" | "user" => {
                assignment["assignee_type"] = json!("individual");
                assignment["individual_user_id"] = json!(assignee_id);
                assignment["individual_username"] = assignment_patch
                    .and_then(|patch| patch.get("individual_username"))
                    .cloned()
                    .unwrap_or(Value::Null);
            }
            other => {
                return Err(DomainError::ValidationError(format!(
                    "unsupported assignee_type: {other}"
                )));
            }
        }

        if let Some(Value::Object(patch)) = assignment_patch {
            if let Some(target) = assignment.as_object_mut() {
                for (key, value) in patch {
                    target.insert(key.clone(), value.clone());
                }
            }
        }

        DispatchService::apply_assignment_json(&mut order, Some(&assignment));
        if order.status == DispatchOrderStatus::Pending {
            order.status = DispatchOrderStatus::Assigned;
        }
        order.dispatched_by = Some(actor_id.to_string());
        order.dispatched_at = order.dispatched_at.or_else(|| Some(Utc::now()));
        order.updated_at = Some(Utc::now());

        self.sync_assignment_members_in_tx(tx, &order, &assignment).await?;
        self.order_tx_repo.save_in_tx(tx, &order).await?;
        self.order_tx_repo
            .append_log_in_tx(
                tx,
                &order.id,
                "reassigned",
                Some(actor_id),
                Some(json!({
                    "assignee_id": assignee_id,
                    "assignee_type": assignment.get("assignee_type").unwrap_or(&Value::Null),
                    "source": "ai_action",
                })),
            )
            .await?;

        Ok(order_to_response(&order))
    }

    /// `DispatchOrder.update_status` 的受控写入口：
    /// 在调用方事务内更新派工单状态并写操作日志（与 outbox 同事务提交）。
    pub async fn update_order_status_in_tx(
        &self,
        tx: &mut Tx,
        order_id: &str,
        new_status: &str,
        actor_id: &str,
        notes: Option<&str>,
    ) -> Result<DispatchOrderResponse, DomainError> {
        DispatchService::ensure_actor(actor_id)?;
        let status = match new_status.trim() {
            "pending" => DispatchOrderStatus::Pending,
            "assigned" => DispatchOrderStatus::Assigned,
            "in_progress" => DispatchOrderStatus::InProgress,
            "completed" => DispatchOrderStatus::Completed,
            "cancelled" => DispatchOrderStatus::Cancelled,
            other => {
                return Err(DomainError::ValidationError(format!(
                    "invalid dispatch order status: {other}"
                )));
            }
        };

        let Some(mut order) = self.order_repo.find_by_id(order_id, false, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };

        let previous_status = order.status.as_ref().to_string();
        order.status = status;
        order.updated_at = Some(Utc::now());

        self.order_tx_repo.save_in_tx(tx, &order).await?;
        self.order_tx_repo
            .append_log_in_tx(
                tx,
                &order.id,
                "status_updated",
                Some(actor_id),
                Some(json!({
                    "previous_status": previous_status,
                    "new_status": new_status,
                    "notes": notes,
                    "source": "ai_action",
                })),
            )
            .await?;

        Ok(order_to_response(&order))
    }

    pub async fn publish_order_in_tx(&self, tx: &mut Tx, order_id: &str, actor_id: &str) -> Result<Value, DomainError> {
        DispatchService::ensure_actor(actor_id)?;
        let mut order =
            self.order_repo
                .find_by_id(order_id, true, None)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "DispatchOrder",
                    id: order_id.to_string(),
                })?;

        let mut published_orders = Vec::new();
        let mut skipped_orders = Vec::new();

        if order.publication_state.trim() != "prepublished" {
            skipped_orders.push(json!({
                "order_id": order.id,
                "flight_id": order.flight_id,
                "task_type": order.task_type,
                "reason": "工单不是预发布状态",
            }));
            return Ok(json!({
                "published_count": 0,
                "published_orders": published_orders,
                "skipped_orders": skipped_orders,
            }));
        }

        if DispatchService::order_has_required_assignments(&order).is_err() {
            let prepared = self.dispatch_service.prepare_order_for_publication(&mut order).await?;
            if prepared.get("updated").and_then(Value::as_bool).unwrap_or(false) {
                self.order_tx_repo.save_in_tx(tx, &order).await?;
            }
            if let Err(updated_reason) = DispatchService::order_has_required_assignments(&order) {
                let reason = prepared
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or(updated_reason);
                skipped_orders.push(json!({
                    "order_id": order.id,
                    "flight_id": order.flight_id,
                    "task_type": order.task_type,
                    "reason": reason,
                }));
                return Ok(json!({
                    "published_count": 0,
                    "published_orders": published_orders,
                    "skipped_orders": skipped_orders,
                }));
            }
        }

        order.publication_state = "published".to_string();
        self.order_tx_repo.save_in_tx(tx, &order).await?;

        published_orders.push(json!({
            "order_id": order.id,
            "flight_id": order.flight_id,
            "task_type": order.task_type,
            "publication_state": order.publication_state,
        }));

        Ok(json!({
            "published_count": 1,
            "published_orders": published_orders,
            "skipped_orders": skipped_orders,
        }))
    }

    async fn sync_assignment_members_in_tx(
        &self,
        tx: &mut Tx,
        order: &DispatchOrder,
        assignment: &Value,
    ) -> Result<(), DomainError> {
        let existing_members = self.member_repo.find_by_order(&order.id).await?;
        let desired_members = DispatchService::build_dispatch_members_from_assignment(order, assignment);
        let desired_by_user = desired_members
            .into_iter()
            .map(|member| (member.user_id.clone(), member))
            .collect::<std::collections::HashMap<_, _>>();

        for member in &existing_members {
            if let Some(desired) = desired_by_user.get(&member.user_id) {
                let mut updated = member.clone();
                updated.role = desired.role;
                updated.source_type = desired.source_type;
                updated.source_team_id = desired.source_team_id.clone();
                updated.slot_code = desired.slot_code.clone();
                updated.qualification_code = desired.qualification_code.clone();
                updated.qualification_level_code = desired.qualification_level_code.clone();
                updated.username = desired.username.clone();
                updated.is_active = true;
                self.member_tx_repo.save_in_tx(tx, &updated).await?;
            } else {
                let mut deactivated = member.clone();
                deactivated.is_active = false;
                self.member_tx_repo.save_in_tx(tx, &deactivated).await?;
            }
        }

        let existing_user_ids: std::collections::HashSet<&str> =
            existing_members.iter().map(|m| m.user_id.as_str()).collect();
        for (user_id, desired) in desired_by_user {
            if !existing_user_ids.contains(user_id.as_str()) {
                self.member_tx_repo.save_in_tx(tx, &desired).await?;
            }
        }

        Ok(())
    }

    /// `DispatchOrder.assign_slot`：把一名人员指派到工单命名槽（一槽一人）。
    ///
    /// PR5 命名槽指派。校验「同科室 / 在岗 / 具备该槽资质」，与预排生成共用资质口径
    /// （`QualificationGrantRepository` + `DepartmentQualificationRepository` 等级覆盖）。
    /// 写：先将该槽现有成员置为不活跃，再将 user 的成员行写入该槽（不存在则新增）。
    pub async fn assign_slot_in_tx(
        &self,
        tx: &mut Tx,
        order_id: &str,
        slot_code: &str,
        user_id: &str,
        actor_id: &str,
    ) -> Result<DispatchOrderResponse, DomainError> {
        DispatchService::ensure_actor(actor_id)?;
        let slot_code = slot_code.trim();
        let user_id = user_id.trim();
        if slot_code.is_empty() {
            return Err(DomainError::ValidationError("slot_code is required".into()));
        }
        if user_id.is_empty() {
            return Err(DomainError::ValidationError("user_id is required".into()));
        }

        let Some(mut order) = self.order_repo.find_by_id(order_id, false, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };
        self.assert_order_assignable(&order)?;

        let requirement = order
            .crew_requirement_snapshot
            .iter()
            .find(|item| item.get("slot_code").and_then(Value::as_str) == Some(slot_code))
            .cloned()
            .ok_or_else(|| DomainError::ValidationError(format!("工单 {order_id} 不存在槽位 {slot_code}")))?;
        self.validate_slot_personnel(&order, slot_code, &requirement, user_id)
            .await?;

        let now = Utc::now();
        let existing_members = self.member_repo.find_by_order(&order.id).await?;
        for member in existing_members
            .iter()
            .filter(|m| m.is_active && m.slot_code.as_deref() == Some(slot_code))
        {
            let mut deactivated = member.clone();
            deactivated.is_active = false;
            deactivated.check_out_time = Some(now);
            self.member_tx_repo.save_in_tx(tx, &deactivated).await?;
        }

        let qualification_code = requirement
            .get("qualification_code")
            .and_then(Value::as_str)
            .map(str::to_string);
        let qualification_level_code = requirement
            .get("min_level_code")
            .and_then(Value::as_str)
            .map(str::to_string);
        let username = self
            .user_repo
            .find_by_id(user_id)
            .await?
            .map(|user| user.username)
            .or_else(|| Some(user_id.to_string()));

        if let Some(existing) = existing_members.iter().find(|m| m.user_id == user_id) {
            let mut updated = existing.clone();
            updated.role = Self::member_role_from_slot(Some(slot_code));
            updated.source_type = AssigneeType::Individual;
            updated.source_team_id = None;
            updated.slot_code = Some(slot_code.to_string());
            updated.qualification_code = qualification_code.clone();
            updated.qualification_level_code = qualification_level_code.clone();
            updated.username = username.clone();
            updated.assigned_at = Some(now);
            updated.check_in_time = None;
            updated.check_out_time = None;
            updated.is_active = true;
            self.member_tx_repo.save_in_tx(tx, &updated).await?;
        } else {
            let new_member = DispatchOrderMember {
                id: ulid::Ulid::new().to_string(),
                dispatch_order_id: order.id.clone(),
                user_id: user_id.to_string(),
                role: Self::member_role_from_slot(Some(slot_code)),
                source_type: AssigneeType::Individual,
                source_team_id: None,
                slot_code: Some(slot_code.to_string()),
                qualification_code,
                qualification_level_code,
                assigned_at: Some(now),
                check_in_time: None,
                check_out_time: None,
                is_active: true,
                username,
            };
            self.member_tx_repo.save_in_tx(tx, &new_member).await?;
        }

        if order.status == DispatchOrderStatus::Pending {
            order.status = DispatchOrderStatus::Assigned;
        }
        order.dispatched_by = Some(actor_id.to_string());
        order.dispatched_at = order.dispatched_at.or(Some(now));
        order.updated_at = Some(now);
        self.order_tx_repo.save_in_tx(tx, &order).await?;
        self.order_tx_repo
            .append_log_in_tx(
                tx,
                &order.id,
                "assign_slot",
                Some(actor_id),
                Some(json!({ "slot_code": slot_code, "user_id": user_id })),
            )
            .await?;

        Ok(order_to_response(&order))
    }

    /// `DispatchOrder.unassign_slot`：把命名槽里的人员清掉（不删槽，只清人）。
    pub async fn unassign_slot_in_tx(
        &self,
        tx: &mut Tx,
        order_id: &str,
        slot_code: &str,
        actor_id: &str,
    ) -> Result<DispatchOrderResponse, DomainError> {
        DispatchService::ensure_actor(actor_id)?;
        let slot_code = slot_code.trim();
        if slot_code.is_empty() {
            return Err(DomainError::ValidationError("slot_code is required".into()));
        }

        let Some(mut order) = self.order_repo.find_by_id(order_id, false, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };
        self.assert_order_assignable(&order)?;
        if !order
            .crew_requirement_snapshot
            .iter()
            .any(|item| item.get("slot_code").and_then(Value::as_str) == Some(slot_code))
        {
            return Err(DomainError::ValidationError(format!(
                "工单 {order_id} 不存在槽位 {slot_code}"
            )));
        }

        let now = Utc::now();
        let existing_members = self.member_repo.find_by_order(&order.id).await?;
        for member in existing_members
            .iter()
            .filter(|m| m.is_active && m.slot_code.as_deref() == Some(slot_code))
        {
            let mut deactivated = member.clone();
            deactivated.is_active = false;
            deactivated.check_out_time = Some(now);
            self.member_tx_repo.save_in_tx(tx, &deactivated).await?;
        }

        order.updated_at = Some(now);
        self.order_tx_repo.save_in_tx(tx, &order).await?;
        self.order_tx_repo
            .append_log_in_tx(
                tx,
                &order.id,
                "unassign_slot",
                Some(actor_id),
                Some(json!({ "slot_code": slot_code })),
            )
            .await?;

        Ok(order_to_response(&order))
    }

    /// `DispatchOrder.add_slot`：给这张单的槽快照加一个命名槽（幂等：已存在则原样返回）。
    pub async fn add_slot_in_tx(
        &self,
        tx: &mut Tx,
        order_id: &str,
        slot_code: &str,
        slot_name: Option<&str>,
        actor_id: &str,
    ) -> Result<DispatchOrderResponse, DomainError> {
        DispatchService::ensure_actor(actor_id)?;
        let slot_code = slot_code.trim();
        if slot_code.is_empty() {
            return Err(DomainError::ValidationError("slot_code is required".into()));
        }

        let Some(mut order) = self.order_repo.find_by_id(order_id, false, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };
        self.assert_order_assignable(&order)?;

        let already_exists = order
            .crew_requirement_snapshot
            .iter()
            .any(|item| item.get("slot_code").and_then(Value::as_str) == Some(slot_code));
        let mut changed = false;
        if already_exists {
            // 幂等：槽已存在时仅更新展示名（若提供）。
            if let Some(name) = slot_name.map(str::trim).filter(|value| !value.is_empty()) {
                if let Some(item) = order
                    .crew_requirement_snapshot
                    .iter_mut()
                    .find(|item| item.get("slot_code").and_then(Value::as_str) == Some(slot_code))
                {
                    if item.get("slot_name").and_then(Value::as_str) != Some(name) {
                        item["slot_name"] = json!(name);
                        changed = true;
                    }
                }
            }
        } else {
            order.crew_requirement_snapshot.push(json!({
                "slot_code": slot_code,
                "slot_name": slot_name.map(str::trim).filter(|value| !value.is_empty()).unwrap_or(slot_code),
                "qualification_code": Value::Null,
                "required_count": 1,
            }));
            changed = true;
        }

        if changed {
            order.updated_at = Some(Utc::now());
            self.order_tx_repo.save_in_tx(tx, &order).await?;
            self.order_tx_repo
                .append_log_in_tx(
                    tx,
                    &order.id,
                    "add_slot",
                    Some(actor_id),
                    Some(json!({ "slot_code": slot_code })),
                )
                .await?;
        }

        Ok(order_to_response(&order))
    }

    /// `DispatchOrder.remove_slot`：从槽快照删掉一个命名槽，并清掉该槽上所有成员。
    pub async fn remove_slot_in_tx(
        &self,
        tx: &mut Tx,
        order_id: &str,
        slot_code: &str,
        actor_id: &str,
    ) -> Result<DispatchOrderResponse, DomainError> {
        DispatchService::ensure_actor(actor_id)?;
        let slot_code = slot_code.trim();
        if slot_code.is_empty() {
            return Err(DomainError::ValidationError("slot_code is required".into()));
        }

        let Some(mut order) = self.order_repo.find_by_id(order_id, false, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };
        self.assert_order_assignable(&order)?;

        let before = order.crew_requirement_snapshot.len();
        order
            .crew_requirement_snapshot
            .retain(|item| item.get("slot_code").and_then(Value::as_str) != Some(slot_code));
        if order.crew_requirement_snapshot.len() == before {
            return Err(DomainError::ValidationError(format!(
                "工单 {order_id} 不存在槽位 {slot_code}"
            )));
        }

        let now = Utc::now();
        let existing_members = self.member_repo.find_by_order(&order.id).await?;
        for member in existing_members
            .iter()
            .filter(|m| m.is_active && m.slot_code.as_deref() == Some(slot_code))
        {
            let mut deactivated = member.clone();
            deactivated.is_active = false;
            deactivated.check_out_time = Some(now);
            self.member_tx_repo.save_in_tx(tx, &deactivated).await?;
        }

        order.updated_at = Some(now);
        self.order_tx_repo.save_in_tx(tx, &order).await?;
        self.order_tx_repo
            .append_log_in_tx(
                tx,
                &order.id,
                "remove_slot",
                Some(actor_id),
                Some(json!({ "slot_code": slot_code })),
            )
            .await?;

        Ok(order_to_response(&order))
    }

    /// `Equipment.assign`：把设备指派到工单设备槽（与人员槽同一套领域模型/同一写入方）。
    /// 同步更新 `equipment_assignment` 快照列，并在同一事务内经
    /// `replace_order_equipment_assignments_in_tx` 回写 `dispatch_order_equipment` 与
    /// `equipment.current_dispatch_id`/`status`——禁止只改设备行或只改快照列。
    pub async fn assign_equipment_slot_in_tx(
        &self,
        tx: &mut Tx,
        order_id: &str,
        slot_code: &str,
        equipment_id: &str,
        actor_id: &str,
    ) -> Result<DispatchOrderResponse, DomainError> {
        DispatchService::ensure_actor(actor_id)?;
        let slot_code = slot_code.trim();
        let equipment_id = equipment_id.trim();
        if slot_code.is_empty() {
            return Err(DomainError::ValidationError("slot_code is required".into()));
        }
        if equipment_id.is_empty() {
            return Err(DomainError::ValidationError("equipment_id is required".into()));
        }

        let Some(mut order) = self.order_repo.find_by_id(order_id, false, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };
        self.assert_order_assignable(&order)?;

        let requirement = order
            .equipment_requirement_snapshot
            .iter()
            .find(|item| item.get("slot_code").and_then(Value::as_str) == Some(slot_code))
            .cloned()
            .ok_or_else(|| DomainError::ValidationError(format!("工单 {order_id} 不存在设备槽位 {slot_code}")))?;

        let equipment = self
            .equipment_repo
            .find_by_id(equipment_id)
            .await?
            .ok_or_else(|| DomainError::ValidationError(format!("设备 {equipment_id} 不存在")))?;
        // 槽位声明了设备类型时必须匹配（与生成期候选集同一校验口径）。
        if let Some(required_type) = requirement
            .get("equipment_type_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if equipment.equipment_type_id.as_deref() != Some(required_type) {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "设备 {equipment_id} 的类型与槽位 {slot_code} 要求的设备类型不一致"
                )));
            }
        }

        let now = Utc::now();
        // 一槽一机、一机一槽：先摘掉该槽旧设备与该设备在本单其它槽上的占用。
        order.equipment_assignment.retain(|entry| {
            entry.get("slot_code").and_then(Value::as_str) != Some(slot_code)
                && entry.get("equipment_id").and_then(Value::as_str) != Some(equipment_id)
        });
        order.equipment_assignment.push(json!({
            "slot_code": slot_code,
            "equipment_id": equipment.id,
            "equipment_code": equipment.code,
            "driver_user_id": Value::Null,
        }));

        // 仅设备落槽不把工单翻成 Assigned：派工语义以人员/责任方为准（与 reassign 一致）。
        order.updated_at = Some(now);
        self.order_tx_repo.save_in_tx(tx, &order).await?;
        self.order_tx_repo
            .replace_order_equipment_assignments_in_tx(tx, &order.id, &Self::equipment_ids_of(&order))
            .await?;
        self.order_tx_repo
            .append_log_in_tx(
                tx,
                &order.id,
                "assign_equipment_slot",
                Some(actor_id),
                Some(json!({ "slot_code": slot_code, "equipment_id": equipment_id })),
            )
            .await?;

        Ok(order_to_response(&order))
    }

    /// `Equipment.release`：把设备从工单设备槽释放（不删槽，只清设备占用）。
    /// `slot_code` 为空时摘掉该设备在本单上的全部槽位占用。
    pub async fn release_equipment_slot_in_tx(
        &self,
        tx: &mut Tx,
        order_id: &str,
        slot_code: Option<&str>,
        equipment_id: &str,
        actor_id: &str,
    ) -> Result<DispatchOrderResponse, DomainError> {
        DispatchService::ensure_actor(actor_id)?;
        let equipment_id = equipment_id.trim();
        if equipment_id.is_empty() {
            return Err(DomainError::ValidationError("equipment_id is required".into()));
        }
        let slot_code = slot_code.map(str::trim).filter(|value| !value.is_empty());

        let Some(mut order) = self.order_repo.find_by_id(order_id, false, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };
        self.assert_order_assignable(&order)?;

        let before = order.equipment_assignment.len();
        order.equipment_assignment.retain(|entry| {
            let equipment_matches = entry.get("equipment_id").and_then(Value::as_str) == Some(equipment_id);
            let slot_matches = slot_code.is_none() || entry.get("slot_code").and_then(Value::as_str) == slot_code;
            !(equipment_matches && slot_matches)
        });
        if order.equipment_assignment.len() == before {
            return Err(DomainError::ValidationError(format!(
                "设备 {equipment_id} 未指派到工单 {order_id}{}",
                slot_code.map(|code| format!(" 的槽位 {code}")).unwrap_or_default()
            )));
        }

        order.updated_at = Some(Utc::now());
        self.order_tx_repo.save_in_tx(tx, &order).await?;
        self.order_tx_repo
            .replace_order_equipment_assignments_in_tx(tx, &order.id, &Self::equipment_ids_of(&order))
            .await?;
        self.order_tx_repo
            .append_log_in_tx(
                tx,
                &order.id,
                "release_equipment_slot",
                Some(actor_id),
                Some(json!({ "slot_code": slot_code, "equipment_id": equipment_id })),
            )
            .await?;

        Ok(order_to_response(&order))
    }

    /// 工单 `equipment_assignment` 快照列里的全部设备 id（去重保序）。
    fn equipment_ids_of(order: &DispatchOrder) -> Vec<String> {
        let mut ids = Vec::new();
        for entry in &order.equipment_assignment {
            if let Some(id) = entry
                .get("equipment_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if !ids.iter().any(|existing| existing == id) {
                    ids.push(id.to_string());
                }
            }
        }
        ids
    }

    /// 已完结（completed/cancelled）的工单不允许改槽。
    fn assert_order_assignable(&self, order: &DispatchOrder) -> Result<(), DomainError> {
        if matches!(
            order.status,
            DispatchOrderStatus::Completed | DispatchOrderStatus::Cancelled
        ) {
            return Err(DomainError::BusinessRuleViolation(format!(
                "工单 {} 已{}，不允许修改槽位",
                order.id,
                order.status.as_ref()
            )));
        }
        Ok(())
    }

    /// 校验「同科室 / 在岗 / 具备该槽资质」。
    async fn validate_slot_personnel(
        &self,
        order: &DispatchOrder,
        slot_code: &str,
        requirement: &Value,
        user_id: &str,
    ) -> Result<(), DomainError> {
        let user = self
            .user_repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| DomainError::ValidationError(format!("人员 {user_id} 不存在")))?;

        // 同科室：个人账号 department_id 必须等于工单科室。
        if let (Some(order_dept), Some(user_dept)) = (&order.department_id, &user.department_id) {
            if order_dept != user_dept {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "人员 {user_id} 与工单科室不一致（cannot assign cross-department）"
                )));
            }
        }

        // 在岗：personnel_runtime 无行视为 off_duty → 拒绝。
        let runtime = self.personnel_runtime_repo.find_by_user(user_id).await?;
        if runtime
            .as_ref()
            .is_none_or(|r| r.current_status != PersonnelStatus::OnDuty)
        {
            return Err(DomainError::BusinessRuleViolation(format!(
                "人员 {user_id} 不在岗（off duty），不能指派到槽位 {slot_code}"
            )));
        }

        // 该槽无资质要求 → 通过。
        let qualification_code = match requirement.get("qualification_code").and_then(Value::as_str) {
            Some(code) if !code.is_empty() => code,
            _ => return Ok(()),
        };
        let min_level_code = requirement.get("min_level_code").and_then(Value::as_str);
        let department_id = order
            .department_id
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| DomainError::ValidationError("工单缺少部门上下文".to_string()))?;
        let at_time = order.planned_end_time.unwrap_or_else(Utc::now);

        let grants = self
            .qualification_grant_repo
            .find_by_department(department_id, Some(at_time), &[user_id.to_string()], false)
            .await?;
        if !grants.iter().any(|grant| {
            grant.qualification_code == qualification_code
                && grant.valid_to.is_none_or(|valid_to| valid_to >= at_time)
                && grant.valid_from.is_none_or(|valid_from| valid_from <= at_time)
        }) {
            return Err(DomainError::BusinessRuleViolation(format!(
                "人员 {user_id} 不满足槽位 {slot_code} 的资质要求 {qualification_code}"
            )));
        }

        // 级别覆盖：仅当要求了 min_level_code 时才校验级别层级。
        if let Some(min_level_code) = min_level_code {
            let levels = self
                .qualification_repo
                .list_levels(department_id, Some(qualification_code), false)
                .await?;
            let level_index = levels
                .into_iter()
                .map(|level| {
                    let mut covered = level.covered_level_codes.into_iter().collect::<HashSet<_>>();
                    covered.insert(level.level_code.clone());
                    (level.level_code, covered)
                })
                .collect::<HashMap<_, _>>();
            let qualified = grants.iter().any(|grant| {
                grant.qualification_code == qualification_code
                    && Self::level_covers_requirement(&level_index, &grant.level_code, Some(min_level_code))
            });
            if !qualified {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "人员 {user_id} 不满足槽位 {slot_code} 的资质级别要求 {qualification_code}:{min_level_code}"
                )));
            }
        }

        Ok(())
    }

    fn member_role_from_slot(slot_code: Option<&str>) -> MemberRole {
        match slot_code.map(str::trim).filter(|value| !value.is_empty()) {
            Some("lead") => MemberRole::Leader,
            Some("driver") => MemberRole::Driver,
            _ => MemberRole::Member,
        }
    }

    fn level_covers_requirement(
        level_index: &HashMap<String, HashSet<String>>,
        grant_level_code: &str,
        min_level_code: Option<&str>,
    ) -> bool {
        match min_level_code {
            Some(min) => level_index
                .get(grant_level_code)
                .is_some_and(|covered| covered.contains(min)),
            None => true,
        }
    }
}
