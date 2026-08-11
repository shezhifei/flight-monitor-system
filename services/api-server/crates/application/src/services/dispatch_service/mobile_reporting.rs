use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::anomaly::*;
use fms_domain::models::dispatch::*;

use crate::schemas::dispatch_schemas::*;

use super::helpers;
use super::{DispatchService, NULL_VALUE};

impl DispatchService {
    pub async fn report_eta(
        &self,
        order_id: &str,
        dto: EtaReportRequest,
        actor_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, false, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        Self::ensure_order_execution_published(&order, "回报预计完成时间")?;

        self.ensure_actor_can_complete_order(&order, order_id, actor_id, "无权回报此派工单预计完成时间")
            .await?;

        if order.status != DispatchOrderStatus::InProgress {
            return Err(DomainError::BusinessRuleViolation(format!(
                "仅作业中派工单可回报预计完成时间，当前状态: {:?}",
                order.status
            )));
        }

        let client_action_id = Self::normalize_optional_ref(dto.client_action_id.as_deref()).map(str::to_string);
        if let Some(ref cid) = client_action_id {
            if self
                .order
                .order_repo
                .has_logged_action(order_id, "estimated_completion_reported", None, Some(cid))
                .await?
            {
                return Ok(serde_json::json!({
                    "dispatch_order_id": order_id,
                    "estimated_completion_time": dto.estimated_completion_time.to_rfc3339(),
                    "estimated_completion_reported_at": order
                        .estimated_completion_reported_at
                        .unwrap_or_else(Utc::now)
                        .to_rfc3339(),
                    "estimated_completion_reported_by": order
                        .estimated_completion_reported_by
                        .clone()
                        .unwrap_or_else(|| actor_id.to_string()),
                    "note": dto.note,
                    "has_conflicts": false,
                    "suggestions": Vec::<Value>::new(),
                    "status": "duplicate",
                    "message": "重复预计完成时间回报已忽略",
                }));
            }
        }

        if let Some(actual_start) = order.actual_start_time {
            if dto.estimated_completion_time < actual_start {
                return Err(DomainError::BusinessRuleViolation(
                    "estimated_completion_time 不能早于 actual_start_time".to_string(),
                ));
            }
        }

        let note = dto.note.as_deref();
        self.order
            .order_repo
            .report_estimated_completion(order_id, dto.estimated_completion_time, actor_id, note)
            .await?;

        let inserted = match client_action_id.as_deref() {
            Some(_) => {
                self.order
                    .order_repo
                    .append_log_once(
                        order_id,
                        "estimated_completion_reported",
                        Some(actor_id),
                        serde_json::json!({
                            "estimated_completion_time": dto.estimated_completion_time.to_rfc3339(),
                            "note": note,
                            "client_action_id": client_action_id.clone(),
                        }),
                    )
                    .await?
            }
            None => {
                self.order
                    .order_repo
                    .append_log(
                        order_id,
                        "estimated_completion_reported",
                        Some(actor_id),
                        Some(serde_json::json!({
                            "estimated_completion_time": dto.estimated_completion_time.to_rfc3339(),
                            "note": note,
                        })),
                    )
                    .await?;
                true
            }
        };
        if !inserted {
            return Ok(serde_json::json!({
                "dispatch_order_id": order_id,
                "estimated_completion_time": dto.estimated_completion_time.to_rfc3339(),
                "estimated_completion_reported_at": order
                    .estimated_completion_reported_at
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339(),
                "estimated_completion_reported_by": order
                    .estimated_completion_reported_by
                    .clone()
                    .unwrap_or_else(|| actor_id.to_string()),
                "note": dto.note,
                "has_conflicts": false,
                "suggestions": Vec::<Value>::new(),
                "status": "duplicate",
                "message": "重复预计完成时间回报已忽略",
            }));
        }

        let (has_conflicts, suggestions) = self
            .build_eta_report_enrichment(&order, dto.estimated_completion_time)
            .await?;

        self.record_collaboration_event(
            &order,
            order_id,
            "eta_reported",
            actor_id,
            client_action_id.clone(),
            serde_json::json!({
                "estimated_completion_time": dto.estimated_completion_time.to_rfc3339(),
                "note": note,
                "has_conflicts": has_conflicts,
            }),
            Utc::now(),
        )
        .await;
        self.sync_dispatch_chat_for_order(order_id).await;
        self.maybe_evaluate_overrun_warning(order_id).await;

        Ok(serde_json::json!({
            "dispatch_order_id": order_id,
            "estimated_completion_time": dto.estimated_completion_time.to_rfc3339(),
            "estimated_completion_reported_at": Utc::now().to_rfc3339(),
            "estimated_completion_reported_by": actor_id,
            "note": dto.note,
            "has_conflicts": has_conflicts,
            "suggestions": suggestions,
        }))
    }

    /// 异常上报 (report-issue)
    pub async fn report_issue(
        &self,
        order_id: &str,
        dto: ReportIssueRequest,
        actor_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        Self::validate_issue_request(&dto)?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, false, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        Self::ensure_order_execution_published(&order, "异常上报")?;

        self.ensure_actor_can_complete_order(&order, order_id, actor_id, "无权上报此派工单异常")
            .await?;

        let anomaly_repo = self
            .notifications
            .anomaly_repo
            .as_ref()
            .ok_or_else(|| DomainError::Internal("异常仓储未配置".to_string()))?;

        let client_action_id = Self::normalize_optional_ref(dto.client_action_id.as_deref()).map(str::to_string);
        if let Some(ref cid) = client_action_id {
            if anomaly_repo
                .find_by_id(&Self::deterministic_issue_anomaly_id(order_id, cid))
                .await?
                .is_some()
            {
                return Ok(serde_json::json!({
                    "success": true,
                    "status": "duplicate",
                    "message": "重复异常上报已忽略",
                    "data": {
                        "anomaly_id": Self::deterministic_issue_anomaly_id(order_id, cid),
                        "dispatch_order_id": order_id,
                    },
                }));
            }
        }

        let input_mode = dto.input_mode.trim().to_ascii_lowercase();
        let issue_type = Self::normalize_issue_type(dto.issue_type.as_deref());
        let severity = Self::parse_issue_severity(dto.severity.as_deref())?;
        let severity_value = match severity {
            AnomalySeverity::Critical => "critical",
            AnomalySeverity::High => "high",
            AnomalySeverity::Medium => "medium",
            AnomalySeverity::Low => "low",
        };
        let anomaly_type = Self::parse_issue_type(&issue_type);
        let title = Self::validate_issue_title(dto.title.as_deref())?;
        let attachments = dto.attachments.as_ref().cloned().unwrap_or_default();
        let voice_attachment_id = Self::normalize_optional_ref(dto.voice_attachment_id.as_deref()).map(str::to_string);

        let resolved_title = if title.is_empty() {
            dto.description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .or_else(|| {
                    dto.note
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                })
                .unwrap_or_else(|| match input_mode.as_str() {
                    "photo" => "现场图片异常首报".to_string(),
                    "voice" => "现场语音异常首报".to_string(),
                    _ => "现场异常首报".to_string(),
                })
        } else {
            title
        };
        let now = Utc::now();
        let anomaly_id = client_action_id
            .as_deref()
            .map(|cid| Self::deterministic_issue_anomaly_id(order_id, cid))
            .unwrap_or_else(|| ulid::Ulid::new().to_string());
        let flight_id = order.flight_id.clone();
        let anomaly = Anomaly {
            anomaly_id: anomaly_id.clone(),
            flight_id: flight_id.clone(),
            anomaly_type,
            severity,
            title: resolved_title.clone(),
            description: dto.description.clone(),
            status: AnomalyStatus::Open,
            detected_at: now,
            resolved_at: None,
            escalation_level: 0,
            last_escalated_at: None,
            linked_todo_id: None,
            rule_id: None,
            context_data: std::collections::HashMap::from([
                ("dispatch_order_id".to_string(), serde_json::json!(order_id)),
                ("reported_by".to_string(), serde_json::json!(actor_id)),
                ("issue_type".to_string(), serde_json::json!(issue_type.clone())),
                ("note".to_string(), serde_json::json!(dto.note.clone())),
                ("attachments".to_string(), serde_json::json!(attachments.clone())),
                (
                    "voice_attachment_id".to_string(),
                    serde_json::json!(voice_attachment_id.clone()),
                ),
                ("input_mode".to_string(), serde_json::json!(input_mode.clone())),
                (
                    "client_action_id".to_string(),
                    serde_json::json!(client_action_id.clone()),
                ),
                ("minimal_first_report".to_string(), serde_json::json!(true)),
                (
                    "position".to_string(),
                    serde_json::json!({
                        "lat": dto.lat,
                        "lng": dto.lng,
                    }),
                ),
            ]),
            created_at: now,
            updated_at: now,
        };
        anomaly_repo.save(&anomaly).await?;

        let inserted = match client_action_id.as_deref() {
            Some(_) => {
                self.order
                    .order_repo
                    .append_log_once(
                        order_id,
                        "issue_reported",
                        Some(actor_id),
                        serde_json::json!({
                            "anomaly_id": anomaly_id.clone(),
                            "title": resolved_title.clone(),
                            "description": dto.description.clone(),
                            "severity": severity_value,
                            "issue_type": issue_type.clone(),
                            "note": dto.note.clone(),
                            "lat": dto.lat,
                            "lng": dto.lng,
                            "attachments": attachments.clone(),
                            "voice_attachment_id": voice_attachment_id.clone(),
                            "input_mode": input_mode.clone(),
                            "client_action_id": client_action_id.clone(),
                            "flight_id": flight_id.clone(),
                        }),
                    )
                    .await?
            }
            None => {
                self.order
                    .order_repo
                    .append_log(
                        order_id,
                        "issue_reported",
                        Some(actor_id),
                        Some(serde_json::json!({
                            "anomaly_id": anomaly_id.clone(),
                            "title": resolved_title.clone(),
                            "description": dto.description.clone(),
                            "severity": severity_value,
                            "issue_type": issue_type.clone(),
                            "note": dto.note.clone(),
                            "lat": dto.lat,
                            "lng": dto.lng,
                            "attachments": attachments.clone(),
                            "voice_attachment_id": voice_attachment_id.clone(),
                            "input_mode": input_mode.clone(),
                            "flight_id": flight_id.clone(),
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
                "message": "重复异常上报已忽略",
                "data": {
                    "anomaly_id": anomaly_id,
                    "dispatch_order_id": order_id,
                    "severity": severity_value,
                    "input_mode": input_mode,
                    "title": resolved_title,
                },
            }));
        }
        self.increment_metric(&format!("dispatch.issue_reported.{input_mode}"));

        self.record_collaboration_event(
            &order,
            order_id,
            "order_issue_reported",
            actor_id,
            client_action_id.clone(),
            serde_json::json!({
                "anomaly_id": anomaly_id.clone(),
                "title": resolved_title.clone(),
                "description": dto.description.clone(),
                "severity": severity_value,
                "issue_type": issue_type.clone(),
                "note": dto.note.clone(),
                "lat": dto.lat,
                "lng": dto.lng,
                "attachments": attachments.clone(),
                "voice_attachment_id": voice_attachment_id.clone(),
                "input_mode": input_mode.clone(),
                "client_action_id": client_action_id.clone(),
            }),
            now,
        )
        .await;

        if matches!(severity, AnomalySeverity::High | AnomalySeverity::Critical) {
            self.send_order_notifications(
                &order,
                actor_id,
                "Dispatch issue requires attention",
                format!(
                    "Dispatch order {} reported issue: {}. Severity: {}.",
                    order_id, resolved_title, severity_value
                ),
                if matches!(severity, AnomalySeverity::Critical) {
                    "critical"
                } else {
                    "warning"
                },
                matches!(severity, AnomalySeverity::Critical),
                "dispatch_issue",
            )
            .await;
        }

        Ok(serde_json::json!({
            "success": true,
            "message": "异常已上报",
            "data": {
                "anomaly_id": anomaly_id,
                "dispatch_order_id": order_id,
                "severity": severity_value,
                "input_mode": input_mode,
                "title": resolved_title,
            },
        }))
    }

    /// 移动端离线动作同步 (mobile/sync/actions)
    pub async fn sync_mobile_actions(
        &self,
        dto: MobileSyncRequest,
        actor_id: &str,
    ) -> Result<MobileSyncResponse, DomainError> {
        Self::ensure_actor(actor_id)?;
        let mut results = Vec::new();
        let mut applied = 0i64;
        let mut duplicates = 0i64;
        let mut failed = 0i64;
        let total = dto.actions.len() as i64;

        let action_log_map: std::collections::HashMap<&str, &str> = [
            ("accept", "accepted"),
            ("checkin", "checkin"),
            ("checkout", "checkout"),
            ("start", "started"),
            ("complete", "completed"),
            ("eta_report", "estimated_completion_reported"),
            ("report_issue", "issue_reported"),
        ]
        .iter()
        .copied()
        .collect();

        for action in &dto.actions {
            let action_type = action.action_type.trim();
            let order_id = action.dispatch_order_id.trim();
            let client_action_id = action.client_action_id.trim();
            let normalized_client_action_id = Self::normalize_optional_ref(Some(client_action_id)).map(str::to_string);
            let action_log = action_log_map.get(action_type).copied().unwrap_or(action_type);

            match self
                .order
                .order_repo
                .has_logged_action(order_id, action_log, None, normalized_client_action_id.as_deref())
                .await
            {
                Ok(true) => {
                    duplicates += 1;
                    results.push(Self::mobile_sync_result(
                        Some(client_action_id),
                        order_id,
                        action_type,
                        "duplicate",
                        "重复动作已忽略",
                    ));
                    continue;
                }
                Ok(false) => {}
                Err(_) => {}
            }

            let payload = action.payload.clone().unwrap_or(serde_json::json!({}));
            let result: Result<Option<Value>, DomainError> = match action_type {
                "accept" => {
                    let accept_dto = DispatchOrderAcceptRequest {
                        note: payload.get("note").and_then(|v| v.as_str()).map(String::from),
                        client_action_id: normalized_client_action_id.clone(),
                    };
                    self.accept_order(order_id, accept_dto, actor_id).await?;
                    Ok(None)
                }
                "start" => {
                    let actual_start_time = payload
                        .get("actual_start_time")
                        .and_then(|v| v.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&Utc))
                        .or_else(|| {
                            action
                                .action_timestamp
                                .as_deref()
                                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                .map(|dt| dt.with_timezone(&Utc))
                        });
                    let start_dto = DispatchOrderStart {
                        actual_start_time,
                        position: None,
                        notes: None,
                        client_action_id: normalized_client_action_id.clone(),
                    };
                    self.start_order(order_id, start_dto, actor_id).await?;
                    Ok(None)
                }
                "complete" => {
                    let actual_end_time = payload
                        .get("actual_end_time")
                        .and_then(|v| v.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&Utc))
                        .or_else(|| {
                            action
                                .action_timestamp
                                .as_deref()
                                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                .map(|dt| dt.with_timezone(&Utc))
                        });
                    let complete_dto = DispatchOrderCompleteReq {
                        actual_end_time,
                        position: None,
                        completion_notes: payload
                            .get("completion_notes")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        issues: vec![],
                        client_action_id: normalized_client_action_id.clone(),
                    };
                    self.complete_order(order_id, complete_dto, actor_id).await?;
                    Ok(None)
                }
                "checkin" => {
                    let ci_dto = DispatchOrderCheckInRequest {
                        qr_code: payload.get("qr_code").and_then(|v| v.as_str()).map(String::from),
                        lat: payload.get("lat").and_then(|v| v.as_f64()),
                        lng: payload.get("lng").and_then(|v| v.as_f64()),
                        accuracy_m: payload.get("accuracy_m").and_then(|v| v.as_f64()),
                        note: payload.get("note").and_then(|v| v.as_str()).map(String::from),
                        client_action_id: normalized_client_action_id.clone(),
                    };
                    self.checkin_order(order_id, ci_dto, actor_id).await?;
                    Ok(None)
                }
                "checkout" => {
                    let recorded_at = payload
                        .get("recorded_at")
                        .and_then(|v| v.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&Utc))
                        .or_else(|| {
                            action
                                .action_timestamp
                                .as_deref()
                                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                .map(|dt| dt.with_timezone(&Utc))
                        });
                    let co_dto = DispatchOrderCheckOutRequest {
                        lat: payload.get("lat").and_then(|v| v.as_f64()),
                        lng: payload.get("lng").and_then(|v| v.as_f64()),
                        note: payload.get("note").and_then(|v| v.as_str()).map(String::from),
                        client_action_id: normalized_client_action_id.clone(),
                        recorded_at,
                    };
                    let checkout_result = self.checkout_order(order_id, co_dto, actor_id).await?;
                    Ok(Some(checkout_result))
                }
                "eta_report" => {
                    let eta_time = Self::parse_action_timestamp(
                        payload.get("estimated_completion_time").and_then(|v| v.as_str()),
                        action.action_timestamp.as_deref(),
                    )
                    .unwrap_or_else(Utc::now);
                    let eta_dto = EtaReportRequest {
                        estimated_completion_time: eta_time,
                        note: payload.get("note").and_then(|v| v.as_str()).map(String::from),
                        client_action_id: normalized_client_action_id.clone(),
                    };
                    let eta_result = self.report_eta(order_id, eta_dto, actor_id).await?;
                    Ok(Some(eta_result))
                }
                "report_issue" => {
                    let issue_dto = ReportIssueRequest {
                        title: payload.get("title").and_then(|v| v.as_str()).map(String::from),
                        description: payload.get("description").and_then(|v| v.as_str()).map(String::from),
                        severity: payload.get("severity").and_then(|v| v.as_str()).map(String::from),
                        issue_type: payload.get("issue_type").and_then(|v| v.as_str()).map(String::from),
                        note: payload.get("note").and_then(|v| v.as_str()).map(String::from),
                        lat: payload.get("lat").and_then(|v| v.as_f64()),
                        lng: payload.get("lng").and_then(|v| v.as_f64()),
                        attachments: payload.get("attachments").and_then(|v| v.as_array()).map(|items| {
                            items
                                .iter()
                                .filter_map(|item| item.as_str().map(String::from))
                                .collect()
                        }),
                        client_action_id: normalized_client_action_id.clone(),
                        input_mode: payload
                            .get("input_mode")
                            .and_then(|v| v.as_str())
                            .unwrap_or("text")
                            .to_string(),
                        voice_attachment_id: payload
                            .get("voice_attachment_id")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    };
                    let issue_result = self.report_issue(order_id, issue_dto, actor_id).await?;
                    Ok(Some(issue_result))
                }
                _ => Err(DomainError::BusinessRuleViolation(format!(
                    "不支持的动作类型: {action_type}"
                ))),
            };

            match result {
                Ok(_command_result) => {
                    applied += 1;
                    results.push(Self::mobile_sync_result(
                        Some(client_action_id),
                        order_id,
                        action_type,
                        "applied",
                        "补传成功",
                    ));
                }
                Err(e) => {
                    failed += 1;
                    results.push(Self::mobile_sync_error_result(
                        Some(client_action_id),
                        order_id,
                        action_type,
                        e,
                    ));
                }
            }
        }

        Ok(MobileSyncResponse {
            total,
            applied,
            duplicates,
            failed,
            results,
        })
    }

    pub(super) async fn try_auto_complete_on_all_checkout(
        &self,
        order_id: &str,
        actor_id: &str,
        all_members: &[DispatchOrderMember],
    ) -> bool {
        if all_members.is_empty() || all_members.iter().any(|member| member.check_out_time.is_none()) {
            return false;
        }
        self.complete_order(
            order_id,
            DispatchOrderCompleteReq {
                actual_end_time: None,
                position: None,
                completion_notes: Some("全员签退自动完结".to_string()),
                issues: vec![],
                client_action_id: None,
            },
            actor_id,
        )
        .await
        .is_ok()
    }

    async fn build_eta_report_enrichment(
        &self,
        order: &DispatchOrder,
        estimated_completion_time: DateTime<Utc>,
    ) -> Result<(bool, Vec<Value>), DomainError> {
        let horizon_start = [order.actual_start_time, order.planned_start_time, Some(Utc::now())]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or_else(Utc::now);
        let base_end = order.planned_end_time.unwrap_or(estimated_completion_time);
        let horizon_end = estimated_completion_time
            .max(base_end)
            .max(horizon_start + Duration::minutes(30));
        let search_end = horizon_end + helpers::normalize_duration(horizon_end - horizon_start);
        let orders = self
            .order
            .order_repo
            .find_orders_in_window(
                horizon_start,
                search_end,
                &["pending", "assigned", "in_progress"],
                None,
                None,
                None,
                false,
            )
            .await?;

        let current_start = order
            .actual_start_time
            .or(order.planned_start_time)
            .or(order.assignment_deadline)
            .or(order.created_at)
            .unwrap_or(horizon_start);
        let current_end = if estimated_completion_time <= current_start {
            current_start + Duration::minutes(8)
        } else {
            estimated_completion_time
        };

        let mut suggestions = orders
            .into_iter()
            .filter(|candidate| candidate.id != order.id)
            .filter(|candidate| {
                !matches!(
                    candidate.status,
                    DispatchOrderStatus::Completed | DispatchOrderStatus::Cancelled
                )
            })
            .filter_map(|candidate| {
                let (candidate_start, candidate_end) = helpers::effective_interval(&candidate, horizon_start);
                if candidate_end <= current_start || current_end <= candidate_start {
                    return None;
                }

                let conflict_kinds = helpers::eta_conflict_kinds(order, &candidate);
                if conflict_kinds.is_empty() {
                    return None;
                }

                let overlap_start = current_start.max(candidate_start);
                let overlap_end = current_end.min(candidate_end);
                let overlap_duration = overlap_end - overlap_start;
                if overlap_duration <= Duration::zero() {
                    return None;
                }

                let candidate_duration = helpers::normalize_duration(candidate_end - candidate_start);
                let buffer = Duration::minutes(5);
                let (suggested_start, suggested_end, suggestion_type) = if candidate_start >= current_start {
                    let next_start = current_end + buffer;
                    (next_start, next_start + candidate_duration, "shift_later")
                } else {
                    let next_end = current_start - buffer;
                    let next_start = next_end - candidate_duration;
                    if next_start < horizon_start - Duration::minutes(240) {
                        let fallback_start = current_end + buffer;
                        (fallback_start, fallback_start + candidate_duration, "shift_later")
                    } else {
                        (next_start, next_end, "shift_earlier")
                    }
                };

                let impact_score = ((overlap_duration.num_seconds() as f64 / 3600.0) * 100.0).round() / 100.0;
                let reason = format!(
                    "ETA 延长后与 {} 发生{}",
                    order.id,
                    helpers::describe_conflict_kinds(&conflict_kinds),
                );

                Some(json!({
                    "dispatch_order_id": candidate.id,
                    "reason": reason,
                    "original_start_time": candidate_start.to_rfc3339(),
                    "original_end_time": candidate_end.to_rfc3339(),
                    "suggested_start_time": suggested_start.to_rfc3339(),
                    "suggested_end_time": suggested_end.to_rfc3339(),
                    "related_dispatch_order_id": order.id,
                    "impact_score": impact_score,
                    "suggestion_type": suggestion_type,
                    "conflict_types": conflict_kinds,
                }))
            })
            .collect::<Vec<_>>();

        suggestions.sort_by(|left, right| {
            let left_score = left.get("impact_score").and_then(Value::as_f64).unwrap_or(0.0);
            let right_score = right.get("impact_score").and_then(Value::as_f64).unwrap_or(0.0);
            right_score
                .partial_cmp(&left_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        suggestions.truncate(20);

        Ok((!suggestions.is_empty(), suggestions))
    }
}
