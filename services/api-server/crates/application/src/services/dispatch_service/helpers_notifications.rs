use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use tracing::warn;

use crate::schemas::dispatch_schemas::*;
use crate::services::notification_service::DispatchBatchNotificationCreate;
use fms_domain::error::DomainError;
use fms_domain::models::anomaly::{AnomalySeverity, AnomalyType};
use fms_domain::models::dispatch::*;
use fms_domain::models::dispatch_collaboration::DispatchCollaborationEvent;

use super::{DispatchService, NULL_VALUE};

impl DispatchService {
    pub(super) async fn record_collaboration_event(
        &self,
        order: &DispatchOrder,
        order_id: &str,
        event_type: &str,
        actor_id: &str,
        correlation_id: Option<String>,
        payload: Value,
        occurred_at: DateTime<Utc>,
    ) {
        let collaboration_repo = self.notifications.collaboration_repo.as_ref();

        let event = DispatchCollaborationEvent {
            event_id: Self::new_dispatch_id(),
            flight_id: order.flight_id.clone(),
            dispatch_order_id: Some(order_id.to_string()),
            group_id: None,
            event_type: event_type.to_string(),
            actor_user_id: Some(actor_id.to_string()),
            actor_username: None,
            correlation_id,
            payload,
            occurred_at,
            source_table: Some("dispatch_order_logs".to_string()),
            source_record_id: None,
        };

        if let Err(error) = collaboration_repo.create_event(&event).await {
            warn!(
                order_id,
                flight_id = %order.flight_id,
                event_type,
                error = %error,
                "failed to record dispatch collaboration event"
            );
        }
    }

    pub(super) async fn collect_notification_recipient_ids(
        &self,
        order: &DispatchOrder,
        actor_id: &str,
    ) -> Vec<String> {
        let mut recipient_ids = HashSet::new();

        if let Some(dispatched_by) = order.dispatched_by.as_deref() {
            let normalized = dispatched_by.trim();
            if !normalized.is_empty() && normalized != actor_id {
                recipient_ids.insert(normalized.to_string());
            }
        }

        if let Some(individual_user_id) = order.individual_user_id.as_deref() {
            let normalized = individual_user_id.trim();
            if !normalized.is_empty() && normalized != actor_id {
                recipient_ids.insert(normalized.to_string());
            }
        }

        if let Some(team_id) = order.team_id.as_deref() {
            let team_repo = self.resources.team_repo.as_ref();
            if let Ok(Some(team)) = team_repo.find_by_id(team_id, false).await {
                if let Some(leader_id) = team.leader_id.as_deref() {
                    let normalized = leader_id.trim();
                    if !normalized.is_empty() && normalized != actor_id {
                        recipient_ids.insert(normalized.to_string());
                    }
                }
            }
        }

        let mut recipients = recipient_ids.into_iter().collect::<Vec<_>>();
        recipients.sort();
        recipients
    }

    pub(super) fn collect_publication_recipient_ids(order: &DispatchOrder) -> Vec<String> {
        let mut recipient_ids = HashSet::new();
        if let Some(individual_user_id) = order.individual_user_id.as_deref() {
            let normalized = individual_user_id.trim();
            if !normalized.is_empty() {
                recipient_ids.insert(normalized.to_string());
            }
        }
        if let Some(members) = order.task_crew.get("members").and_then(Value::as_array) {
            for member in members {
                let Some(user_id) = member
                    .get("user_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                recipient_ids.insert(user_id.to_string());
            }
        }
        let mut recipients = recipient_ids.into_iter().collect::<Vec<_>>();
        recipients.sort();
        recipients
    }

    pub(super) async fn send_order_notifications(
        &self,
        order: &DispatchOrder,
        actor_id: &str,
        title: &str,
        body: String,
        severity: &str,
        receipt_required: bool,
        origin_type: &str,
    ) {
        let notification_service = self.notifications.notification_service.as_ref();

        let recipient_ids = self.collect_notification_recipient_ids(order, actor_id).await;
        if recipient_ids.is_empty() {
            return;
        }

        if let Err(error) = notification_service
            .send_dispatch_batch(DispatchBatchNotificationCreate {
                user_ids: recipient_ids,
                title: title.to_string(),
                body,
                category: "dispatch".to_string(),
                severity: severity.to_string(),
                flight_id: Some(order.flight_id.clone()),
                related_entity_type: Some("dispatch_order".to_string()),
                related_entity_id: Some(order.id.clone()),
                dispatch_order_id: Some(order.id.clone()),
                group_id: None,
                sender_user_id: None,
                sender_username_snapshot: None,
                origin_type: origin_type.to_string(),
                receipt_required,
            })
            .await
        {
            warn!(
                order_id = %order.id,
                flight_id = %order.flight_id,
                error = %error,
                "failed to send dispatch notification fanout"
            );
        }
    }

    pub async fn send_publication_notifications(&self, order: &DispatchOrder) {
        let notification_service = self.notifications.notification_service.as_ref();

        let recipient_ids = Self::collect_publication_recipient_ids(order);
        if recipient_ids.is_empty() {
            return;
        }

        let body = format!(
            "Dispatch order {} for flight {} (step {}) is now published for execution.",
            order.id, order.flight_id, order.task_type
        );
        if let Err(error) = notification_service
            .send_dispatch_batch(DispatchBatchNotificationCreate {
                user_ids: recipient_ids,
                title: "Dispatch order published".to_string(),
                body,
                category: "dispatch".to_string(),
                severity: "info".to_string(),
                flight_id: Some(order.flight_id.clone()),
                related_entity_type: Some("dispatch_order".to_string()),
                related_entity_id: Some(order.id.clone()),
                dispatch_order_id: Some(order.id.clone()),
                group_id: None,
                sender_user_id: None,
                sender_username_snapshot: None,
                origin_type: "prepublish".to_string(),
                receipt_required: false,
            })
            .await
        {
            warn!(
                order_id = %order.id,
                flight_id = %order.flight_id,
                error = %error,
                "failed to send dispatch publication notification fanout"
            );
        }
    }

    pub async fn sync_dispatch_chat_for_order(&self, order_id: &str) {
        let dispatch_chat_service = self.notifications.dispatch_chat_service.as_ref();
        dispatch_chat_service.sync_dispatch_order_chat(order_id).await;
    }

    pub(super) fn dispatch_order_status_value(status: DispatchOrderStatus) -> &'static str {
        match status {
            DispatchOrderStatus::Pending => "pending",
            DispatchOrderStatus::Assigned => "assigned",
            DispatchOrderStatus::InProgress => "in_progress",
            DispatchOrderStatus::Completed => "completed",
            DispatchOrderStatus::Cancelled => "cancelled",
        }
    }

    pub(super) fn parse_action_timestamp(primary: Option<&str>, fallback: Option<&str>) -> Option<DateTime<Utc>> {
        primary
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .or_else(|| {
                fallback
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.with_timezone(&Utc))
            })
    }

    pub(super) fn merge_json_object(target: &mut Value, source: Option<Value>) {
        let Some(Value::Object(source_map)) = source else {
            return;
        };
        let Some(target_map) = target.as_object_mut() else {
            return;
        };
        for (key, value) in source_map {
            target_map.insert(key, value);
        }
    }

    pub(super) fn validate_coordinate_range(value: Option<f64>, field: &str) -> Result<(), DomainError> {
        let Some(value) = value else {
            return Ok(());
        };
        let valid = match field {
            "lat" => (-90.0..=90.0).contains(&value),
            "lng" => (-180.0..=180.0).contains(&value),
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err(DomainError::ValidationError(format!("{field} 超出允许范围")))
        }
    }

    pub(super) fn validate_issue_title(title: Option<&str>) -> Result<String, DomainError> {
        let Some(title) = title else {
            return Ok(String::new());
        };
        let normalized = title.trim();
        if normalized.chars().count() > 200 {
            return Err(DomainError::ValidationError(
                "title 长度不能超过 200 个字符".to_string(),
            ));
        }
        Ok(normalized.to_string())
    }

    pub(super) fn validate_issue_request(dto: &ReportIssueRequest) -> Result<(), DomainError> {
        dto.validate().map_err(DomainError::ValidationError)?;
        if !matches!(
            dto.input_mode.trim().to_ascii_lowercase().as_str(),
            "text" | "photo" | "voice"
        ) {
            return Err(DomainError::ValidationError(
                "input_mode 必须是 text/photo/voice 之一".to_string(),
            ));
        }
        let has_text = dto.title.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_some()
            || dto
                .description
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_some()
            || dto.note.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_some();
        let has_attachment = dto.attachments.as_ref().map(|a| !a.is_empty()).unwrap_or(false)
            || dto
                .voice_attachment_id
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_some();
        if !has_text && !has_attachment {
            return Err(DomainError::ValidationError(
                "至少提供文本、附件或语音首报之一".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn normalize_issue_type(issue_type: Option<&str>) -> String {
        let normalized = issue_type
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_else(|| "dispatch_issue".to_string());
        if normalized.chars().count() > 64 {
            "dispatch_issue".to_string()
        } else {
            normalized
        }
    }

    pub(super) fn parse_issue_severity(severity: Option<&str>) -> Result<AnomalySeverity, DomainError> {
        match severity
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
            .as_deref()
        {
            None | Some("medium") => Ok(AnomalySeverity::Medium),
            Some("low") => Ok(AnomalySeverity::Low),
            Some("high") => Ok(AnomalySeverity::High),
            Some("critical") => Ok(AnomalySeverity::Critical),
            Some(other) => Err(DomainError::BusinessRuleViolation(format!("无效异常级别: {other}"))),
        }
    }

    pub(super) fn parse_issue_type(issue_type: &str) -> AnomalyType {
        match issue_type {
            "gate_stand_conflict" => AnomalyType::GateStandConflict,
            "kpi_degradation" => AnomalyType::KpiDegradation,
            "ai_risk" => AnomalyType::AiRisk,
            "dispatch_issue" => AnomalyType::DispatchIssue,
            _ => AnomalyType::DispatchIssue,
        }
    }

    pub(super) async fn record_travel_time_on_checkout(
        &self,
        actor_id: &str,
        order: &DispatchOrder,
        member: &DispatchOrderMember,
    ) -> Option<serde_json::Value> {
        let checkin_time = member.check_in_time?;
        let current_stand = order.stand_id.as_deref()?;
        let member_repo = self.order.member_repo.as_ref();
        let travel_repo = self.resources.travel_stats_repo.as_ref();
        let prev = member_repo
            .find_latest_checkout_for_user(actor_id, checkin_time)
            .await
            .ok()??;
        let prev_stand = prev.get("stand_id").and_then(|value| value.as_str())?;
        if prev_stand == current_stand {
            return None;
        }
        let prev_time_value = prev.get("check_out_time")?.clone();
        let prev_time = match prev_time_value {
            Value::String(value) => chrono::DateTime::parse_from_rfc3339(&value)
                .ok()
                .map(|value| value.with_timezone(&Utc))?,
            other => serde_json::from_value::<DateTime<Utc>>(other).ok()?,
        };
        let travel_minutes = (checkin_time - prev_time).num_seconds() as f64 / 60.0;
        if !(0.0 < travel_minutes && travel_minutes <= 240.0) {
            return None;
        }
        let _ = travel_repo
            .record_travel(prev_stand, current_stand, travel_minutes)
            .await;
        Some(serde_json::json!({
            "from_order_id": prev.get("dispatch_order_id").unwrap_or(&NULL_VALUE),
            "from_stand_code": prev.get("stand_code").unwrap_or(&NULL_VALUE),
            "travel_minutes": (travel_minutes * 100.0).round() / 100.0,
        }))
    }

    pub(super) async fn start_order_runtime(
        &self,
        order: &DispatchOrder,
        order_id: &str,
        actor_id: &str,
        actual_start: DateTime<Utc>,
        notes: Option<&str>,
        reason: &str,
    ) -> Result<bool, DomainError> {
        let started = self
            .order
            .order_repo
            .start_order(order_id, actual_start, actor_id)
            .await?;
        if !started {
            return Ok(false);
        }

        self.record_collaboration_event(
            order,
            order_id,
            "order_started",
            actor_id,
            None,
            serde_json::json!({
                "actual_start_time": actual_start.to_rfc3339(),
                "notes": notes,
                "reason": reason,
            }),
            actual_start,
        )
        .await;
        self.send_order_notifications(
            order,
            actor_id,
            "派工单已开始执行",
            format!("派工单 {} 已开始执行。", order_id),
            "info",
            false,
            "dispatch_start",
        )
        .await;
        self.sync_dispatch_chat_for_order(order_id).await;

        Ok(true)
    }

    pub(super) fn extract_order_baseline_members(order: &DispatchOrder) -> HashMap<String, Vec<String>> {
        let mut baseline_by_slot = HashMap::<String, Vec<String>>::new();
        if let Some(members) = order.task_crew.get("members").and_then(Value::as_array) {
            for member in members {
                let Some(user_id) = member
                    .get("user_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                else {
                    continue;
                };
                let slot_code = member
                    .get("slot_code")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("slot")
                    .to_string();
                baseline_by_slot.entry(slot_code).or_default().push(user_id);
            }
        } else {
            for member in &order.members {
                let user_id = member.user_id.trim();
                if user_id.is_empty() {
                    continue;
                }
                let slot_code = member
                    .slot_code
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("slot")
                    .to_string();
                baseline_by_slot.entry(slot_code).or_default().push(user_id.to_string());
            }
        }
        for values in baseline_by_slot.values_mut() {
            values.sort();
        }
        baseline_by_slot
    }

    pub(super) fn window_task_interval(
        order: &DispatchOrder,
        fallback_start: DateTime<Utc>,
    ) -> (DateTime<Utc>, DateTime<Utc>) {
        let start_time = order.planned_start_time.unwrap_or(fallback_start);
        let end_time = order
            .planned_end_time
            .unwrap_or_else(|| start_time + Duration::minutes(15));
        if end_time < start_time {
            (start_time, start_time)
        } else {
            (start_time, end_time)
        }
    }

    pub(super) fn schedule_source_text(value: ScheduleSource) -> &'static str {
        match value {
            ScheduleSource::ShiftInstance => "shift_instance",
            ScheduleSource::CurrentStatusFallback => "current_status_fallback",
        }
    }

    pub(super) fn alert_severity_text(value: AlertSeverity) -> &'static str {
        match value {
            AlertSeverity::Info => "info",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Critical => "critical",
        }
    }

    pub(super) fn alert_to_json(alert: &DispatchAlert) -> Value {
        json!({
            "id": alert.id,
            "flight_id": alert.flight_id,
            "task_type": alert.task_type,
            "alert_type": alert.alert_type,
            "severity": Self::alert_severity_text(alert.severity),
            "message": alert.message,
            "is_resolved": alert.is_resolved,
            "resolved_at": alert.resolved_at,
            "resolved_by": alert.resolved_by,
            "resolution_notes": alert.resolution_notes,
            "notify_users": alert.notify_users,
            "created_at": alert.created_at,
            "dedupe_key": alert.dedupe_key,
            "current_order_id": alert.current_order_id,
            "next_order_id": alert.next_order_id,
            "last_detected_at": alert.last_detected_at,
            "occurrence_count": alert.occurrence_count,
            "acknowledged_at": alert.acknowledged_at,
            "acknowledged_by": alert.acknowledged_by,
            "details": if alert.details.is_null() {
                json!({})
            } else {
                alert.details.clone()
            },
        })
    }

    pub(super) async fn create_dispatch_alert(
        &self,
        flight_id: &str,
        task_type: &str,
        alert_type: &str,
        message: String,
        severity: AlertSeverity,
    ) -> Result<Option<DispatchAlert>, DomainError> {
        let alert_repo = self.notifications.alert_repo.as_ref();
        let alert = DispatchAlert {
            id: Self::new_dispatch_id(),
            flight_id: Some(flight_id.trim().to_string()).filter(|value| !value.is_empty()),
            task_type: Some(task_type.trim().to_string()).filter(|value| !value.is_empty()),
            alert_type: alert_type.trim().to_string(),
            severity,
            message,
            is_resolved: false,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
            notify_users: Vec::new(),
            created_at: Some(Utc::now()),
            dedupe_key: None,
            current_order_id: None,
            next_order_id: None,
            last_detected_at: None,
            occurrence_count: 1,
            acknowledged_at: None,
            acknowledged_by: None,
            details: serde_json::Value::Object(Default::default()),
        };
        alert_repo.save(&alert).await?;
        Ok(Some(alert))
    }

    pub(super) fn level_covers_requirement(
        level_index: &HashMap<String, HashSet<String>>,
        level_code: &str,
        min_level_code: Option<&str>,
    ) -> bool {
        let Some(min_level_code) = min_level_code.map(str::trim).filter(|value| !value.is_empty()) else {
            return true;
        };
        if level_code == min_level_code {
            return true;
        }
        level_index
            .get(level_code)
            .map(|covered| covered.contains(min_level_code))
            .unwrap_or(false)
    }

    pub(super) fn build_equipment_gap_from_snapshot(requirements: &[Value], reason: &str) -> Vec<Value> {
        requirements
            .iter()
            .filter_map(|item| item.as_object())
            .map(|item| {
                let required_count = item.get("required_count").and_then(Value::as_i64).unwrap_or(1).max(1);
                json!({
                    "slot_code": item.get("slot_code").and_then(Value::as_str).unwrap_or_default(),
                    "equipment_type_id": item.get("equipment_type_id").unwrap_or(&NULL_VALUE),
                    "equipment_type_code": item.get("equipment_type_code").unwrap_or(&NULL_VALUE),
                    "required_count": required_count,
                    "assigned_count": 0,
                    "missing_count": required_count,
                    "reason": reason,
                })
            })
            .collect()
    }

    pub(super) fn users_overlap_window(
        bookings: &HashMap<String, Vec<(DateTime<Utc>, DateTime<Utc>)>>,
        user_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> bool {
        bookings
            .get(user_id)
            .into_iter()
            .flatten()
            .any(|(booked_start, booked_end)| *booked_start < end_time && start_time < *booked_end)
    }

    pub(super) fn resolve_window_assignment_team(members: &[Value]) -> (Option<String>, Option<String>) {
        let mut counts = HashMap::<String, i64>::new();
        let mut team_names = HashMap::<String, Option<String>>::new();
        for member in members {
            let Some(team_id) = member
                .get("source_team_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
            else {
                continue;
            };
            *counts.entry(team_id.clone()).or_insert(0) += 1;
            team_names.entry(team_id.clone()).or_insert_with(|| {
                member
                    .get("source_team_name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        }
        counts
            .into_iter()
            .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
            .map(|(team_id, _)| {
                let team_name = team_names.remove(&team_id).flatten();
                (Some(team_id), team_name)
            })
            .unwrap_or((None, None))
    }
}

// ---------------------------------------------------------------------------
// Free functions (module-private helpers)
// ---------------------------------------------------------------------------

pub(super) fn optimal_order_status(order: &DispatchOrder) -> String {
    order.status.as_ref().to_string()
}

pub(super) fn optimal_order_has_assignment(order: &DispatchOrder) -> bool {
    order
        .team_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
        || order
            .individual_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        || !order_member_user_ids(order).is_empty()
}

pub(super) fn order_to_response(o: &DispatchOrder) -> DispatchOrderResponse {
    let now = Utc::now();
    let (effective_start_time, effective_end_time) = effective_interval(o, now);
    DispatchOrderResponse {
        id: o.id.clone(),
        flight_id: o.flight_id.clone(),
        task_type: o.task_type.clone(),
        task_type_name: o.task_type_name.clone(),
        stand_id: o.stand_id.clone(),
        stand_code: o.stand_code.clone(),
        terminal: o.terminal.clone(),
        assignee_type: format!("{:?}", o.assignee_type).to_lowercase(),
        team_id: o.team_id.clone(),
        team_name: o.team_name.clone(),
        department: o.department.clone(),
        individual_user_id: o.individual_user_id.clone(),
        individual_username: o.individual_username.clone(),
        driver_type: o.driver_type.map(|value| value.as_ref().to_string()),
        driver_team_id: o.driver_team_id.clone(),
        driver_user_id: o.driver_user_id.clone(),
        driver_assignment: None,
        planned_start_time: o.planned_start_time,
        planned_end_time: o.planned_end_time,
        actual_start_time: o.actual_start_time,
        actual_end_time: o.actual_end_time,
        estimated_completion_time: o.estimated_completion_time,
        estimated_completion_reported_by: o.estimated_completion_reported_by.clone(),
        estimated_completion_reported_at: o.estimated_completion_reported_at,
        estimated_completion_note: o.estimated_completion_note.clone(),
        effective_start_time: Some(effective_start_time),
        effective_end_time: Some(effective_end_time),
        effective_end_source: None,
        gate: o.gate.clone(),
        status: o.status.as_ref().to_string(),
        dispatch_type: o.dispatch_type.as_ref().to_string(),
        dispatched_at: o.dispatched_at,
        estimated_arrival_minutes: o.estimated_arrival_minutes,
        source: o.source.clone(),
        schedule_source: o.schedule_source.as_ref().to_string(),
        lock_level: o.lock_level.as_ref().to_string(),
        publication_state: o.publication_state.clone(),
        source_type: o.source_type.clone(),
        department_id: o.department_id.clone(),
        leg_scope: o.leg_scope.clone(),
        generation_rule_id: o.generation_rule_id.clone(),
        generation_rule_version: o.generation_rule_version,
        generation_anchor_type: o.generation_anchor_type.clone(),
        generation_anchor_time: o.generation_anchor_time,
        completion_time_mode: o.completion_time_mode.clone(),
        completion_anchor_type: o.completion_anchor_type.clone(),
        completion_anchor_time: o.completion_anchor_time,
        completion_offset_minutes: o.completion_offset_minutes,
        completion_warning_lead_minutes: o.completion_warning_lead_minutes,
        publish_trigger_mode: o.publish_trigger_mode.clone(),
        publish_at: o.publish_at,
        turnaround_pair_key: o.turnaround_pair_key.clone(),
        turnaround_constraint_mode: o.turnaround_constraint_mode.clone(),
        department_rule_version: o.department_rule_version.clone(),
        crew_requirement_snapshot: o.crew_requirement_snapshot.clone(),
        equipment_requirement_snapshot: o.equipment_requirement_snapshot.clone(),
        task_crew: match &o.task_crew {
            serde_json::Value::Object(map) if !map.is_empty() => serde_json::from_value(o.task_crew.clone()).ok(),
            _ => None,
        },
        equipment_assignment: o.equipment_assignment.clone(),
        qualification_gap: o.qualification_gap.clone(),
        equipment_gap: o.equipment_gap.clone(),
        availability_reason: o.availability_reason.clone(),
        score_breakdown: match &o.score_breakdown {
            serde_json::Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => HashMap::new(),
        },
        conflict_reason: o.conflict_reason.clone(),
        origin_type: if o.source.trim().eq_ignore_ascii_case("workflow") {
            "workflow".to_string()
        } else {
            "manual".to_string()
        },
        origin_label: if o.source.trim().eq_ignore_ascii_case("workflow") {
            "流程".to_string()
        } else {
            "人工".to_string()
        },
        process_instance_id: o.process_instance_id.clone(),
        process_task_id: o.process_task_id.clone(),
        workflow_status: Some(o.workflow_status.clone()),
        workflow_context: match &o.workflow_context {
            serde_json::Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => HashMap::new(),
        },
        recommended_assignees: Vec::new(),
        recommendation_score: o.recommendation_score,
        supervisor_notified: o.supervisor_notified,
        supervisor_notified_at: o.supervisor_notified_at,
        assignment_deadline: o.assignment_deadline,
        completion_notes: o.completion_notes.clone(),
        created_at: o.created_at,
        members: o
            .members
            .iter()
            .map(|item| DispatchOrderMemberResponse {
                id: item.id.clone(),
                user_id: item.user_id.clone(),
                role: item.role.as_ref().to_string(),
                source_type: item.source_type.as_ref().to_string(),
                source_team_id: item.source_team_id.clone(),
                slot_code: item.slot_code.clone(),
                qualification_code: item.qualification_code.clone(),
                qualification_level_code: item.qualification_level_code.clone(),
                assigned_at: item.assigned_at,
                check_in_time: item.check_in_time,
                check_out_time: item.check_out_time,
                is_active: item.is_active,
                username: item.username.clone(),
            })
            .collect(),
        equipment_codes: o.equipment_list.iter().map(|e| e.code.clone()).collect(),
        notification_receipt_summary: HashMap::new(),
    }
}

pub(super) fn effective_interval(order: &DispatchOrder, fallback_now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = order
        .actual_start_time
        .or(order.planned_start_time)
        .or(order.assignment_deadline)
        .or(order.created_at)
        .unwrap_or(fallback_now);
    let mut end = order
        .actual_end_time
        .or(order.estimated_completion_time)
        .or(order.planned_end_time)
        .unwrap_or(start + Duration::minutes(20));
    if end <= start {
        end = start + Duration::minutes(8);
    }
    (start, end)
}

pub(super) fn normalize_duration(duration: Duration) -> Duration {
    if duration <= Duration::zero() {
        Duration::minutes(8)
    } else {
        duration
    }
}

pub(super) fn order_member_user_ids(order: &DispatchOrder) -> Vec<String> {
    let mut user_ids = Vec::new();
    let mut seen = HashSet::new();
    if let Some(user_id) = order
        .individual_user_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if seen.insert(user_id) {
            user_ids.push(user_id.to_string());
        }
    }

    for member in order.members.iter().filter(|member| member.is_active) {
        let user_id = member.user_id.trim();
        if !user_id.is_empty() && seen.insert(user_id) {
            user_ids.push(user_id.to_string());
        }
    }

    user_ids
}

fn overlapping_member_user_ids(left: &DispatchOrder, right: &DispatchOrder) -> Vec<String> {
    let left_user_ids = order_member_user_ids(left);
    let right_user_ids = order_member_user_ids(right).into_iter().collect::<HashSet<_>>();
    let mut overlapping = left_user_ids
        .into_iter()
        .filter(|user_id| right_user_ids.contains(user_id))
        .collect::<Vec<_>>();
    overlapping.sort();
    overlapping.dedup();
    overlapping
}

pub(super) fn eta_conflict_kinds(current: &DispatchOrder, candidate: &DispatchOrder) -> Vec<String> {
    let mut conflict_kinds = Vec::new();

    if current.team_id.is_some() && current.team_id == candidate.team_id {
        conflict_kinds.push("team_overlap".to_string());
    }
    if current.individual_user_id.is_some() && current.individual_user_id == candidate.individual_user_id {
        conflict_kinds.push("individual_overlap".to_string());
    }
    if !overlapping_member_user_ids(current, candidate).is_empty()
        && !(current.individual_user_id.is_some() && current.individual_user_id == candidate.individual_user_id)
    {
        conflict_kinds.push("person_time_overlap".to_string());
    }
    if current.stand_id.is_some() && current.stand_id == candidate.stand_id {
        conflict_kinds.push("stand_overlap".to_string());
    }

    conflict_kinds.sort();
    conflict_kinds.dedup();
    conflict_kinds
}

pub(super) fn describe_conflict_kinds(conflict_kinds: &[String]) -> String {
    let labels = conflict_kinds
        .iter()
        .map(|kind| match kind.as_str() {
            "team_overlap" => "班组时间冲突",
            "individual_overlap" => "执行人重叠",
            "person_time_overlap" => "成员编组重叠",
            "stand_overlap" => "机位窗口重叠",
            _ => "资源冲突",
        })
        .collect::<Vec<_>>();
    labels.join("/")
}

/// 地球表面两点间距离（米）
pub(super) fn haversine_distance(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const R: f64 = 6_371_000.0; // 地球半径（米）
    let d_lat = (lat2 - lat1).to_radians();
    let d_lng = (lng2 - lng1).to_radians();
    let a =
        (d_lat / 2.0).sin().powi(2) + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    R * c
}

pub(super) fn equipment_distance_sort_key(equipment: &Equipment, stand_position: Option<(f64, f64)>) -> f64 {
    let Some((stand_lat, stand_lng)) = stand_position else {
        return f64::MAX / 2.0;
    };
    let Some(lat) = equipment.current_position_lat else {
        return f64::MAX / 2.0;
    };
    let Some(lng) = equipment.current_position_lng else {
        return f64::MAX / 2.0;
    };
    haversine_distance(lat, lng, stand_lat, stand_lng)
}
