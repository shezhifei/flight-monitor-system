use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::models::flight::Flight;
use fms_domain::models::flight_leg::FlightTypeCode;

use crate::schemas::dispatch_schemas::*;

use super::helpers;
use super::{DispatchService, GeneratedFlightDispatchRequest, PreparedWindowOrder, ReplanExecutionResult, NULL_VALUE};

impl DispatchService {
    pub(super) fn generation_leg_scope_value(value: LegScope) -> &'static str {
        match value {
            LegScope::Inbound => "inbound",
            LegScope::Outbound => "outbound",
            LegScope::None => "none",
        }
    }

    pub(super) fn generation_publish_trigger_mode_value(value: PublishTriggerMode) -> &'static str {
        match value {
            PublishTriggerMode::Time => "time",
            PublishTriggerMode::Event => "event",
            PublishTriggerMode::Either => "either",
            PublishTriggerMode::BothRequired => "both_required",
        }
    }

    pub(super) fn generation_turnaround_constraint_mode_value(value: TurnaroundConstraintMode) -> &'static str {
        match value {
            TurnaroundConstraintMode::SamePerson => "same_person",
            TurnaroundConstraintMode::SoftPreferSamePerson => "soft_prefer_same_person",
            TurnaroundConstraintMode::HandoverRequired => "handover_required",
            TurnaroundConstraintMode::Disabled => "disabled",
        }
    }

    pub(super) fn flight_status_name(flight: Option<&Flight>) -> Option<String> {
        let status = flight.map(|item| item.status)?;
        let value = match status {
            fms_domain::models::value_objects::FlightStatus::Scheduled => "SCHEDULED",
            fms_domain::models::value_objects::FlightStatus::PrevDeparted => "PREV_DEPARTED",
            fms_domain::models::value_objects::FlightStatus::Arrived => "ARRIVED",
            fms_domain::models::value_objects::FlightStatus::CheckInEnd => "CHECK_IN_END",
            fms_domain::models::value_objects::FlightStatus::Boarding => "BOARDING",
            fms_domain::models::value_objects::FlightStatus::BoardingUrge => "BOARDING_URGE",
            fms_domain::models::value_objects::FlightStatus::BoardingEnd => "BOARDING_END",
            fms_domain::models::value_objects::FlightStatus::Departed => "DEPARTED",
            fms_domain::models::value_objects::FlightStatus::NextArrived => "NEXT_ARRIVED",
            fms_domain::models::value_objects::FlightStatus::Cancelled => "CANCELLED",
            fms_domain::models::value_objects::FlightStatus::Delayed => "DELAYED",
        };
        Some(value.to_string())
    }

    pub(super) fn flight_nature_value(value: FlightTypeCode) -> &'static str {
        match value {
            FlightTypeCode::Domestic => "domestic",
            FlightTypeCode::Intl => "intl",
            FlightTypeCode::Region => "region",
        }
    }

    fn resolve_context_datetime(context: &HashMap<String, Value>, field: &str) -> Option<DateTime<Utc>> {
        let value = context.get(field)?;
        if let Some(datetime) = value.as_str() {
            return DateTime::parse_from_rfc3339(datetime)
                .ok()
                .map(|item| item.with_timezone(&Utc));
        }
        None
    }

    fn legacy_resolve_generation_anchor_time(
        context: &HashMap<String, Value>,
        anchor_type: &str,
    ) -> Option<DateTime<Utc>> {
        match anchor_type {
            "actual_arrival" => Self::resolve_context_datetime(context, "actual_arrival")
                .or_else(|| Self::resolve_context_datetime(context, "estimated_arrival"))
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_arrival"))
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_time")),
            "estimated_arrival" => Self::resolve_context_datetime(context, "estimated_arrival")
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_arrival"))
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_time")),
            "scheduled_arrival" => Self::resolve_context_datetime(context, "scheduled_arrival")
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_time")),
            "actual_departure" => Self::resolve_context_datetime(context, "actual_departure")
                .or_else(|| Self::resolve_context_datetime(context, "estimated_departure"))
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_departure"))
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_time")),
            "estimated_departure" => Self::resolve_context_datetime(context, "estimated_departure")
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_departure"))
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_time")),
            "scheduled_departure" => Self::resolve_context_datetime(context, "scheduled_departure")
                .or_else(|| Self::resolve_context_datetime(context, "scheduled_time")),
            "scheduled_time" => Self::resolve_context_datetime(context, "scheduled_time"),
            _ => None,
        }
    }

    fn resolve_planned_completion_time(
        context: &HashMap<String, Value>,
        mode: &str,
        completion_anchor_type: Option<&str>,
        completion_offset_minutes: Option<i32>,
        planned_start_time: DateTime<Utc>,
        duration_minutes: i32,
    ) -> Result<(DateTime<Utc>, Option<DateTime<Utc>>), DomainError> {
        let (planned_end_time, completion_anchor_time) = match mode {
            "start_plus_duration" => (
                planned_start_time + Duration::minutes(i64::from(duration_minutes)),
                None,
            ),
            "completion_anchor_offset" => {
                let anchor_type = completion_anchor_type.ok_or_else(|| {
                    DomainError::BusinessRuleViolation("完成锚点模式缺少 completion_anchor_type".to_string())
                })?;
                let anchor_time = Self::legacy_resolve_generation_anchor_time(context, anchor_type)
                    .ok_or_else(|| DomainError::BusinessRuleViolation(format!("无法解析完成锚点 {anchor_type}")))?;
                let offset = completion_offset_minutes.ok_or_else(|| {
                    DomainError::BusinessRuleViolation("完成锚点模式缺少 completion_offset_minutes".to_string())
                })?;
                (anchor_time + Duration::minutes(i64::from(offset)), Some(anchor_time))
            }
            other => {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "未知预计完成时间模式: {other}"
                )))
            }
        };

        if planned_end_time <= planned_start_time {
            return Err(DomainError::BusinessRuleViolation(format!(
                "预计完成时间 {} 必须晚于预计开始时间 {}",
                planned_end_time.to_rfc3339(),
                planned_start_time.to_rfc3339()
            )));
        }
        Ok((planned_end_time, completion_anchor_time))
    }

    fn legacy_generation_conditions_match(
        context: &HashMap<String, Value>,
        conditions: &HashMap<String, Value>,
    ) -> bool {
        for (key, expected) in conditions {
            if expected.is_null()
                || expected.as_str().is_some_and(|value| value.trim().is_empty())
                || expected.as_array().is_some_and(|items| items.is_empty())
                || expected.as_object().is_some_and(|items| items.is_empty())
            {
                continue;
            }
            let actual = context.get(key);
            if let Some(expected_list) = expected.as_array() {
                let normalized_expected = expected_list
                    .iter()
                    .filter_map(|item| item.as_str().map(|value| value.trim().to_ascii_lowercase()))
                    .filter(|value| !value.is_empty())
                    .collect::<HashSet<_>>();
                let normalized_actual = actual
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                if !normalized_expected.contains(&normalized_actual) {
                    return false;
                }
                continue;
            }
            if let Some(expected_bool) = expected.as_bool() {
                if actual.and_then(Value::as_bool).unwrap_or(false) != expected_bool {
                    return false;
                }
                continue;
            }
            let normalized_expected = match expected {
                Value::String(text) => text.trim().to_ascii_lowercase(),
                other => other.to_string().trim().to_ascii_lowercase(),
            };
            let normalized_actual = actual
                .map(|value| match value {
                    Value::String(text) => text.trim().to_ascii_lowercase(),
                    other => other.to_string().trim().to_ascii_lowercase(),
                })
                .unwrap_or_default();
            if normalized_actual != normalized_expected {
                return false;
            }
        }
        true
    }

    fn build_flight_leg_contexts(
        &self,
        flight: Option<&Flight>,
        flight_id: &str,
        stand_id: &str,
        eta: DateTime<Utc>,
        etd: DateTime<Utc>,
        terminal: Option<&str>,
    ) -> HashMap<String, HashMap<String, Value>> {
        let resolved_terminal = terminal
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| flight.and_then(|item| item.terminal.clone()));
        let mut shared = HashMap::new();
        let aircraft_type = flight.and_then(|item| {
            let value = item.aircraft_type_detail.as_ref()?.0.trim();
            (!value.is_empty()).then(|| value.to_string())
        });
        let registration = flight.and_then(|item| {
            item.registration
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
        let stand_type = flight.and_then(|item| {
            item.inbound_leg
                .as_ref()
                .and_then(|leg| leg.stand_type.clone())
                .or_else(|| item.outbound_leg.as_ref().and_then(|leg| leg.stand_type.clone()))
        });
        let is_turnaround = flight.map(Flight::is_turnaround_flight).unwrap_or(true);
        let arrival_anchor = flight
            .and_then(|item| item.actual_arrival)
            .or_else(|| flight.and_then(|item| item.estimated_arrival))
            .or_else(|| flight.and_then(|item| item.scheduled_arrival))
            .unwrap_or(eta);
        let departure_anchor = flight
            .and_then(|item| item.estimated_departure)
            .or_else(|| flight.and_then(|item| item.scheduled_departure))
            .or_else(|| flight.and_then(|item| item.actual_departure))
            .unwrap_or(etd);
        let delta_t_minutes = ((departure_anchor - arrival_anchor).num_seconds() as f64 / 60.0).round() as i64;

        shared.insert("flight_id".to_string(), json!(flight_id));
        shared.insert("terminal".to_string(), json!(resolved_terminal));
        shared.insert("stand_id".to_string(), json!(stand_id));
        shared.insert("stand_type".to_string(), json!(stand_type));
        shared.insert("stand_area".to_string(), Value::Null);
        shared.insert("aircraft_type".to_string(), json!(aircraft_type));
        shared.insert("registration".to_string(), json!(registration));
        shared.insert("is_turnaround".to_string(), json!(is_turnaround));
        shared.insert(
            "turnaround_pair_key".to_string(),
            json!(if flight.is_none() || is_turnaround {
                Some(flight_id.to_string())
            } else {
                None::<String>
            }),
        );
        shared.insert("arrival_anchor_time".to_string(), json!(arrival_anchor));
        shared.insert("departure_anchor_time".to_string(), json!(departure_anchor));
        shared.insert("delta_t_minutes".to_string(), json!(delta_t_minutes));
        shared.insert("minimum_turnaround_minutes".to_string(), Value::Null);
        shared.insert("slack_minutes".to_string(), Value::Null);
        shared.insert("flight_status".to_string(), json!(Self::flight_status_name(flight)));
        shared.insert(
            "has_boarding_restriction".to_string(),
            json!(flight.is_some_and(|item| item.has_boarding_restriction)),
        );
        shared.insert(
            "is_quick_turnaround".to_string(),
            json!(flight.is_some_and(|item| item.is_quick_turnaround)),
        );
        shared.insert(
            "is_commercial_signed".to_string(),
            json!(flight.is_some_and(|item| item.is_commercial_signed)),
        );

        let mut contexts = HashMap::new();
        if let Some(inbound_leg) = flight.and_then(|item| item.inbound_leg.as_ref()) {
            let mut context = shared.clone();
            context.insert("leg_scope".to_string(), json!("inbound"));
            context.insert(
                "flight_nature".to_string(),
                json!(Self::flight_nature_value(inbound_leg.flight_type)),
            );
            context.insert("is_vip".to_string(), json!(inbound_leg.is_vip));
            context.insert(
                "scheduled_time".to_string(),
                json!(inbound_leg.scheduled_time.unwrap_or(eta)),
            );
            context.insert(
                "scheduled_arrival".to_string(),
                json!(flight.and_then(|item| item.scheduled_arrival).unwrap_or(eta)),
            );
            context.insert(
                "estimated_arrival".to_string(),
                json!(flight.and_then(|item| item.estimated_arrival).unwrap_or(eta)),
            );
            context.insert(
                "actual_arrival".to_string(),
                json!(flight.and_then(|item| item.actual_arrival)),
            );
            context.insert(
                "scheduled_departure".to_string(),
                json!(flight.and_then(|item| item.scheduled_departure).unwrap_or(etd)),
            );
            context.insert(
                "estimated_departure".to_string(),
                json!(flight.and_then(|item| item.estimated_departure).unwrap_or(etd)),
            );
            context.insert(
                "actual_departure".to_string(),
                json!(flight.and_then(|item| item.actual_departure)),
            );
            contexts.insert("inbound".to_string(), context);
        } else if eta != DateTime::<Utc>::MIN_UTC {
            let mut context = shared.clone();
            context.insert("leg_scope".to_string(), json!("inbound"));
            context.insert("flight_nature".to_string(), json!("domestic"));
            context.insert("is_vip".to_string(), json!(false));
            context.insert("scheduled_time".to_string(), json!(eta));
            context.insert("scheduled_arrival".to_string(), json!(eta));
            context.insert("estimated_arrival".to_string(), json!(eta));
            context.insert("actual_arrival".to_string(), Value::Null);
            context.insert("scheduled_departure".to_string(), json!(etd));
            context.insert("estimated_departure".to_string(), json!(etd));
            context.insert("actual_departure".to_string(), Value::Null);
            contexts.insert("inbound".to_string(), context);
        }

        if let Some(outbound_leg) = flight.and_then(|item| item.outbound_leg.as_ref()) {
            let mut context = shared.clone();
            context.insert("leg_scope".to_string(), json!("outbound"));
            context.insert(
                "flight_nature".to_string(),
                json!(Self::flight_nature_value(outbound_leg.flight_type)),
            );
            context.insert("is_vip".to_string(), json!(outbound_leg.is_vip));
            context.insert(
                "scheduled_time".to_string(),
                json!(outbound_leg.scheduled_time.unwrap_or(etd)),
            );
            context.insert(
                "scheduled_arrival".to_string(),
                json!(flight.and_then(|item| item.scheduled_arrival).unwrap_or(eta)),
            );
            context.insert(
                "estimated_arrival".to_string(),
                json!(flight.and_then(|item| item.estimated_arrival).unwrap_or(eta)),
            );
            context.insert(
                "actual_arrival".to_string(),
                json!(flight.and_then(|item| item.actual_arrival)),
            );
            context.insert(
                "scheduled_departure".to_string(),
                json!(flight.and_then(|item| item.scheduled_departure).unwrap_or(etd)),
            );
            context.insert(
                "estimated_departure".to_string(),
                json!(flight.and_then(|item| item.estimated_departure).unwrap_or(etd)),
            );
            context.insert(
                "actual_departure".to_string(),
                json!(flight.and_then(|item| item.actual_departure)),
            );
            contexts.insert("outbound".to_string(), context);
        } else if etd != DateTime::<Utc>::MIN_UTC {
            let mut context = shared;
            context.insert("leg_scope".to_string(), json!("outbound"));
            context.insert("flight_nature".to_string(), json!("domestic"));
            context.insert("is_vip".to_string(), json!(false));
            context.insert("scheduled_time".to_string(), json!(etd));
            context.insert("scheduled_arrival".to_string(), json!(eta));
            context.insert("estimated_arrival".to_string(), json!(eta));
            context.insert("actual_arrival".to_string(), Value::Null);
            context.insert("scheduled_departure".to_string(), json!(etd));
            context.insert("estimated_departure".to_string(), json!(etd));
            context.insert("actual_departure".to_string(), Value::Null);
            contexts.insert("outbound".to_string(), context);
        }

        contexts
    }

    pub(super) async fn build_generation_requests(
        &self,
        flight_id: &str,
        stand_id: &str,
        eta: DateTime<Utc>,
        etd: DateTime<Utc>,
        terminal: Option<&str>,
    ) -> Result<Vec<GeneratedFlightDispatchRequest>, DomainError> {
        let department_repo = &self.rules.department_repo;
        let generation_rule_repo = &self.rules.generation_rule_repo;
        let adjustment_rule_repo = &self.rules.adjustment_rule_repo;
        let task_type_repo = &self.rules.task_type_repo;
        let task_type_requirement_repo = &self.rules.task_type_requirement_repo;

        let flight = if let Some(flight_repo) = self.rules.flight_repo.as_ref() {
            flight_repo.find_by_id(flight_id).await?
        } else {
            None
        };
        let contexts = self.build_flight_leg_contexts(flight.as_ref(), flight_id, stand_id, eta, etd, terminal);

        let mut requests = Vec::new();
        let departments = department_repo.find_all(false, 1000, 0).await?;
        for department in departments {
            let generation_rules = generation_rule_repo
                .list_rules(&department.id, Some("published"))
                .await?;
            if generation_rules.is_empty() {
                continue;
            }
            let adjustment_rules = adjustment_rule_repo
                .list_rules(&department.id, Some("published"))
                .await?;

            for rule in generation_rules {
                let leg_scope = Self::generation_leg_scope_value(rule.leg_scope);
                let Some(context) = contexts.get(leg_scope) else {
                    continue;
                };
                if !Self::legacy_generation_conditions_match(context, &rule.conditions) {
                    continue;
                }

                let requirement_version = task_type_requirement_repo
                    .find_published(&rule.department_id, &rule.task_type)
                    .await?
                    .ok_or_else(|| {
                        DomainError::BusinessRuleViolation(format!(
                            "规则 {} 命中，但作业类型 {} 缺少已发布作业类型规则",
                            rule.id, rule.task_type
                        ))
                    })?;
                let crew_source = if requirement_version.crew_requirements.is_empty() {
                    &requirement_version.requirements
                } else {
                    &requirement_version.crew_requirements
                };
                let crew_requirement_snapshot = Self::serialize_crew_requirement_snapshot(crew_source);
                if crew_requirement_snapshot.is_empty() {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "规则 {} 命中，但作业类型 {} 缺少人员资质要求",
                        rule.id, rule.task_type
                    )));
                }
                let equipment_requirement_snapshot =
                    Self::serialize_equipment_requirement_snapshot(&requirement_version.equipment_requirements);
                if equipment_requirement_snapshot.is_empty() {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "规则 {} 命中，但作业类型 {} 缺少设备类型要求",
                        rule.id, rule.task_type
                    )));
                }
                let anchor_time =
                    Self::legacy_resolve_generation_anchor_time(context, rule.generation_anchor_type.as_str())
                        .ok_or_else(|| {
                            DomainError::BusinessRuleViolation(format!(
                                "规则 {} 命中，但无法解析生成锚点 {}",
                                rule.id, rule.generation_anchor_type
                            ))
                        })?;
                let planned_start_time = anchor_time + Duration::minutes(i64::from(rule.start_offset_minutes));
                let duration_minutes = if rule.completion_time_mode == "start_plus_duration" {
                    if let Some(duration) = rule.duration_minutes {
                        duration
                    } else {
                        task_type_repo
                            .find_by_code(&rule.task_type)
                            .await?
                            .and_then(|item| item.default_duration_minutes)
                            .unwrap_or(15)
                    }
                } else {
                    0
                };
                let (planned_end_time, completion_anchor_time) = Self::resolve_planned_completion_time(
                    context,
                    &rule.completion_time_mode,
                    rule.completion_anchor_type.as_deref(),
                    rule.completion_offset_minutes,
                    planned_start_time,
                    duration_minutes,
                )?;
                let turnaround_constraint_mode = requirement_version
                    .turnaround_continuity_rules
                    .iter()
                    .find(|item| item.enabled)
                    .map(|item| Self::generation_turnaround_constraint_mode_value(item.constraint_mode).to_string());
                let mut request = GeneratedFlightDispatchRequest {
                    task_type: rule.task_type.clone(),
                    stand_id: stand_id.to_string(),
                    terminal: context
                        .get("terminal")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    planned_start_time,
                    planned_end_time,
                    source_type: "generated".to_string(),
                    department_id: rule.department_id.clone(),
                    leg_scope: leg_scope.to_string(),
                    generation_rule_id: rule.id.clone(),
                    generation_rule_version: rule.version_no,
                    generation_anchor_type: rule.generation_anchor_type.clone(),
                    generation_anchor_time: anchor_time,
                    completion_time_mode: rule.completion_time_mode.clone(),
                    completion_anchor_type: rule.completion_anchor_type.clone(),
                    completion_anchor_time,
                    completion_offset_minutes: rule.completion_offset_minutes,
                    completion_warning_lead_minutes: rule.completion_warning_lead_minutes,
                    publish_trigger_mode: Self::generation_publish_trigger_mode_value(rule.publish_trigger_mode)
                        .to_string(),
                    publish_at: rule
                        .publish_offset_minutes
                        .map(|offset| anchor_time + Duration::minutes(offset as i64)),
                    turnaround_pair_key: context
                        .get("turnaround_pair_key")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    turnaround_constraint_mode,
                    department_rule_version: requirement_version.id.clone(),
                    crew_requirement_snapshot,
                    equipment_requirement_snapshot,
                };
                for adjustment_rule in &adjustment_rules {
                    if adjustment_rule.task_type != rule.task_type {
                        continue;
                    }
                    if !Self::legacy_generation_conditions_match(context, &adjustment_rule.conditions) {
                        continue;
                    }
                    Self::apply_generation_adjustments(&mut request, &adjustment_rule.actions);
                }
                if request.planned_end_time <= request.planned_start_time {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "规则 {} 调整后的预计完成时间必须晚于预计开始时间",
                        rule.id
                    )));
                }
                requests.push(request);
            }
        }

        Ok(requests)
    }

    fn apply_generation_adjustments(request: &mut GeneratedFlightDispatchRequest, actions: &[Value]) {
        for action in actions {
            let Some(action_type) = action.get("action_type").and_then(Value::as_str) else {
                continue;
            };
            let slot_code = action.get("slot_code").and_then(Value::as_str).unwrap_or_default();
            match action_type {
                "increase_slot_count" => {
                    for item in &mut request.crew_requirement_snapshot {
                        if item.get("slot_code").and_then(Value::as_str) == Some(slot_code) {
                            let current = item.get("required_count").and_then(Value::as_i64).unwrap_or(1);
                            let delta = action.get("delta").and_then(Value::as_i64).unwrap_or(1);
                            item["required_count"] = json!(current + delta);
                        }
                    }
                }
                "add_slot" => {
                    if let Some(slot) = action.get("slot").cloned() {
                        request.crew_requirement_snapshot.push(slot);
                    }
                }
                "upgrade_min_level" => {
                    for item in &mut request.crew_requirement_snapshot {
                        if item.get("slot_code").and_then(Value::as_str) == Some(slot_code) {
                            item["min_level_code"] = action.get("min_level_code").cloned().unwrap_or(Value::Null);
                        }
                    }
                }
                "extend_duration" => {
                    let delta = action.get("delta_minutes").and_then(Value::as_i64).unwrap_or(0);
                    request.planned_end_time += Duration::minutes(delta);
                }
                "advance_publish_offset" => {
                    if let Some(publish_at) = request.publish_at.as_mut() {
                        let delta = action.get("delta_minutes").and_then(Value::as_i64).unwrap_or(0);
                        *publish_at -= Duration::minutes(delta);
                    }
                }
                "delay_publish_offset" => {
                    if let Some(publish_at) = request.publish_at.as_mut() {
                        let delta = action.get("delta_minutes").and_then(Value::as_i64).unwrap_or(0);
                        *publish_at += Duration::minutes(delta);
                    }
                }
                "increase_equipment_count" => {
                    for item in &mut request.equipment_requirement_snapshot {
                        if item.get("slot_code").and_then(Value::as_str) == Some(slot_code) {
                            let current = item.get("required_count").and_then(Value::as_i64).unwrap_or(1);
                            let delta = action.get("delta").and_then(Value::as_i64).unwrap_or(1);
                            item["required_count"] = json!(current + delta);
                        }
                    }
                }
                "add_equipment_type_requirement" => {
                    if let Some(slot) = action.get("equipment_slot").cloned() {
                        request.equipment_requirement_snapshot.push(slot);
                    }
                }
                "require_driver_for_equipment" => {
                    for item in &mut request.equipment_requirement_snapshot {
                        if item.get("slot_code").and_then(Value::as_str) == Some(slot_code) {
                            item["requires_driver"] = json!(true);
                            if let Some(value) = action.get("driver_qualification_code") {
                                item["driver_qualification_code"] = value.clone();
                            }
                            if let Some(value) = action.get("driver_min_level_code") {
                                item["driver_min_level_code"] = value.clone();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) async fn select_preparation_members(
        &self,
        order: &DispatchOrder,
        department_id: &str,
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        crew_requirement_snapshot: &[Value],
    ) -> Result<(Vec<Value>, Vec<Value>, Option<String>, Option<String>, Option<String>), DomainError> {
        let Some(team_member_repo) = self.resources.team_member_repo.as_ref() else {
            return Ok((
                Vec::new(),
                vec![json!({
                    "reason": "team_member_repo_unavailable",
                })],
                Some("当前无法补齐执行编组".to_string()),
                None,
                None,
            ));
        };
        let Some(qualification_grant_repo) = self.resources.qualification_grant_repo.as_ref() else {
            return Ok((
                Vec::new(),
                vec![json!({
                    "reason": "qualification_grant_repo_unavailable",
                })],
                Some("当前无法补齐执行编组".to_string()),
                None,
                None,
            ));
        };
        let Some(qualification_repo) = self.resources.qualification_repo.as_ref() else {
            return Ok((
                Vec::new(),
                vec![json!({
                    "reason": "qualification_repo_unavailable",
                })],
                Some("当前无法补齐执行编组".to_string()),
                None,
                None,
            ));
        };

        let active_user_ids = team_member_repo.list_active_users().await?;
        if active_user_ids.is_empty() {
            return Ok((
                Vec::new(),
                vec![json!({
                    "reason": "no_qualified_grants",
                })],
                Some("当前无法补齐执行编组".to_string()),
                None,
                None,
            ));
        }

        let grants = qualification_grant_repo
            .find_by_department(department_id, Some(planned_start_time), &active_user_ids, false)
            .await?;
        if grants.is_empty() {
            return Ok((
                Vec::new(),
                vec![json!({
                    "reason": "no_qualified_grants",
                })],
                Some("当前无法补齐执行编组".to_string()),
                None,
                None,
            ));
        }

        let levels = qualification_repo.list_levels(department_id, None, false).await?;
        let level_index = levels
            .into_iter()
            .map(|level| {
                let mut covered = level.covered_level_codes.into_iter().collect::<HashSet<_>>();
                covered.insert(level.level_code.clone());
                (level.level_code, covered)
            })
            .collect::<HashMap<_, _>>();

        let overlapping_orders = self
            .order
            .order_repo
            .find_orders_in_window(
                planned_start_time,
                planned_end_time,
                &Self::ACTIVE_CONFLICT_STATUSES,
                None,
                None,
                order.terminal.as_deref(),
                false,
            )
            .await?;
        let busy_user_ids = overlapping_orders
            .into_iter()
            .filter(|candidate| candidate.id != order.id)
            .flat_map(|candidate| Self::order_member_user_ids(&candidate))
            .collect::<HashSet<_>>();

        let mut team_member_cache = HashMap::<String, Vec<TeamMember>>::new();
        let mut grants_by_user = HashMap::<String, Vec<fms_domain::models::dispatch::QualificationGrant>>::new();
        for grant in grants {
            if busy_user_ids.contains(grant.user_id.as_str()) {
                continue;
            }
            grants_by_user.entry(grant.user_id.clone()).or_default().push(grant);
        }

        let mut selected_user_ids = HashSet::<String>::new();
        let mut selected_members = Vec::<Value>::new();
        let mut qualification_gap = Vec::<Value>::new();
        let mut selected_team_ids = Vec::<String>::new();

        for requirement in crew_requirement_snapshot {
            let Some(requirement_obj) = requirement.as_object() else {
                continue;
            };
            let slot_code = requirement_obj
                .get("slot_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or_default()
                .to_string();
            let qualification_code = requirement_obj
                .get("qualification_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or_default()
                .to_string();
            let min_level_code = requirement_obj
                .get("min_level_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let required_count = requirement_obj
                .get("required_count")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .max(1) as usize;

            let mut slot_assignments = Vec::<Value>::new();
            let mut candidate_rows = grants_by_user
                .iter()
                .filter(|(user_id, _)| !selected_user_ids.contains(*user_id))
                .filter_map(|(user_id, user_grants)| {
                    let matched_grant = user_grants.iter().find(|grant| {
                        grant.qualification_code == qualification_code
                            && Self::level_covers_requirement(
                                &level_index,
                                &grant.level_code,
                                min_level_code.as_deref(),
                            )
                    })?;
                    Some((
                        user_id.clone(),
                        matched_grant.level_code.clone(),
                        matched_grant.source_team_id.clone(),
                    ))
                })
                .collect::<Vec<_>>();
            candidate_rows.sort_by(|left, right| left.0.cmp(&right.0));

            for (user_id, level_code, source_team_id) in candidate_rows.into_iter().take(required_count) {
                selected_user_ids.insert(user_id.clone());
                if let Some(source_team_id) = source_team_id.as_deref() {
                    selected_team_ids.push(source_team_id.to_string());
                }
                let memberships = if let Some(existing) = team_member_cache.get(&user_id) {
                    existing.clone()
                } else {
                    let loaded = team_member_repo.find_by_user(&user_id).await?;
                    team_member_cache.insert(user_id.clone(), loaded.clone());
                    loaded
                };
                let matched_membership = memberships
                    .iter()
                    .find(|member| {
                        source_team_id
                            .as_deref()
                            .map(|team_id| member.team_id == team_id)
                            .unwrap_or(true)
                    })
                    .cloned();
                let source_team_name = if let (Some(team_repo), Some(team_id)) =
                    (self.resources.team_repo.as_ref(), source_team_id.as_deref())
                {
                    team_repo.find_by_id(team_id, false).await?.map(|team| team.name)
                } else {
                    None
                };
                slot_assignments.push(json!({
                    "user_id": user_id,
                    "username": matched_membership.as_ref().and_then(|member| member.username.clone()),
                    "source_team_id": source_team_id,
                    "source_team_name": source_team_name,
                    "slot_code": slot_code,
                    "qualification_code": qualification_code,
                    "qualification_level_code": level_code,
                }));
            }

            let assigned_count = slot_assignments.len();
            selected_members.extend(slot_assignments);
            if assigned_count < required_count {
                qualification_gap.push(json!({
                    "slot_code": slot_code,
                    "qualification_code": qualification_code,
                    "min_level_code": min_level_code,
                    "required_count": required_count,
                    "assigned_count": assigned_count,
                    "missing_count": required_count - assigned_count,
                    "reason": "qualification_crew_unavailable",
                }));
            }
        }

        if !qualification_gap.is_empty() {
            return Ok((
                Vec::new(),
                qualification_gap,
                Some("当前无法补齐执行编组".to_string()),
                None,
                None,
            ));
        }

        let mut dominant_team_id = None;
        let mut dominant_team_name = None;
        if !selected_team_ids.is_empty() {
            let mut counts = HashMap::<String, usize>::new();
            for team_id in selected_team_ids {
                *counts.entry(team_id).or_insert(0) += 1;
            }
            if let Some((team_id, _)) = counts
                .into_iter()
                .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
            {
                dominant_team_name = if let Some(team_repo) = self.resources.team_repo.as_ref() {
                    team_repo.find_by_id(&team_id, false).await?.map(|team| team.name)
                } else {
                    None
                };
                dominant_team_id = Some(team_id);
            }
        }

        Ok((
            selected_members,
            Vec::new(),
            Some("人员编组满足已发布资质组合规则".to_string()),
            dominant_team_id,
            dominant_team_name,
        ))
    }

    pub(super) async fn assign_equipment_for_publication(
        &self,
        order: &DispatchOrder,
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        stand_position: Option<(f64, f64)>,
        equipment_requirement_snapshot: &[Value],
        task_crew_members: &[Value],
    ) -> Result<(Vec<Value>, Vec<Value>), DomainError> {
        let Some(equipment_repo) = self.resources.equipment_repo.as_ref() else {
            if equipment_requirement_snapshot.is_empty() {
                return Ok((Vec::new(), Vec::new()));
            }
            return Ok((
                Vec::new(),
                Self::build_equipment_gap_from_snapshot(equipment_requirement_snapshot, "equipment_repo_unavailable"),
            ));
        };

        let overlapping_orders = self
            .order
            .order_repo
            .find_orders_in_window(
                planned_start_time,
                planned_end_time,
                &Self::ACTIVE_CONFLICT_STATUSES,
                None,
                None,
                order.terminal.as_deref(),
                false,
            )
            .await?;
        let busy_equipment_ids = overlapping_orders
            .into_iter()
            .filter(|candidate| candidate.id != order.id)
            .flat_map(|candidate| {
                candidate
                    .equipment_assignment
                    .into_iter()
                    .filter_map(|item| {
                        item.get("equipment_id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<HashSet<_>>();

        let driver_user_id = task_crew_members
            .iter()
            .filter_map(|item| item.get("user_id").and_then(Value::as_str))
            .map(str::trim)
            .find(|value| !value.is_empty())
            .map(str::to_string);

        let mut selected_equipment_ids = HashSet::<String>::new();
        let mut equipment_assignment = Vec::<Value>::new();
        let mut equipment_gap = Vec::<Value>::new();

        for requirement in equipment_requirement_snapshot {
            let Some(requirement_obj) = requirement.as_object() else {
                continue;
            };
            let slot_code = requirement_obj
                .get("slot_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or_default()
                .to_string();
            let equipment_type_id = requirement_obj
                .get("equipment_type_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let equipment_type_code = requirement_obj
                .get("equipment_type_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let required_count = requirement_obj
                .get("required_count")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .max(1) as usize;
            let requires_driver = requirement_obj
                .get("requires_driver")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let mut candidates = equipment_repo
                .find_available_for_dispatch(equipment_type_id.as_deref(), order.terminal.as_deref())
                .await?
                .into_iter()
                .filter(|equipment| !busy_equipment_ids.contains(equipment.id.as_str()))
                .filter(|equipment| !selected_equipment_ids.contains(equipment.id.as_str()))
                .collect::<Vec<_>>();
            candidates.sort_by(|left, right| {
                helpers::equipment_distance_sort_key(left, stand_position)
                    .partial_cmp(&helpers::equipment_distance_sort_key(right, stand_position))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.code.cmp(&right.code))
            });

            let mut assigned_count = 0usize;
            for equipment in candidates.into_iter().take(required_count) {
                selected_equipment_ids.insert(equipment.id.clone());
                assigned_count += 1;
                equipment_assignment.push(json!({
                    "slot_code": slot_code,
                    "equipment_id": equipment.id,
                    "equipment_code": equipment.code,
                    "driver_user_id": if requires_driver { driver_user_id.clone() } else { None::<String> },
                }));
            }

            if assigned_count < required_count {
                equipment_gap.push(json!({
                    "slot_code": slot_code,
                    "equipment_type_id": equipment_type_id,
                    "equipment_type_code": equipment_type_code,
                    "required_count": required_count,
                    "assigned_count": assigned_count,
                    "missing_count": required_count - assigned_count,
                    "reason": "equipment_unassigned",
                }));
            }
        }

        Ok((equipment_assignment, equipment_gap))
    }

    pub(super) async fn prepare_window_candidate_order(
        &self,
        order: &DispatchOrder,
        terminal: Option<&str>,
        fallback_start: DateTime<Utc>,
    ) -> Result<PreparedWindowOrder, DomainError> {
        let Some(stand_repo) = self.resources.stand_repo.as_ref() else {
            return Err(DomainError::ValidationError(
                "机位仓储未配置，无法执行窗口优化".to_string(),
            ));
        };
        let Some(team_member_repo) = self.resources.team_member_repo.as_ref() else {
            return Err(DomainError::ValidationError(
                "班组成员仓储未配置，无法执行窗口优化".to_string(),
            ));
        };
        let Some(qualification_grant_repo) = self.resources.qualification_grant_repo.as_ref() else {
            return Err(DomainError::ValidationError(
                "资质授权仓储未配置，无法执行窗口优化".to_string(),
            ));
        };
        let Some(qualification_repo) = self.resources.qualification_repo.as_ref() else {
            return Err(DomainError::ValidationError(
                "资质等级仓储未配置，无法执行窗口优化".to_string(),
            ));
        };
        let Some(resource_availability_service) = self.resources.resource_availability_service.as_ref() else {
            return Err(DomainError::ValidationError(
                "资源可用性服务未配置，无法执行窗口优化".to_string(),
            ));
        };

        let stand_id = order
            .stand_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| DomainError::ValidationError(format!("工单 {} 缺少机位信息", order.id)))?;
        let stand = stand_repo
            .find_by_id(stand_id)
            .await?
            .ok_or_else(|| DomainError::ValidationError(format!("工单 {} 缺少机位信息", order.id)))?;
        let stand_position = (stand.position_lat, stand.position_lng);
        let (planned_start_time, planned_end_time) = Self::window_task_interval(order, fallback_start);
        let resolved_terminal = terminal
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                order
                    .terminal
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            });

        let department_id = self.resolve_order_department_id(order).await?.ok_or_else(|| {
            DomainError::ValidationError(format!("工单 {} 缺少科室规则上下文，无法执行窗口优化", order.id))
        })?;
        let (crew_requirement_snapshot, equipment_requirement_snapshot, department_rule_version) =
            self.resolve_order_requirement_snapshots(order, &department_id).await?;
        if crew_requirement_snapshot.is_empty() {
            return Err(DomainError::ValidationError(format!(
                "工单 {} 缺少已发布资质规则，无法执行窗口优化",
                order.id
            )));
        }

        let active_user_ids = team_member_repo.list_active_users().await?;
        if active_user_ids.is_empty() {
            return Err(DomainError::ValidationError(format!(
                "工单 {} 当前无在岗可用人员",
                order.id
            )));
        }

        let grants = qualification_grant_repo
            .find_by_department(&department_id, Some(planned_start_time), &active_user_ids, false)
            .await?;
        if grants.is_empty() {
            return Err(DomainError::ValidationError(format!(
                "工单 {} 当前无持证人员可用于窗口优化",
                order.id
            )));
        }

        let levels = qualification_repo.list_levels(&department_id, None, false).await?;
        let level_index = levels
            .into_iter()
            .map(|level| {
                let mut covered = level.covered_level_codes.into_iter().collect::<HashSet<_>>();
                covered.insert(level.level_code.clone());
                (level.level_code, covered)
            })
            .collect::<HashMap<_, _>>();

        let candidate_user_ids = grants
            .iter()
            .map(|grant| grant.user_id.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if candidate_user_ids.is_empty() {
            return Err(DomainError::ValidationError(format!(
                "工单 {} 当前无持证人员可用于窗口优化",
                order.id
            )));
        }

        let availability_items = resource_availability_service
            .list_employee_availability(
                &candidate_user_ids,
                planned_start_time,
                planned_end_time,
                resolved_terminal.as_deref(),
            )
            .await?;
        let availability_by_user = availability_items
            .into_iter()
            .filter(|item| item.available)
            .map(|item| (item.resource_id.clone(), item))
            .collect::<HashMap<_, _>>();
        if availability_by_user.is_empty() {
            return Err(DomainError::ValidationError(format!(
                "工单 {} 当前无在岗且可用的持证人员",
                order.id
            )));
        }

        let mut grants_by_user = HashMap::<String, Vec<_>>::new();
        for grant in grants {
            if availability_by_user.contains_key(grant.user_id.as_str()) {
                grants_by_user.entry(grant.user_id.clone()).or_default().push(grant);
            }
        }

        let mut available_candidates = Vec::new();
        for user_id in candidate_user_ids {
            let Some(availability) = availability_by_user.get(&user_id) else {
                continue;
            };
            let user_grants = grants_by_user.get(&user_id).cloned().unwrap_or_default();
            if user_grants.is_empty() {
                continue;
            }
            let memberships = team_member_repo.find_by_user(&user_id).await?;
            let fallback_membership = memberships.iter().find(|member| member.is_active).cloned();
            let fallback_team_id = fallback_membership
                .as_ref()
                .map(|member| member.team_id.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let fallback_team_name = if let (Some(team_repo), Some(team_id)) =
                (self.resources.team_repo.as_ref(), fallback_team_id.as_deref())
            {
                team_repo.find_by_id(team_id, false).await?.map(|team| team.name)
            } else {
                None
            };
            let username = fallback_membership.as_ref().and_then(|member| member.username.clone());
            let mut qualifications = Vec::new();
            for grant in user_grants {
                let source_team_id = grant
                    .source_team_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| fallback_team_id.clone());
                let source_team_name = if let (Some(team_repo), Some(team_id)) =
                    (self.resources.team_repo.as_ref(), source_team_id.as_deref())
                {
                    team_repo
                        .find_by_id(team_id, false)
                        .await?
                        .map(|team| team.name)
                        .or_else(|| fallback_team_name.clone())
                } else {
                    fallback_team_name.clone()
                };
                qualifications.push((
                    grant.qualification_code,
                    Some(grant.level_code),
                    source_team_id,
                    source_team_name,
                ));
            }
            available_candidates.push(super::WindowOptimizationCandidate {
                user_id,
                username,
                source_team_id: fallback_team_id,
                source_team_name: fallback_team_name,
                schedule_source: availability.schedule_source,
                qualifications,
            });
        }

        if available_candidates.is_empty() {
            return Err(DomainError::ValidationError(format!(
                "工单 {} 当前无在岗且可用的持证人员",
                order.id
            )));
        }

        available_candidates.sort_by(|left, right| left.user_id.cmp(&right.user_id));

        Ok(PreparedWindowOrder {
            order: order.clone(),
            stand_position,
            department_rule_version,
            crew_requirement_snapshot,
            equipment_requirement_snapshot,
            level_index,
            baseline_by_slot: Self::extract_order_baseline_members(order),
            available_candidates,
        })
    }

    pub(super) async fn assign_window_task(
        &self,
        prepared: &PreparedWindowOrder,
        bookings: &HashMap<String, Vec<(DateTime<Utc>, DateTime<Utc>)>>,
        fallback_start: DateTime<Utc>,
    ) -> Result<Option<(Value, Vec<String>, ScheduleSource, f64, f64)>, DomainError> {
        let (planned_start_time, planned_end_time) = Self::window_task_interval(&prepared.order, fallback_start);
        let mut selected_members = Vec::<Value>::new();
        let mut selected_user_ids = HashSet::<String>::new();
        let mut selected_schedule_sources = Vec::<ScheduleSource>::new();

        for requirement in &prepared.crew_requirement_snapshot {
            let Some(requirement_obj) = requirement.as_object() else {
                continue;
            };
            let base_slot_code = requirement_obj
                .get("slot_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("slot")
                .to_string();
            let qualification_code = requirement_obj
                .get("qualification_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or_default()
                .to_string();
            let min_level_code = requirement_obj
                .get("min_level_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let required_count = requirement_obj
                .get("required_count")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .max(1) as usize;
            let baseline_candidates = prepared
                .baseline_by_slot
                .get(&base_slot_code)
                .cloned()
                .unwrap_or_default();

            for slot_index in 0..required_count {
                let expanded_slot_code = if required_count == 1 {
                    base_slot_code.clone()
                } else {
                    format!("{base_slot_code}#{}", slot_index + 1)
                };
                let baseline_user_id = baseline_candidates.get(slot_index).cloned();
                let mut candidates = prepared
                    .available_candidates
                    .iter()
                    .filter_map(|candidate| {
                        if selected_user_ids.contains(candidate.user_id.as_str())
                            || Self::users_overlap_window(
                                bookings,
                                &candidate.user_id,
                                planned_start_time,
                                planned_end_time,
                            )
                        {
                            return None;
                        }
                        let matched_qualification = candidate.qualifications.iter().find(
                            |(candidate_qualification_code, candidate_level_code, _, _)| {
                                candidate_qualification_code == &qualification_code
                                    && candidate_level_code
                                        .as_deref()
                                        .map(|level_code| {
                                            Self::level_covers_requirement(
                                                &prepared.level_index,
                                                level_code,
                                                min_level_code.as_deref(),
                                            )
                                        })
                                        .unwrap_or(min_level_code.is_none())
                            },
                        )?;
                        Some((candidate, matched_qualification))
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by(|(left_candidate, _), (right_candidate, _)| {
                    let left_baseline = baseline_user_id
                        .as_deref()
                        .map(|value| value == left_candidate.user_id)
                        .unwrap_or(false);
                    let right_baseline = baseline_user_id
                        .as_deref()
                        .map(|value| value == right_candidate.user_id)
                        .unwrap_or(false);
                    right_baseline
                        .cmp(&left_baseline)
                        .then_with(|| left_candidate.user_id.cmp(&right_candidate.user_id))
                });
                let Some((candidate, matched_qualification)) = candidates.into_iter().next() else {
                    return Ok(None);
                };
                let (_, qualification_level_code, qualification_source_team_id, qualification_source_team_name) =
                    matched_qualification.clone();
                selected_user_ids.insert(candidate.user_id.clone());
                selected_schedule_sources.push(candidate.schedule_source);
                selected_members.push(json!({
                    "user_id": candidate.user_id,
                    "username": candidate.username,
                    "source_team_id": qualification_source_team_id.clone().or_else(|| candidate.source_team_id.clone()),
                    "source_team_name": qualification_source_team_name.clone().or_else(|| candidate.source_team_name.clone()),
                    "slot_code": expanded_slot_code,
                    "qualification_code": qualification_code,
                    "qualification_level_code": qualification_level_code,
                }));
            }
        }

        if selected_members.is_empty() {
            return Ok(None);
        }

        let (team_id, team_name) = Self::resolve_window_assignment_team(&selected_members);
        let assignee_type = if selected_members.len() == 1 {
            "individual"
        } else {
            "team"
        };
        let individual_user_id = (assignee_type == "individual")
            .then(|| {
                selected_members
                    .first()
                    .and_then(|item| item.get("user_id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .flatten();
        let individual_username = (assignee_type == "individual")
            .then(|| {
                selected_members
                    .first()
                    .and_then(|item| item.get("username"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .flatten();
        let source_team_ids = selected_members
            .iter()
            .filter_map(|member| member.get("source_team_id").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let source_team_names = selected_members
            .iter()
            .filter_map(|member| member.get("source_team_name").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let (equipment_assignment, equipment_gap) = self
            .assign_equipment_for_publication(
                &prepared.order,
                planned_start_time,
                planned_end_time,
                Some(prepared.stand_position),
                &prepared.equipment_requirement_snapshot,
                &selected_members,
            )
            .await?;

        let task_crew = serde_json::Value::Object(serde_json::Map::from_iter(vec![
            ("members".to_string(), json!(selected_members.clone())),
            ("source_team_ids".to_string(), json!(source_team_ids)),
            ("source_team_names".to_string(), json!(source_team_names)),
            ("generated_from".to_string(), json!("window_optimization")),
        ]));

        let assignment = json!({
            "assignee_type": assignee_type,
            "team_id": team_id,
            "team_name": team_name,
            "individual_user_id": individual_user_id,
            "individual_username": individual_username,
            "task_crew": task_crew,
            "equipment_assignment": equipment_assignment,
            "equipment_gap": equipment_gap,
            "department_rule_version": prepared.department_rule_version,
            "crew_requirement_snapshot": prepared.crew_requirement_snapshot,
            "equipment_requirement_snapshot": prepared.equipment_requirement_snapshot,
            "score_breakdown": {},
        });

        let assigned_user_ids: Vec<String> = selected_user_ids.into_iter().collect();
        let travel_time = 0.0f64;
        let total_distance_meters = 0.0f64;

        Ok(Some((
            assignment,
            assigned_user_ids,
            selected_schedule_sources
                .into_iter()
                .next()
                .unwrap_or(ScheduleSource::CurrentStatusFallback),
            travel_time,
            total_distance_meters,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn scheduled_context() -> HashMap<String, Value> {
        HashMap::from([("scheduled_time".to_string(), json!("2026-08-08T10:00:00Z"))])
    }

    #[test]
    fn scheduled_time_is_an_explicit_generation_anchor() {
        let resolved = DispatchService::legacy_resolve_generation_anchor_time(&scheduled_context(), "scheduled_time");

        assert_eq!(resolved, Some(Utc.with_ymd_and_hms(2026, 8, 8, 10, 0, 0).unwrap()));
    }

    #[test]
    fn unknown_generation_anchor_does_not_fall_back_to_scheduled_time() {
        let resolved = DispatchService::legacy_resolve_generation_anchor_time(&scheduled_context(), "estimated_time");

        assert!(resolved.is_none());
    }

    #[test]
    fn planned_completion_can_be_start_plus_duration() {
        let start = Utc.with_ymd_and_hms(2026, 8, 8, 10, 0, 0).unwrap();
        let (end, completion_anchor) = DispatchService::resolve_planned_completion_time(
            &scheduled_context(),
            "start_plus_duration",
            None,
            None,
            start,
            25,
        )
        .expect("duration mode");

        assert_eq!(end, Utc.with_ymd_and_hms(2026, 8, 8, 10, 25, 0).unwrap());
        assert_eq!(completion_anchor, None);
    }

    #[test]
    fn planned_completion_can_be_anchor_plus_negative_offset() {
        let context = HashMap::from([
            ("scheduled_time".to_string(), json!("2026-08-08T10:00:00Z")),
            ("estimated_departure".to_string(), json!("2026-08-08T11:00:00Z")),
        ]);
        let start = Utc.with_ymd_and_hms(2026, 8, 8, 10, 0, 0).unwrap();
        let (end, completion_anchor) = DispatchService::resolve_planned_completion_time(
            &context,
            "completion_anchor_offset",
            Some("estimated_departure"),
            Some(-10),
            start,
            0,
        )
        .expect("completion anchor mode");

        assert_eq!(end, Utc.with_ymd_and_hms(2026, 8, 8, 10, 50, 0).unwrap());
        assert_eq!(
            completion_anchor,
            Some(Utc.with_ymd_and_hms(2026, 8, 8, 11, 0, 0).unwrap())
        );
    }

    #[test]
    fn planned_completion_rejects_unresolvable_anchor() {
        let start = Utc.with_ymd_and_hms(2026, 8, 8, 10, 0, 0).unwrap();
        let result = DispatchService::resolve_planned_completion_time(
            &scheduled_context(),
            "completion_anchor_offset",
            Some("unknown"),
            Some(10),
            start,
            0,
        );

        assert!(matches!(result, Err(DomainError::BusinessRuleViolation(message)) if message.contains("无法解析")));
    }

    #[test]
    fn planned_completion_must_be_later_than_start() {
        let start = Utc.with_ymd_and_hms(2026, 8, 8, 10, 0, 0).unwrap();
        let result = DispatchService::resolve_planned_completion_time(
            &scheduled_context(),
            "start_plus_duration",
            None,
            None,
            start,
            0,
        );

        assert!(matches!(result, Err(DomainError::BusinessRuleViolation(message)) if message.contains("必须晚于")));
    }
}
