use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::anomaly::*;
use fms_domain::models::dispatch::*;

use crate::schemas::dispatch_schemas::*;

use super::helpers;
use super::{DispatchService, NULL_VALUE};

impl DispatchService {
    pub async fn start_order(
        &self,
        order_id: &str,
        dto: DispatchOrderStart,
        actor_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        Self::ensure_order_execution_published(&order, "开始执行")?;

        if order.status == DispatchOrderStatus::InProgress {
            self.ensure_actor_can_start_order(&order, order_id, actor_id, "无权操作此派工单")
                .await?;
            return Ok(Self::already_started_response(order.actual_start_time, Utc::now()));
        }

        if order.status != DispatchOrderStatus::Assigned {
            return Err(DomainError::InvalidStateTransition {
                from: format!("{:?}", order.status),
                to: "in_progress".to_string(),
            });
        }

        self.ensure_actor_can_start_order(&order, order_id, actor_id, "无权开始此派工单")
            .await?;

        if !self
            .order
            .order_repo
            .has_logged_action(order_id, "accepted", Some(actor_id), None)
            .await?
        {
            return Err(DomainError::BusinessRuleViolation("请先接单再开始执行".to_string()));
        }

        {
            let member_repo = self.order.member_repo.as_ref();
            let member = member_repo.find_by_order_and_user(order_id, actor_id).await?;
            if let Some(m) = &member {
                if m.check_in_time.is_none() {
                    return Err(DomainError::BusinessRuleViolation("请先完成签到再开始执行".to_string()));
                }
            } else {
                return Err(DomainError::BusinessRuleViolation("请先完成签到再开始执行".to_string()));
            }
        }

        let actual_start = dto.actual_start_time.unwrap_or_else(Utc::now);
        let started = self
            .start_order_runtime(
                &order,
                order_id,
                actor_id,
                actual_start,
                dto.notes.as_deref(),
                "manual_start",
            )
            .await?;
        if !started {
            let latest = self
                .order
                .order_repo
                .find_by_id(order_id, true, None)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "DispatchOrder",
                    id: order_id.to_string(),
                })?;
            if latest.status == DispatchOrderStatus::InProgress {
                return Ok(Self::already_started_response(latest.actual_start_time, actual_start));
            }
            return Err(DomainError::InvalidStateTransition {
                from: format!("{:?}", latest.status),
                to: "in_progress".to_string(),
            });
        }

        self.evaluate_overrun_warning(order_id).await;

        Ok(serde_json::json!({
            "message": "派工单已开始执行",
            "actual_start_time": actual_start.to_rfc3339(),
        }))
    }

    /// 完成派工
    pub async fn complete_order(
        &self,
        order_id: &str,
        dto: DispatchOrderCompleteReq,
        actor_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        Self::ensure_order_execution_published(&order, "完工")?;

        if order.status == DispatchOrderStatus::Completed {
            self.ensure_actor_can_complete_order(&order, order_id, actor_id, "无权操作此派工单")
                .await?;
            return Ok(Self::already_completed_response(order.actual_end_time, Utc::now()));
        }

        if order.status != DispatchOrderStatus::InProgress {
            return Err(DomainError::BusinessRuleViolation(format!(
                "派工单状态不正确，当前状态: {:?}",
                order.status
            )));
        }
        self.ensure_actor_can_complete_order(&order, order_id, actor_id, "无权操作此派工单")
            .await?;

        let mut gate = json!({
            "enforced": false,
            "ready": true,
            "blocking_issues": [],
            "soft_missing_count": 0,
            "can_soft_complete": true,
            "required_total": 0,
            "completed_required": 0,
            "template_version": Value::Null,
            "pending_routine_items": [],
            "failed_routine_items": [],
            "routine_total": 0,
            "completed_routine": 0,
        });
        {
            let checklist_repo = self.resources.checklist_repo.as_ref();
            let template = checklist_repo.get_template(&order.task_type).await?;
            let records = checklist_repo.list_records(order_id).await?;
            gate = Self::build_checklist_status(order_id, &order.task_type, template.as_ref(), &records)?;
            let enforced = gate.get("enforced").and_then(|v| v.as_bool()).unwrap_or(false);
            let ready = gate.get("ready").and_then(|v| v.as_bool()).unwrap_or(true);
            let can_soft_complete = gate.get("can_soft_complete").and_then(|v| v.as_bool()).unwrap_or(ready);
            if enforced && !can_soft_complete {
                self.increment_metric("dispatch.order.complete.blocked");
                return Err(Self::checklist_completion_blocked_error(&gate));
            }
        }

        let client_action_id = Self::normalize_optional_ref(dto.client_action_id.as_deref()).map(str::to_string);
        let actual_end = dto.actual_end_time.unwrap_or_else(Utc::now);
        if let Some(actual_start) = order.actual_start_time {
            if actual_end < actual_start {
                return Err(DomainError::BusinessRuleViolation(
                    "actual_end_time 不能早于 actual_start_time".to_string(),
                ));
            }
        }
        let completed = self
            .order
            .order_repo
            .complete_order(order_id, actual_end, actor_id, dto.completion_notes.as_deref())
            .await?;
        if !completed {
            let latest = self
                .order
                .order_repo
                .find_by_id(order_id, true, None)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "DispatchOrder",
                    id: order_id.to_string(),
                })?;
            if latest.status == DispatchOrderStatus::Completed {
                return Ok(Self::already_completed_response(latest.actual_end_time, actual_end));
            }
            return Err(DomainError::BusinessRuleViolation(format!(
                "派工单状态不正确，当前状态: {:?}",
                latest.status
            )));
        }

        self.record_collaboration_event(
            &order,
            order_id,
            "order_completed",
            actor_id,
            None,
            serde_json::json!({
                "actual_end_time": actual_end.to_rfc3339(),
                "completion_notes": dto.completion_notes,
            }),
            actual_end,
        )
        .await;
        self.send_order_notifications(
            &order,
            actor_id,
            "派工单已完成",
            format!("派工单 {} 已完成。", order_id),
            "info",
            false,
            "dispatch_complete",
        )
        .await;
        self.sync_dispatch_chat_for_order(order_id).await;

        let completion_mode = if gate.get("soft_missing_count").and_then(|v| v.as_i64()).unwrap_or(0) > 0 {
            "soft_complete"
        } else {
            "hard_complete"
        };
        let followup_todo = if completion_mode == "soft_complete" {
            self.increment_metric("dispatch.order.complete.soft");
            self.ensure_followup_todo(
                &order,
                actor_id,
                "dispatch_soft_followup",
                format!("补录安全检查 - {}", order.id),
                format!(
                    "派工单 {} 已软闭环完工，请班组长补核常规安全项。 待补项: {}; 失败项: {}。",
                    order.id,
                    gate.get("pending_routine_items")
                        .and_then(|v| v.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .take(8)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "无".to_string()),
                    gate.get("failed_routine_items")
                        .and_then(|v| v.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .take(8)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "无".to_string()),
                ),
                "高",
                Some(actual_end + Duration::hours(4)),
                vec![
                    "dispatch".to_string(),
                    "team_lead_followup".to_string(),
                    "soft_completion".to_string(),
                ],
            )
            .await?
        } else {
            None
        };

        if completion_mode == "soft_complete" {
            let empty_array = serde_json::Value::Array(vec![]);
            let zero = serde_json::Value::from(0);
            self.order.order_repo.append_log(
                order_id,
                "soft_completion_followup_created",
                Some(actor_id),
                Some(json!({
                    "owner_role": "team_lead",
                    "soft_missing_count": gate.get("soft_missing_count").unwrap_or(&zero),
                    "pending_routine_items": gate.get("pending_routine_items").unwrap_or(&empty_array),
                    "failed_routine_items": gate.get("failed_routine_items").unwrap_or(&empty_array),
                    "todo_id": followup_todo.as_ref().and_then(|todo| todo.get("todo_id")).unwrap_or(&NULL_VALUE),
                    "assigned_to": followup_todo.as_ref().and_then(|todo| todo.get("assigned_to")).unwrap_or(&NULL_VALUE),
                    "client_action_id": client_action_id,
                })),
            ).await?;
        }

        self.evaluate_overrun_warning(order_id).await;

        Ok(json!({
            "message": "派工单已完成",
            "actual_end_time": actual_end.to_rfc3339(),
            "completion_mode": completion_mode,
            "followup_required": completion_mode == "soft_complete",
            "followup_owner_role": if completion_mode == "soft_complete" { Some("team_lead") } else { None::<&str> },
            "followup_todo_id": followup_todo.as_ref().and_then(|todo| todo.get("todo_id")).unwrap_or(&NULL_VALUE),
        }))
    }

    /// 取消派工
    pub async fn cancel_order(
        &self,
        order_id: &str,
        dto: DispatchOrderCancelRequest,
        actor_id: &str,
        is_privileged: bool,
    ) -> Result<bool, DomainError> {
        Self::ensure_actor(actor_id)?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        if order.status == DispatchOrderStatus::Cancelled {
            return Ok(false);
        }
        if order.status == DispatchOrderStatus::Completed {
            return Err(DomainError::BusinessRuleViolation("该派工单无法取消".to_string()));
        }
        if !is_privileged {
            self.ensure_actor_can_start_order(&order, order_id, actor_id, "仅管理员、调度管理员或该派工执行人可取消")
                .await?;
        }
        let cancel_reason = dto.reason.clone();
        let client_action_id = Self::normalize_optional_ref(dto.client_action_id.as_deref()).map(str::to_string);
        let cancelled = self
            .order
            .order_repo
            .update_status(order_id, "cancelled", Some(actor_id), !is_privileged)
            .await?;
        if !cancelled {
            let latest = self
                .order
                .order_repo
                .find_by_id(order_id, true, None)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "DispatchOrder",
                    id: order_id.to_string(),
                })?;
            if latest.status == DispatchOrderStatus::Cancelled {
                return Ok(false);
            }
            return Err(DomainError::BusinessRuleViolation("该派工单无法取消".to_string()));
        }

        self.order
            .order_repo
            .append_log(
                order_id,
                "cancelled",
                Some(actor_id),
                Some(serde_json::json!({
                    "reason": cancel_reason.clone(),
                    "client_action_id": client_action_id.clone(),
                    "cancelled_at": Utc::now().to_rfc3339(),
                })),
            )
            .await?;

        let now = Utc::now();
        self.record_collaboration_event(
            &order,
            order_id,
            "order_cancelled",
            actor_id,
            client_action_id,
            serde_json::json!({
                "reason": cancel_reason,
                "is_privileged": is_privileged,
            }),
            now,
        )
        .await;
        self.send_order_notifications(
            &order,
            actor_id,
            "派工单已取消",
            format!("派工单 {} 已取消。", order_id),
            "warning",
            false,
            "dispatch_cancel",
        )
        .await;
        self.sync_dispatch_chat_for_order(order_id).await;
        self.evaluate_overrun_warning(order_id).await;

        Ok(true)
    }

    /// 接受派工 (B2 gap fix)
    pub async fn accept_order(
        &self,
        order_id: &str,
        dto: DispatchOrderAcceptRequest,
        actor_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        Self::ensure_order_execution_published(&order, "接单")?;

        if !matches!(
            order.status,
            DispatchOrderStatus::Assigned | DispatchOrderStatus::InProgress
        ) {
            return Err(DomainError::BusinessRuleViolation(format!(
                "当前状态不允许接单: {:?}",
                order.status
            )));
        }

        self.ensure_actor_can_start_order(&order, order_id, actor_id, "无权接收此派工单")
            .await?;

        let client_action_id = dto.client_action_id.as_deref().map(str::trim).filter(|s| !s.is_empty());
        if let Some(client_action_id) = client_action_id {
            if self
                .order
                .order_repo
                .has_logged_action(order_id, "accepted", None, Some(client_action_id))
                .await?
            {
                return Ok(serde_json::json!({
                    "success": true,
                    "status": "duplicate",
                    "message": "重复接单请求已忽略",
                }));
            }
        }

        if self
            .order
            .order_repo
            .has_logged_action(order_id, "accepted", Some(actor_id), None)
            .await?
        {
            return Ok(serde_json::json!({
                "success": true,
                "status": "accepted",
                "message": "已接单",
            }));
        }

        let accept_note = dto.note.clone();
        let inserted = match client_action_id {
            Some(_) => {
                self.order
                    .order_repo
                    .append_log_once(
                        order_id,
                        "accepted",
                        Some(actor_id),
                        serde_json::json!({
                            "note": accept_note.clone(),
                            "client_action_id": client_action_id,
                            "accepted_at": Utc::now().to_rfc3339(),
                        }),
                    )
                    .await?
            }
            None => {
                self.order
                    .order_repo
                    .append_log(
                        order_id,
                        "accepted",
                        Some(actor_id),
                        Some(serde_json::json!({
                            "note": accept_note.clone(),
                            "accepted_at": Utc::now().to_rfc3339(),
                        })),
                    )
                    .await?;
                true
            }
        };
        if !inserted {
            return Ok(serde_json::json!({
                "success": true,
                "status": "duplicate",
                "message": "重复接单请求已忽略",
            }));
        }

        let now = Utc::now();
        self.record_collaboration_event(
            &order,
            order_id,
            "order_accepted",
            actor_id,
            client_action_id.map(str::to_string),
            serde_json::json!({
                "note": accept_note,
                "accepted_at": now.to_rfc3339(),
            }),
            now,
        )
        .await;
        self.send_order_notifications(
            &order,
            actor_id,
            "派工单已接单",
            format!("派工单 {} 已被接收。", order_id),
            "info",
            false,
            "dispatch_accept",
        )
        .await;
        Ok(serde_json::json!({
            "success": true,
            "status": "accepted",
            "message": "接单成功",
        }))
    }

    /// 签到 (B6 gap fix: QR validation, distance fence, member upsert)
    pub async fn checkin_order(
        &self,
        order_id: &str,
        dto: DispatchOrderCheckInRequest,
        actor_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        Self::ensure_order_execution_published(&order, "签到")?;

        self.ensure_actor_can_start_order(&order, order_id, actor_id, "无权操作此派工单")
            .await?;

        if order.status == DispatchOrderStatus::Completed || order.status == DispatchOrderStatus::Cancelled {
            return Err(DomainError::BusinessRuleViolation("当前状态不可签到".into()));
        }

        let client_action_id = dto
            .client_action_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        if let Some(cid) = client_action_id {
            if self
                .order
                .order_repo
                .has_logged_action(order_id, "checkin", None, Some(cid))
                .await?
            {
                return Ok(serde_json::json!({
                    "message": "重复签到请求已忽略",
                    "status": "duplicate",
                }));
            }
        }

        if dto.lat.is_some() ^ dto.lng.is_some() {
            return Err(DomainError::BusinessRuleViolation(
                "位置参数需要同时提供 lat 与 lng".to_string(),
            ));
        }

        let mut distance_to_stand_m = None;
        let mut verification_status = "pending_verification".to_string();
        let mut verification_source = "manual".to_string();
        if let (Some(lat), Some(lng), Some(stand_id)) = (dto.lat, dto.lng, order.stand_id.as_deref()) {
            {
                let stand_repo = self.resources.stand_repo.as_ref();
                if let Some(stand) = stand_repo.find_by_id(stand_id).await? {
                    if stand.position_lat != 0.0 || stand.position_lng != 0.0 {
                        let distance_m = helpers::haversine_distance(lat, lng, stand.position_lat, stand.position_lng);
                        distance_to_stand_m = Some(distance_m);
                        if distance_m <= 300.0 {
                            verification_status = "verified".to_string();
                            verification_source = "geo".to_string();
                        }
                    }
                }
            }
        }
        if verification_status != "verified" {
            if let Some(ref qr) = dto.qr_code {
                let qr = qr.trim();
                let expected = format!("dispatch:{order_id}");
                let legacy_expected = format!("dispatch_order:{order_id}");
                if qr == expected || qr == legacy_expected || qr == order_id {
                    verification_status = "verified".to_string();
                    verification_source = "qr".to_string();
                }
            }
        }

        let now = Utc::now();
        let mut followup_todo = None;
        let mut auto_started = false;

        {
            let member_repo = self.order.member_repo.as_ref();
            let existing = member_repo.find_by_order_and_user(order_id, actor_id).await?;
            match existing {
                Some(mut m) => {
                    if m.check_in_time.is_some() {
                        return Ok(serde_json::json!({
                            "message": "您已到场",
                            "status": "already_checked_in",
                            "check_in_time": m.check_in_time.map(|value| value.to_rfc3339()),
                            "distance_to_stand_m": distance_to_stand_m.map(|value| (value * 100.0).round() / 100.0),
                            "verification_status": verification_status,
                            "verification_source": verification_source,
                            "auto_started": order.status == DispatchOrderStatus::InProgress,
                            "order_status": Self::dispatch_order_status_value(order.status),
                        }));
                    }
                    m.check_in_time = Some(now);
                    member_repo.save(&m).await?;
                }
                None => {
                    if !Self::should_auto_create_checkin_member(
                        order.assignee_type,
                        order.individual_user_id.as_deref(),
                        actor_id,
                    ) {
                        return Err(DomainError::NotFound {
                            entity_type: "DispatchOrderMember",
                            id: format!("{order_id}:{actor_id}"),
                        });
                    }
                    let member = DispatchOrderMember {
                        id: ulid::Ulid::new().to_string(),
                        dispatch_order_id: order_id.to_string(),
                        user_id: actor_id.to_string(),
                        role: MemberRole::Member,
                        source_type: AssigneeType::Individual,
                        source_team_id: None,
                        slot_code: None,
                        qualification_code: None,
                        qualification_level_code: None,
                        assigned_at: Some(now),
                        check_in_time: Some(now),
                        check_out_time: None,
                        is_active: true,
                        username: None,
                    };
                    member_repo.save(&member).await?;
                }
            }
        }

        let checkin_note = dto.note.clone();
        let checkin_qr_code = dto.qr_code.clone();
        let inserted = match client_action_id {
            Some(_) => {
                self.order
                    .order_repo
                    .append_log_once(
                        order_id,
                        "checkin",
                        Some(actor_id),
                        serde_json::json!({
                            "client_action_id": client_action_id,
                            "qr_code": checkin_qr_code.clone(),
                            "lat": dto.lat,
                            "lng": dto.lng,
                            "accuracy_m": dto.accuracy_m,
                            "distance_to_stand_m": distance_to_stand_m,
                            "note": checkin_note.clone(),
                            "verification_status": verification_status.clone(),
                            "verification_source": verification_source.clone(),
                            "check_in_time": now.to_rfc3339(),
                            "event_id": Self::new_dispatch_id(),
                        }),
                    )
                    .await?
            }
            None => {
                self.order
                    .order_repo
                    .append_log(
                        order_id,
                        "checkin",
                        Some(actor_id),
                        Some(serde_json::json!({
                            "qr_code": checkin_qr_code.clone(),
                            "lat": dto.lat,
                            "lng": dto.lng,
                            "accuracy_m": dto.accuracy_m,
                            "distance_to_stand_m": distance_to_stand_m,
                            "note": checkin_note.clone(),
                            "verification_status": verification_status.clone(),
                            "verification_source": verification_source.clone(),
                            "check_in_time": now.to_rfc3339(),
                            "event_id": Self::new_dispatch_id(),
                        })),
                    )
                    .await?;
                true
            }
        };
        if !inserted {
            return Ok(serde_json::json!({
                "message": "重复签到请求已忽略",
                "status": "duplicate",
            }));
        }

        self.record_collaboration_event(
            &order,
            order_id,
            "order_checked_in",
            actor_id,
            client_action_id.map(str::to_string),
            serde_json::json!({
                "client_action_id": client_action_id,
                "qr_code": checkin_qr_code,
                "lat": dto.lat,
                "lng": dto.lng,
                "accuracy_m": dto.accuracy_m,
                "distance_to_stand_m": distance_to_stand_m,
                "note": checkin_note,
                "check_in_time": now.to_rfc3339(),
                "verification_status": verification_status.clone(),
                "verification_source": verification_source.clone(),
            }),
            now,
        )
        .await;

        if order.status == DispatchOrderStatus::Assigned {
            auto_started = self
                .start_order_runtime(&order, order_id, actor_id, now, None, "auto_start_after_checkin")
                .await?;
        }

        if verification_status == "pending_verification" {
            self.increment_metric("dispatch.order.arrival.pending_verification");
            let due_at = now + Duration::hours(2);
            followup_todo = self
                .ensure_followup_todo(
                    &order,
                    actor_id,
                    "dispatch_arrival_verification",
                    format!("补核到场记录 - {}", order.id),
                    format!(
                        "派工单 {} 到场记录未通过自动核验。 请班组长在 2 小时内补核到场来源与现场真实性。 来源: {}; 距离: {} 米。",
                        order.id,
                        verification_source,
                        distance_to_stand_m
                            .map(|value| format!("{value:.2}"))
                            .unwrap_or_else(|| "未记录".to_string()),
                    ),
                    "高",
                    Some(due_at),
                    vec![
                        "dispatch".to_string(),
                        "team_lead_followup".to_string(),
                        "arrival_verification".to_string(),
                    ],
                )
                .await?;
            self.order.order_repo
                .append_log(
                    order_id,
                    "arrival_verification_followup_created",
                    Some(actor_id),
                    Some(json!({
                        "verification_status": verification_status,
                        "verification_source": verification_source,
                        "distance_to_stand_m": distance_to_stand_m.map(|value| (value * 100.0).round() / 100.0),
                        "todo_id": followup_todo.as_ref().and_then(|todo| todo.get("todo_id")).unwrap_or(&NULL_VALUE),
                        "assigned_to": followup_todo.as_ref().and_then(|todo| todo.get("assigned_to")).unwrap_or(&NULL_VALUE),
                        "due_at": due_at.to_rfc3339(),
                    })),
                )
                .await?;
        }

        Ok(serde_json::json!({
            "message": "到场成功",
            "check_in_time": now.to_rfc3339(),
            "distance_to_stand_m": distance_to_stand_m.map(|value| (value * 100.0).round() / 100.0),
            "verification_status": verification_status,
            "verification_source": verification_source,
            "auto_started": auto_started,
            "order_status": if auto_started {
                "in_progress"
            } else {
                Self::dispatch_order_status_value(order.status)
            },
            "followup_todo_id": followup_todo.as_ref().and_then(|todo| todo.get("todo_id")).unwrap_or(&NULL_VALUE),
        }))
    }

    /// 签退 (B7 gap fix: member checkout, travel time recording, auto-complete)
    pub async fn checkout_order(
        &self,
        order_id: &str,
        dto: DispatchOrderCheckOutRequest,
        actor_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        Self::validate_coordinate_range(dto.lat, "lat")?;
        Self::validate_coordinate_range(dto.lng, "lng")?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        Self::ensure_order_execution_published(&order, "签退")?;
        self.ensure_actor_can_complete_order(&order, order_id, actor_id, "无权签退此派工单")
            .await?;

        if order.status != DispatchOrderStatus::InProgress && order.status != DispatchOrderStatus::Assigned {
            return Err(DomainError::BusinessRuleViolation(
                "仅进行中或已派发的工单可以签退".into(),
            ));
        }

        let client_action_id = dto
            .client_action_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        if let Some(cid) = client_action_id {
            if self
                .order
                .order_repo
                .has_logged_action(order_id, "checkout", None, Some(cid))
                .await?
            {
                return Ok(serde_json::json!({
                    "message": "重复签退请求已忽略",
                    "status": "duplicate",
                }));
            }
        }

        let recorded_at = dto.recorded_at;
        let recorded_at_display = recorded_at.as_ref().map(|value| value.to_rfc3339());
        let now = recorded_at.unwrap_or_else(Utc::now);
        let mut auto_completed = false;
        let mut travel_info = None;

        {
            let member_repo = self.order.member_repo.as_ref();
            let existing = member_repo.find_by_order_and_user(order_id, actor_id).await?;
            match existing {
                Some(mut m) => {
                    if m.check_out_time.is_some() {
                        return Ok(serde_json::json!({
                            "message": "您已签退",
                            "status": "already_checked_out",
                            "check_out_time": m.check_out_time,
                        }));
                    }
                    if m.check_in_time.is_none() {
                        return Err(DomainError::ValidationError("未找到签到记录，无法签退".to_string()));
                    }
                    m.check_out_time = Some(now);
                    member_repo.save(&m).await?;
                    travel_info = self.record_travel_time_on_checkout(actor_id, &order, &m).await;
                }
                None => {
                    return Err(DomainError::NotFound {
                        entity_type: "DispatchOrderMember",
                        id: format!("{order_id}:{actor_id}"),
                    });
                }
            }

            let all_members = member_repo.find_by_order(order_id).await?;
            auto_completed = self
                .try_auto_complete_on_all_checkout(order_id, actor_id, &all_members)
                .await;
        }

        let checkout_note = dto.note.clone();
        let inserted = match client_action_id {
            Some(_) => {
                self.order
                    .order_repo
                    .append_log_once(
                        order_id,
                        "checkout",
                        Some(actor_id),
                        serde_json::json!({
                            "client_action_id": client_action_id,
                            "check_out_time": now.to_rfc3339(),
                            "lat": dto.lat,
                            "lng": dto.lng,
                            "note": checkout_note.clone(),
                            "recorded_at": recorded_at_display.clone(),
                            "travel_info": travel_info.clone(),
                            "auto_completed": auto_completed,
                            "event_id": Self::new_dispatch_id(),
                        }),
                    )
                    .await?
            }
            None => {
                self.order
                    .order_repo
                    .append_log(
                        order_id,
                        "checkout",
                        Some(actor_id),
                        Some(serde_json::json!({
                            "check_out_time": now.to_rfc3339(),
                            "lat": dto.lat,
                            "lng": dto.lng,
                            "note": checkout_note.clone(),
                            "recorded_at": recorded_at_display.clone(),
                            "travel_info": travel_info.clone(),
                            "auto_completed": auto_completed,
                            "event_id": Self::new_dispatch_id(),
                        })),
                    )
                    .await?;
                true
            }
        };
        if !inserted {
            return Ok(serde_json::json!({
                "message": "重复签退请求已忽略",
                "status": "duplicate",
            }));
        }

        self.record_collaboration_event(
            &order,
            order_id,
            "order_checked_out",
            actor_id,
            client_action_id.map(str::to_string),
            serde_json::json!({
                "client_action_id": client_action_id,
                "check_out_time": now.to_rfc3339(),
                "lat": dto.lat,
                "lng": dto.lng,
                "note": checkout_note,
                "recorded_at": recorded_at_display,
                "travel_info": travel_info.clone(),
                "auto_completed": auto_completed,
            }),
            now,
        )
        .await;

        let mut result = serde_json::json!({
            "message": "签退成功",
            "check_out_time": now.to_rfc3339(),
            "auto_completed": auto_completed,
        });
        if let Some(travel_info) = travel_info {
            Self::merge_json_object(&mut result, Some(travel_info));
        }
        Ok(result)
    }
}
