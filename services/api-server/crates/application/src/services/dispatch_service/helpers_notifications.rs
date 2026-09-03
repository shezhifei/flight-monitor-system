use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
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
}
