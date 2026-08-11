//! Ontology V1 建议动作（契约 §3.2；交接 Phase 2）
//!
//! `flight.suggest_stand_adjustment` / `dispatch.suggest_replan` /
//! `anomaly.suggest_escalation` / `flight.suggest_delay_action` /
//! `notification.suggest_broadcast`。
//!
//! 规则（契约 §4.3）：建议动作不直接写业务表，只生成 proposal 载荷
//! （risk_level / approval_policy / constraint_results / before_snapshot /
//! after_preview），由现有 proposal / pending-action / approval 管线消费。

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use fms_domain::models::anomaly::AnomalySeverity;
use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ontology::schema_export::FLIGHT_OPS_ONTOLOGY_VERSION;
use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::dispatch_repository::{DispatchOrderRepository, StandRepository, TeamRepository};
use fms_domain::ports::flight_repository::FlightRepository;
use fms_domain::ports::ontology_repository::StandOccupationRepository;

use super::ontology_read_action_service::ReadActionError;

const CANDIDATE_STANDS_SCANNED: i64 = 20;
const CANDIDATE_TEAMS_SCANNED: i64 = 20;
/// 建议 proposal 默认有效期（契约：过期 proposal 必须拒绝执行）。
const SUGGESTION_TTL_MINUTES: i64 = 30;

/// 动作名 → 所需权限（建议动作只读现状并生成 proposal，权限取对应域的读权限；
/// notification.suggest_broadcast 保守起见要求发送权限）。
pub fn advisory_action_permission(action_name: &str) -> Option<&'static str> {
    match action_name {
        "flight.suggest_stand_adjustment" | "flight.suggest_delay_action" => Some("flight:read"),
        "dispatch.suggest_replan" => Some("dispatch:read"),
        "anomaly.suggest_escalation" => Some("anomaly:read"),
        "notification.suggest_broadcast" => Some("notification:send"),
        _ => None,
    }
}

fn repo_err(error: impl std::fmt::Display) -> ReadActionError {
    ReadActionError::Repository(error.to_string())
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).filter(|value| !value.trim().is_empty())
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ReadActionError> {
    arg_str(args, key)
        .ok_or_else(|| ReadActionError::InvalidArguments(format!("missing required argument `{key}`")))
}

fn arg_datetime(args: &Value, key: &str) -> Result<Option<DateTime<Utc>>, ReadActionError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => raw
            .parse::<DateTime<Utc>>()
            .map(Some)
            .map_err(|_| ReadActionError::InvalidArguments(format!("`{key}` is not an RFC3339 datetime"))),
        Some(_) => Err(ReadActionError::InvalidArguments(format!(
            "`{key}` must be an RFC3339 datetime string"
        ))),
    }
}

fn constraint(name: &str, passed: bool, severity: &str, message: Option<&str>) -> Value {
    json!({
        "constraint_name": name,
        "constraint_type": "Precondition",
        "passed": passed,
        "severity": severity,
        "message": message,
    })
}

fn evidence(context: Value) -> Value {
    json!({
        "retrieved_at": Utc::now(),
        "ontology_version": FLIGHT_OPS_ONTOLOGY_VERSION,
        "context": context,
    })
}

/// 建议动作统一输出：指向受控写动作的 proposal 载荷（不落库、不执行业务写）。
fn suggestion_envelope(
    object_type: &str,
    object_id: &str,
    action_name: &str,
    arguments: Value,
    risk_level: &str,
    constraint_results: Vec<Value>,
    before_snapshot: Value,
    after_preview: Value,
    confidence: f64,
    reasoning: &str,
    extra: Value,
) -> Value {
    let now = Utc::now();
    let mut payload = json!({
        "suggestion": {
            "ontology_version": FLIGHT_OPS_ONTOLOGY_VERSION,
            "object_type": object_type,
            "object_id": object_id,
            "action_name": action_name,
            "arguments": arguments,
            "risk_level": risk_level,
            "approval_policy": "require_approval",
            "constraint_results": constraint_results,
            "before_snapshot": before_snapshot,
            "after_preview": after_preview,
            "confidence": confidence,
            "reasoning": reasoning,
            "expires_at": now + Duration::minutes(SUGGESTION_TTL_MINUTES),
        },
        "evidence": evidence(json!({})),
    });
    if let (Some(root), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            root.insert(key.clone(), value.clone());
        }
    }
    payload
}

pub struct OntologyAdvisoryService {
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    stand_repo: Arc<dyn StandRepository + Send + Sync>,
    stand_occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
    dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    team_repo: Arc<dyn TeamRepository + Send + Sync>,
    anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
}

impl OntologyAdvisoryService {
    pub fn new(
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        stand_repo: Arc<dyn StandRepository + Send + Sync>,
        stand_occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
        dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
    ) -> Self {
        Self {
            flight_repo,
            stand_repo,
            stand_occupation_repo,
            dispatch_repo,
            team_repo,
            anomaly_repo,
        }
    }

    pub async fn execute(&self, action_name: &str, arguments: &Value) -> Result<Value, ReadActionError> {
        match action_name {
            "flight.suggest_stand_adjustment" => self.flight_suggest_stand_adjustment(arguments).await,
            "dispatch.suggest_replan" => self.dispatch_suggest_replan(arguments).await,
            "anomaly.suggest_escalation" => self.anomaly_suggest_escalation(arguments).await,
            "flight.suggest_delay_action" => self.flight_suggest_delay_action(arguments).await,
            "notification.suggest_broadcast" => self.notification_suggest_broadcast(arguments).await,
            other => Err(ReadActionError::UnknownAction(other.to_string())),
        }
    }

    /// StandRecommendationService：为航班生成换机位 proposal（update_stand）。
    async fn flight_suggest_stand_adjustment(&self, args: &Value) -> Result<Value, ReadActionError> {
        let flight_id = required_str(args, "flight_id")?;
        let flight = self
            .flight_repo
            .find_by_id(flight_id)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| ReadActionError::NotFound(format!("flight {flight_id}")))?;

        // 时间窗：优先 实际/预计到达 → 实际/预计起飞，回退 now → now+2h。
        let now = Utc::now();
        let window_start = flight
            .actual_arrival
            .or(flight.estimated_arrival)
            .or(flight.scheduled_arrival)
            .unwrap_or(now);
        let window_end = flight
            .actual_departure
            .or(flight.estimated_departure)
            .or(flight.scheduled_departure)
            .unwrap_or(window_start + Duration::hours(2));
        let (window_start, window_end) = if window_end <= window_start {
            (now, now + Duration::hours(2))
        } else {
            (window_start, window_end)
        };

        let current_stand = flight.stand.clone();
        let requested = arg_str(args, "new_stand_id");
        let candidates = self
            .stand_repo
            .find_all(None, false, CANDIDATE_STANDS_SCANNED, 0)
            .await
            .map_err(repo_err)?;

        // 目标机位：显式指定时校验存在性；否则扫描窗口内无占用冲突的可用机位。
        let (target, overlap_conflicts) = match requested {
            Some(code) => {
                let stand = candidates
                    .iter()
                    .find(|s| s.code == code || s.id == code)
                    .cloned()
                    .ok_or_else(|| ReadActionError::NotFound(format!("stand {code}")))?;
                let overlaps = self
                    .stand_occupation_repo
                    .list_overlapping(&stand.code, window_start, window_end)
                    .await
                    .map_err(repo_err)?;
                (stand, overlaps)
            }
            None => {
                let mut chosen = None;
                for candidate in &candidates {
                    if current_stand.as_ref().is_some_and(|s| s.as_str() == candidate.code.as_str()) {
                        continue;
                    }
                    let overlaps = self
                        .stand_occupation_repo
                        .list_overlapping(&candidate.code, window_start, window_end)
                        .await
                        .map_err(repo_err)?;
                    if overlaps.is_empty() {
                        chosen = Some((candidate.clone(), overlaps));
                        break;
                    }
                }
                chosen.ok_or_else(|| {
                    ReadActionError::NotFound("no available stand in scanned candidates".to_string())
                })?
            }
        };

        // 机位时段重叠是 warning（维护规则 3），不硬拦。
        let mut constraint_results = vec![
            constraint("target_stand_exists", true, "error", None),
            constraint("target_stand_active", target.is_active, "error", None),
        ];
        if overlap_conflicts.is_empty() {
            constraint_results.push(constraint("no_occupation_overlap", true, "warning", None));
        } else {
            constraint_results.push(constraint(
                "no_occupation_overlap",
                false,
                "warning",
                Some(&format!("{} overlapping occupation(s)", overlap_conflicts.len())),
            ));
        }

        let confidence = if overlap_conflicts.is_empty() { 0.9 } else { 0.6 };
        Ok(suggestion_envelope(
            "Flight",
            flight_id,
            "update_stand",
            json!({ "new_stand_id": target.code }),
            "medium",
            constraint_results,
            json!({ "stand": current_stand }),
            json!({ "stand": target.code }),
            confidence,
            &format!(
                "stand {} suggested for flight {} in window {} ~ {}",
                target.code, flight_id, window_start, window_end
            ),
            json!({
                "conflicts": overlap_conflicts.iter().map(|o| json!({
                    "registration": o.registration,
                    "start_time": o.starts_at,
                    "end_time": o.ends_at,
                })).collect::<Vec<_>>(),
            }),
        ))
    }

    /// DispatchReplanAdvisorService：为派工单生成改派 proposal（reassign）。
    async fn dispatch_suggest_replan(&self, args: &Value) -> Result<Value, ReadActionError> {
        let order_id = required_str(args, "dispatch_order_id")?;
        let reason = required_str(args, "reason")?;
        let order = self
            .dispatch_repo
            .find_by_id(order_id, false, None)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| ReadActionError::NotFound(format!("dispatch order {order_id}")))?;

        // 候选班组：显式指定则校验；否则取可用班组（按班组类型/航站楼不过滤，保持只读简单性）。
        let target_team = match arg_str(args, "target_team_id") {
            Some(team_id) => self
                .team_repo
                .find_by_id(team_id, false)
                .await
                .map_err(repo_err)?
                .ok_or_else(|| ReadActionError::NotFound(format!("team {team_id}")))?,
            None => {
                let available = self
                    .team_repo
                    .find_available_for_dispatch(None, order.terminal.as_deref())
                    .await
                    .map_err(repo_err)?;
                let mut teams = available
                    .into_iter()
                    .filter(|team| team.id != order.team_id.as_deref().unwrap_or(""))
                    .collect::<Vec<_>>();
                teams.truncate(CANDIDATE_TEAMS_SCANNED as usize);
                teams.into_iter().next().ok_or_else(|| {
                    ReadActionError::NotFound("no available team for replan".to_string())
                })?
            }
        };

        // 目标班组在订单窗口内的既有冲突。
        let conflicts = match (order.planned_start_time, order.planned_end_time) {
            (Some(start), Some(end)) if end > start => self
                .dispatch_repo
                .find_overlapping_orders(start, end, Some(&target_team.id), None, None, Some(&order.id))
                .await
                .map_err(repo_err)?,
            _ => Vec::new(),
        };

        let mut constraint_results = vec![
            constraint("target_team_exists", true, "error", None),
            constraint("target_team_active", target_team.is_active, "error", None),
            constraint("target_team_different", target_team.id != order.team_id.as_deref().unwrap_or(""), "warning", None),
        ];
        if conflicts.is_empty() {
            constraint_results.push(constraint("no_window_conflict", true, "warning", None));
        } else {
            constraint_results.push(constraint(
                "no_window_conflict",
                false,
                "warning",
                Some(&format!("{} conflicting order(s) for target team", conflicts.len())),
            ));
        }

        // 分数启发式：无冲突 + 活跃班组 = 高分。
        let score_before = 0.5f64;
        let score_after = if conflicts.is_empty() && target_team.is_active { 0.9 } else { 0.55 };
        let confidence = if conflicts.is_empty() { 0.85 } else { 0.5 };

        Ok(suggestion_envelope(
            "DispatchOrder",
            order_id,
            "reassign",
            json!({ "assignee_id": target_team.id, "reason": reason }),
            "high",
            constraint_results,
            json!({ "team_id": order.team_id, "status": order.status.as_ref() }),
            json!({ "team_id": target_team.id, "team_name": target_team.name }),
            confidence,
            &format!("replan order {} to team {}: {}", order_id, target_team.id, reason),
            json!({
                "resource_changes": [{
                    "kind": "team",
                    "from": order.team_id,
                    "to": target_team.id,
                }],
                "score_before": score_before,
                "score_after": score_after,
                "conflicts": conflicts.iter().map(|c| json!({
                    "order_id": c.id,
                    "task_type": c.task_type,
                })).collect::<Vec<_>>(),
            }),
        ))
    }

    /// AnomalyEscalationAdvisorService：为异常生成升级 proposal（escalate）。
    async fn anomaly_suggest_escalation(&self, args: &Value) -> Result<Value, ReadActionError> {
        let anomaly_id = required_str(args, "anomaly_id")?;
        let anomaly = self
            .anomaly_repo
            .find_by_id(anomaly_id)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| ReadActionError::NotFound(format!("anomaly {anomaly_id}")))?;
        if anomaly.status == fms_domain::models::anomaly::AnomalyStatus::Resolved {
            return Err(ReadActionError::InvalidArguments(format!(
                "anomaly {anomaly_id} is already resolved"
            )));
        }

        let now = Utc::now();
        let age_minutes = (now - anomaly.detected_at).num_minutes();
        let unacknowledged = anomaly.status == fms_domain::models::anomaly::AnomalyStatus::Open;
        // 升级类型：critical 或超时未认领 → 严重度升级；否则处理路径升级。
        let (escalation_type, severity_after) =
            if anomaly.severity == AnomalySeverity::Critical || (unacknowledged && age_minutes >= 60) {
                ("severity_escalation", "critical")
            } else {
                ("handling_escalation", anomaly.severity.as_ref())
            };

        let constraint_results = vec![
            constraint("anomaly_unresolved", true, "error", None),
            constraint(
                "escalation_needed",
                anomaly.severity == AnomalySeverity::Critical || unacknowledged,
                "warning",
                Some(&format!("age {} min, status {}", age_minutes, anomaly.status.as_ref())),
            ),
        ];

        let reason = format!(
            "{}: anomaly {} ({}) open for {} min",
            escalation_type, anomaly_id, anomaly.severity.as_ref(), age_minutes
        );
        Ok(suggestion_envelope(
            "Anomaly",
            anomaly_id,
            "escalate",
            json!({ "reason": reason }),
            "medium",
            constraint_results,
            json!({
                "severity": anomaly.severity.as_ref(),
                "status": anomaly.status.as_ref(),
                "escalation_level": anomaly.escalation_level,
            }),
            json!({
                "escalation_level": anomaly.escalation_level + 1,
                "severity": severity_after,
            }),
            0.8,
            &reason,
            json!({
                "escalation_type": escalation_type,
                "targets": {
                    "notification": {
                        "action_name": "send",
                        "title": format!("[{}] {}", escalation_type, anomaly.title),
                        "body": reason,
                    },
                    "todo": anomaly.linked_todo_id,
                },
            }),
        ))
    }

    /// DelayAdvisorService：为延误航班生成处置 proposal（update_delay + 关联派工动作）。
    async fn flight_suggest_delay_action(&self, args: &Value) -> Result<Value, ReadActionError> {
        let flight_id = required_str(args, "flight_id")?;
        let flight = self
            .flight_repo
            .find_by_id(flight_id)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| ReadActionError::NotFound(format!("flight {flight_id}")))?;

        let new_departure = match arg_datetime(args, "new_estimated_departure")? {
            Some(value) => value,
            // 默认：原预计/计划起飞 +30 分钟。
            None => flight
                .estimated_departure
                .or(flight.scheduled_departure)
                .map(|base| base + Duration::minutes(30))
                .ok_or_else(|| {
                    ReadActionError::InvalidArguments(
                        "flight has no departure time; provide `new_estimated_departure`".to_string(),
                    )
                })?,
        };

        let delayed = flight.status == FlightStatus::Delayed;
        let open_anomalies = self
            .anomaly_repo
            .find_by_flight(flight_id)
            .await
            .map_err(repo_err)?
            .into_iter()
            .filter(|a| a.status != fms_domain::models::anomaly::AnomalyStatus::Resolved)
            .collect::<Vec<_>>();
        let pending_orders = self
            .dispatch_repo
            .find_by_flight(flight_id)
            .await
            .map_err(repo_err)?
            .into_iter()
            .filter(|o| matches!(o.status.as_ref(), "pending" | "assigned"))
            .collect::<Vec<_>>();
        // 计划时间早于新起飞时间的派工需要随之调整。
        let impacted_orders = pending_orders
            .iter()
            .filter(|o| o.planned_start_time.is_some_and(|t| t < new_departure))
            .map(|o| {
                json!({
                    "dispatch_order_id": o.id,
                    "task_type": o.task_type,
                    "planned_start_time": o.planned_start_time,
                    "suggested_action": "reschedule_after_new_departure",
                })
            })
            .collect::<Vec<_>>();

        let constraint_results = vec![
            constraint("flight_exists", true, "error", None),
            constraint("flight_delayed", delayed, "warning", None),
            constraint(
                "new_departure_after_current",
                flight.estimated_departure.or(flight.scheduled_departure).is_none_or(|base| new_departure > base),
                "warning",
                None,
            ),
        ];

        Ok(suggestion_envelope(
            "Flight",
            flight_id,
            "update_delay",
            json!({ "new_estimated_departure": new_departure }),
            "medium",
            constraint_results,
            json!({
                "status": flight.status.code(),
                "estimated_departure": flight.estimated_departure,
                "scheduled_departure": flight.scheduled_departure,
            }),
            json!({ "estimated_departure": new_departure }),
            if delayed { 0.85 } else { 0.5 },
            &format!(
                "delay handling for flight {}: new departure {} with {} impacted dispatch order(s)",
                flight_id,
                new_departure,
                impacted_orders.len()
            ),
            json!({
                "open_anomalies": open_anomalies.iter().map(|a| json!({
                    "anomaly_id": a.anomaly_id,
                    "severity": a.severity.as_ref(),
                })).collect::<Vec<_>>(),
                "related_dispatch_actions": impacted_orders,
            }),
        ))
    }

    /// Notification broadcast advisor：只生成广播 proposal，不产生发送副作用。
    async fn notification_suggest_broadcast(&self, args: &Value) -> Result<Value, ReadActionError> {
        let title = required_str(args, "title")?;
        let body = required_str(args, "body")?;
        let scope = arg_str(args, "scope").unwrap_or("all");
        if !matches!(scope, "all" | "on_duty_teams" | "department") {
            return Err(ReadActionError::InvalidArguments(
                "`scope` must be one of all|on_duty_teams|department".to_string(),
            ));
        }
        if scope == "department" && arg_str(args, "department_id").is_none() {
            return Err(ReadActionError::InvalidArguments(
                "`department_id` is required when scope is department".to_string(),
            ));
        }

        // 接收面：按 scope 推导受众描述；具体用户解析在受控执行阶段完成（无副作用）。
        let recipients = match scope {
            "on_duty_teams" => json!({ "kind": "team_status", "team_status": "on_duty" }),
            "department" => json!({ "kind": "department", "department_id": arg_str(args, "department_id") }),
            _ => json!({ "kind": "all_users" }),
        };

        let constraint_results = vec![
            constraint("title_present", true, "error", None),
            constraint("body_present", true, "error", None),
            constraint("recipients_resolvable", true, "warning", None),
        ];

        Ok(suggestion_envelope(
            "Notification",
            "broadcast",
            "send",
            json!({ "title": title, "body": body, "recipients": recipients }),
            "medium",
            constraint_results,
            Value::Null,
            json!({ "title": title, "recipients": recipients }),
            0.9,
            &format!("broadcast proposal '{}' for scope {}", title, scope),
            json!({ "side_effects": "none until approval" }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ontology_read_action_service::tests::{
        anomaly_fixture, flight_fixture, occupation_fixture, order_fixture, stand_fixture,
        FakeAnomalyRepo, FakeDispatchRepo, FakeFlightRepo, FakeOccupationRepo, FakeStandRepo,
        FakeTeamRepo,
    };
    use chrono::Utc;
    use fms_domain::models::anomaly::{AnomalySeverity, AnomalyStatus};
    use fms_domain::models::dispatch::Team;
    use fms_domain::models::value_objects::FlightStatus;

    fn advisory_service(
        flights: Vec<fms_domain::models::flight::Flight>,
        stands: Vec<fms_domain::models::dispatch::Stand>,
        occupations: Vec<fms_domain::models::ontology_v1::StandOccupation>,
        orders: Vec<fms_domain::models::dispatch::DispatchOrder>,
        teams: Vec<Team>,
        anomalies: Vec<fms_domain::models::anomaly::Anomaly>,
    ) -> OntologyAdvisoryService {
        OntologyAdvisoryService::new(
            Arc::new(FakeFlightRepo { flights: std::sync::Mutex::new(flights) }),
            Arc::new(FakeStandRepo { stands: std::sync::Mutex::new(stands) }),
            Arc::new(FakeOccupationRepo { occupations: std::sync::Mutex::new(occupations) }),
            Arc::new(FakeDispatchRepo { orders: std::sync::Mutex::new(orders) }),
            Arc::new(FakeTeamRepo { teams: std::sync::Mutex::new(teams) }),
            Arc::new(FakeAnomalyRepo { anomalies: std::sync::Mutex::new(anomalies) }),
        )
    }

    fn team_fixture(id: &str, name: &str) -> Team {
        Team {
            id: id.to_string(),
            name: name.to_string(),
            team_type_id: None,
            code: None,
            leader_id: None,
            terminal: None,
            current_status: fms_domain::models::dispatch::TeamStatus::OnDuty,
            current_position_lat: None,
            current_position_lng: None,
            current_stand_id: None,
            last_position_update: None,
            created_at: None,
            updated_at: None,
            is_active: true,
            team_type: None,
            members: vec![],
        }
    }

    #[tokio::test]
    async fn unknown_advisory_action_is_rejected() {
        let svc = advisory_service(vec![], vec![], vec![], vec![], vec![], vec![]);
        let err = svc.execute("flight.suggest_nothing", &json!({})).await.unwrap_err();
        assert!(matches!(err, ReadActionError::UnknownAction(_)));
    }

    #[tokio::test]
    async fn suggest_stand_adjustment_picks_conflict_free_stand() {
        let mut flight = flight_fixture("FL1", FlightStatus::Arrived, true, true);
        flight.stand = Some("S1".into());
        let svc = advisory_service(
            vec![flight],
            vec![stand_fixture("ST1", "S1", true), stand_fixture("ST2", "S2", true)],
            // S2 在航班窗口外占用，不冲突（航班窗口由 fixture now 推导）。
            vec![occupation_fixture("S2", "B-9999", 300, 420)],
            vec![],
            vec![],
            vec![],
        );
        let result = svc
            .execute("flight.suggest_stand_adjustment", &json!({"flight_id": "FL1"}))
            .await
            .expect("stand suggestion");
        let suggestion = &result["suggestion"];
        assert_eq!(suggestion["action_name"], "update_stand");
        assert_eq!(suggestion["object_type"], "Flight");
        assert_eq!(suggestion["object_id"], "FL1");
        assert_eq!(suggestion["arguments"]["new_stand_id"], "S2");
        assert_eq!(suggestion["risk_level"], "medium");
        assert_eq!(suggestion["approval_policy"], "require_approval");
        assert_eq!(suggestion["before_snapshot"]["stand"], "S1");
        assert_eq!(suggestion["after_preview"]["stand"], "S2");
        assert!(suggestion["expires_at"].is_string());
        let constraints = suggestion["constraint_results"].as_array().expect("constraints");
        assert!(constraints.iter().all(|c| c["constraint_name"].is_string()));
        assert!(constraints
            .iter()
            .any(|c| c["constraint_name"] == "no_occupation_overlap" && c["passed"].as_bool() == Some(true)));
        assert!(result["conflicts"].as_array().unwrap().is_empty());
        assert!(result["evidence"]["retrieved_at"].is_string());
    }

    #[tokio::test]
    async fn suggest_stand_adjustment_reports_overlap_warning_not_block() {
        // 维护规则 3：机位时段重叠是 warning，不得硬拦。
        let flight = flight_fixture("FL1", FlightStatus::Arrived, true, true);
        let svc = advisory_service(
            vec![flight],
            vec![stand_fixture("ST2", "S2", true)],
            vec![occupation_fixture("S2", "B-9999", -60, 240)],
            vec![],
            vec![],
            vec![],
        );
        let result = svc
            .execute(
                "flight.suggest_stand_adjustment",
                &json!({"flight_id": "FL1", "new_stand_id": "S2"}),
            )
            .await
            .expect("overlap must be warning, not hard reject");
        let suggestion = &result["suggestion"];
        let overlap = suggestion["constraint_results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["constraint_name"] == "no_occupation_overlap")
            .cloned()
            .expect("overlap constraint");
        assert_eq!(overlap["passed"], false);
        assert_eq!(overlap["severity"], "warning");
        assert_eq!(result["conflicts"][0]["registration"], "B-9999");
    }

    #[tokio::test]
    async fn suggest_stand_adjustment_flight_not_found() {
        let svc = advisory_service(vec![], vec![], vec![], vec![], vec![], vec![]);
        let err = svc
            .execute("flight.suggest_stand_adjustment", &json!({"flight_id": "MISSING"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ReadActionError::NotFound(_)));
    }

    #[tokio::test]
    async fn suggest_replan_generates_reassign_proposal_with_scores() {
        let order = order_fixture("ORD1", "FL1", "pending", Some("TEAM_A"));
        let svc = advisory_service(
            vec![],
            vec![],
            vec![],
            vec![order],
            vec![team_fixture("TEAM_A", "Alpha"), team_fixture("TEAM_B", "Bravo")],
            vec![],
        );
        let result = svc
            .execute(
                "dispatch.suggest_replan",
                &json!({"dispatch_order_id": "ORD1", "reason": "team unavailable"}),
            )
            .await
            .expect("replan suggestion");
        let suggestion = &result["suggestion"];
        assert_eq!(suggestion["action_name"], "reassign");
        assert_eq!(suggestion["risk_level"], "high");
        assert_eq!(suggestion["arguments"]["reason"], "team unavailable");
        assert_ne!(suggestion["arguments"]["assignee_id"], "TEAM_A");
        assert_eq!(result["score_before"], 0.5);
        assert!(result["score_after"].as_f64().unwrap() > 0.5);
        assert_eq!(result["resource_changes"][0]["kind"], "team");
        assert_eq!(result["resource_changes"][0]["from"], "TEAM_A");
    }

    #[tokio::test]
    async fn suggest_replan_order_not_found() {
        let svc = advisory_service(vec![], vec![], vec![], vec![], vec![team_fixture("T1", "A")], vec![]);
        let err = svc
            .execute("dispatch.suggest_replan", &json!({"dispatch_order_id": "MISSING", "reason": "x"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ReadActionError::NotFound(_)));
    }

    #[tokio::test]
    async fn suggest_escalation_severity_for_critical_open_anomaly() {
        let anomaly = anomaly_fixture("AN1", "FL1", AnomalySeverity::Critical, AnomalyStatus::Open, 10);
        let svc = advisory_service(vec![], vec![], vec![], vec![], vec![], vec![anomaly]);
        let result = svc
            .execute("anomaly.suggest_escalation", &json!({"anomaly_id": "AN1"}))
            .await
            .expect("escalation suggestion");
        let suggestion = &result["suggestion"];
        assert_eq!(suggestion["action_name"], "escalate");
        assert_eq!(suggestion["after_preview"]["escalation_level"], 1);
        assert_eq!(result["escalation_type"], "severity_escalation");
        assert!(result["targets"]["notification"]["title"].is_string());
    }

    #[tokio::test]
    async fn suggest_escalation_rejects_resolved_anomaly() {
        let anomaly = anomaly_fixture("AN2", "FL1", AnomalySeverity::High, AnomalyStatus::Resolved, 10);
        let svc = advisory_service(vec![], vec![], vec![], vec![], vec![], vec![anomaly]);
        let err = svc
            .execute("anomaly.suggest_escalation", &json!({"anomaly_id": "AN2"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ReadActionError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn suggest_delay_action_lists_impacted_dispatch_orders() {
        let flight = flight_fixture("FL1", FlightStatus::Delayed, true, true);
        let mut impacted = order_fixture("ORD1", "FL1", "pending", Some("TEAM_A"));
        // 计划开始早于新起飞时间（scheduled +30min）→ 需要随之调整。
        impacted.planned_start_time = Some(Utc::now());
        let svc = advisory_service(vec![flight], vec![], vec![], vec![impacted], vec![], vec![]);
        let result = svc
            .execute("flight.suggest_delay_action", &json!({"flight_id": "FL1"}))
            .await
            .expect("delay suggestion");
        let suggestion = &result["suggestion"];
        assert_eq!(suggestion["action_name"], "update_delay");
        assert!(suggestion["arguments"]["new_estimated_departure"].is_string());
        assert_eq!(result["related_dispatch_actions"][0]["dispatch_order_id"], "ORD1");
        assert_eq!(
            result["related_dispatch_actions"][0]["suggested_action"],
            "reschedule_after_new_departure"
        );
    }

    #[tokio::test]
    async fn suggest_broadcast_has_no_side_effects_and_validates_scope() {
        let svc = advisory_service(vec![], vec![], vec![], vec![], vec![], vec![]);
        let result = svc
            .execute(
                "notification.suggest_broadcast",
                &json!({"title": "weather", "body": "snow", "scope": "on_duty_teams"}),
            )
            .await
            .expect("broadcast suggestion");
        let suggestion = &result["suggestion"];
        assert_eq!(suggestion["action_name"], "send");
        assert_eq!(suggestion["object_id"], "broadcast");
        assert_eq!(suggestion["arguments"]["recipients"]["kind"], "team_status");
        assert!(suggestion["before_snapshot"].is_null(), "建议动作不得产生 before 状态");
        assert_eq!(result["side_effects"], "none until approval");

        // 非法 scope / department 缺参必须拒绝。
        let err = svc
            .execute("notification.suggest_broadcast", &json!({"title": "a", "body": "b", "scope": "vip"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ReadActionError::InvalidArguments(_)));
        let err = svc
            .execute(
                "notification.suggest_broadcast",
                &json!({"title": "a", "body": "b", "scope": "department"}),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ReadActionError::InvalidArguments(_)));
    }
}
