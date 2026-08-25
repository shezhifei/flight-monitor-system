//! 派工单受控写入方。
//!
//! `DispatchService` 的非事务侧保持原样；本写入方只在调用方开好的事务里写
//! `DispatchOrder`（及其成员），与 `TodoWriter` / `BusinessCaseWriter` 同形：
//! 方法体把 `&mut Tx` 转发给本来就对 `Tx` 泛型的仓储端口，`Tx` 由适配层选定。
//! 重型领域逻辑（`prepare_order_for_publication` 等）仍在 `DispatchService` 上，
//! 写入方通过持有的 `Arc<DispatchService>` 复用它，不复制第二份。

use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::ports::dispatch_repository::{
    DispatchOrderMemberRepository, DispatchOrderMemberTransactionalRepository, DispatchOrderRepository,
    DispatchOrderTransactionalRepository, TeamRepository,
};

use crate::schemas::dispatch_schemas::*;

use super::helpers::order_to_response;
use super::DispatchService;

pub struct DispatchOrderWriter<Tx> {
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    order_tx_repo: Arc<dyn DispatchOrderTransactionalRepository<Tx> + Send + Sync>,
    member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
    member_tx_repo: Arc<dyn DispatchOrderMemberTransactionalRepository<Tx> + Send + Sync>,
    team_repo: Arc<dyn TeamRepository + Send + Sync>,
    dispatch_service: Arc<DispatchService>,
}

impl<Tx> DispatchOrderWriter<Tx> {
    pub fn new(
        order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        order_tx_repo: Arc<dyn DispatchOrderTransactionalRepository<Tx> + Send + Sync>,
        member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
        member_tx_repo: Arc<dyn DispatchOrderMemberTransactionalRepository<Tx> + Send + Sync>,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        dispatch_service: Arc<DispatchService>,
    ) -> Self {
        Self {
            order_repo,
            order_tx_repo,
            member_repo,
            member_tx_repo,
            team_repo,
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
            .unwrap_or("team")
        {
            "team" => {
                assignment["assignee_type"] = json!("team");
                assignment["team_id"] = json!(assignee_id);
                assignment["team_name"] = match self.team_repo.find_by_id(assignee_id, false).await? {
                    Some(team) => json!(team.name),
                    None => Value::Null,
                };
                assignment["individual_user_id"] = Value::Null;
                assignment["individual_username"] = Value::Null;
            }
            "individual" | "user" => {
                assignment["assignee_type"] = json!("individual");
                assignment["individual_user_id"] = json!(assignee_id);
                assignment["individual_username"] = assignment_patch
                    .and_then(|patch| patch.get("individual_username"))
                    .cloned()
                    .unwrap_or(Value::Null);
                assignment["team_id"] = Value::Null;
                assignment["team_name"] = Value::Null;
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
}
