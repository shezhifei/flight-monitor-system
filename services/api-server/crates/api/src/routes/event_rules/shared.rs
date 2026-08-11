//! 事件驱动的派工规则路由
//!
//! 提供调整规则和生成规则的 CRUD API。

pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::types::ConcreteEventRuleAdminService;
pub(crate) use actix_web::{HttpRequest, HttpResponse};
pub(crate) use fms_application::repositories::event_rule_repository::{
    AdjustmentRuleRecord, GenerationRuleRecord, ListAdjustmentRulesParams, ListGenerationRulesParams,
};
pub(crate) use fms_application::schemas::dispatch_schemas::*;
pub(crate) use fms_application::services::dispatch_order_adjuster_handler::DispatchOrderAdjusterHandler;
pub(crate) use fms_application::services::domain_event_subscriber_service::DomainEventEnvelope;
pub(crate) use fms_domain::error::DomainError;
pub(crate) use serde_json::{json, Value};
pub(crate) use std::sync::Arc;
pub(crate) fn ok_resp<T: serde::Serialize>(req: &HttpRequest, data: T) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "error": null,
        "request_id": request_id(req),
    }))
}

pub(crate) fn compare_numbers<F>(actual: &Value, expected: &Value, compare: F) -> bool
where
    F: FnOnce(f64, f64) -> bool,
{
    match (actual.as_f64(), expected.as_f64()) {
        (Some(left), Some(right)) => compare(left, right),
        _ => false,
    }
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ListAdjustmentRulesQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub department_id: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ListGenerationRulesQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub department_id: Option<String>,
}

pub(crate) fn request_id(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

pub(crate) fn ok_empty(req: &HttpRequest) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": null,
        "error": null,
        "request_id": request_id(req),
    }))
}

pub(crate) fn err_resp(req: &HttpRequest, message: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(json!({
        "success": false,
        "data": null,
        "error": message,
        "request_id": request_id(req),
    }))
}

pub(crate) fn map_event_rule_admin_error(error: DomainError) -> ApiError {
    match error {
        DomainError::NotFound {
            entity_type: "adjustment_rule",
            id,
        } => ApiError::NotFound(format!("Adjustment rule not found: {}", id)),
        DomainError::NotFound {
            entity_type: "generation_rule",
            id,
        } => ApiError::NotFound(format!("Generation rule not found: {}", id)),
        other => ApiError::from(other),
    }
}

pub(crate) fn record_to_adjustment_response(record: AdjustmentRuleRecord) -> DispatchOrderAdjustmentRuleResponse {
    let adjuster_type = match record.adjuster_type.as_str() {
        "add_crew_slot" => AdjustmentActionType::AddCrewSlot,
        "increase_crew_count" => AdjustmentActionType::IncreaseCrewCount,
        "upgrade_crew_level" => AdjustmentActionType::UpgradeCrewLevel,
        "add_equipment_slot" => AdjustmentActionType::AddEquipmentSlot,
        "increase_equipment_count" => AdjustmentActionType::IncreaseEquipmentCount,
        "extend_duration" => AdjustmentActionType::ExtendDuration,
        "shorten_duration" => AdjustmentActionType::ShortenDuration,
        "advance_publish" => AdjustmentActionType::AdvancePublish,
        "delay_publish" => AdjustmentActionType::DelayPublish,
        "require_driver_for_equipment" => AdjustmentActionType::RequireDriverForEquipment,
        _ => AdjustmentActionType::AddCrewSlot,
    };

    DispatchOrderAdjustmentRuleResponse {
        id: record.id,
        adjuster_type,
        name: record.name,
        description: record.description,
        event_patterns: record.event_patterns,
        priority: record.priority,
        conditions: record.conditions,
        config: record.config,
        is_enabled: record.is_enabled,
        department_id: record.department_id,
        department_name: record.department_name,
        created_at: record.created_at,
        updated_at: record.updated_at,
        created_by: record.created_by,
    }
}

pub(crate) fn record_to_generation_response(record: GenerationRuleRecord) -> EventDrivenGenerationRuleResponse {
    let config: GenerationRuleConfig = serde_json::from_value(record.config.clone()).unwrap_or(GenerationRuleConfig {
        task_type: "".to_string(),
        duration_minutes_from: None,
        fixed_duration_minutes: None,
        crew_requirements: vec![],
        equipment_requirements: vec![],
    });

    EventDrivenGenerationRuleResponse {
        id: record.id,
        generator_type: record.generator_type,
        name: record.name,
        description: record.description,
        event_patterns: record.event_patterns,
        priority: record.priority,
        conditions: record.conditions,
        config,
        is_enabled: record.is_enabled,
        department_id: record.department_id,
        department_name: record.department_name,
        created_at: record.created_at,
        updated_at: record.updated_at,
        created_by: record.created_by,
    }
}

pub(crate) fn get_action_description(action_type: &str, config: &Value) -> String {
    let config = adjustment_action_config(config);
    match action_type {
        "add_crew_slot" => {
            let slot = config.get("slot_code").and_then(|v| v.as_str()).unwrap_or("");
            let count = config.get("required_count").and_then(|v| v.as_i64()).unwrap_or(0);
            format!("添加 {} 个 {}", count, slot)
        }
        "increase_crew_count" => {
            let slot = config.get("slot_code").and_then(|v| v.as_str()).unwrap_or("");
            let delta = config.get("delta").and_then(|v| v.as_i64()).unwrap_or(0);
            format!("增加 {} {} 人", slot, delta)
        }
        "extend_duration" => {
            let mins = config.get("delta_minutes").and_then(|v| v.as_i64()).unwrap_or(0);
            format!("延长 {} 分钟", mins)
        }
        "advance_publish" => {
            let mins = config.get("delta_minutes").and_then(|v| v.as_i64()).unwrap_or(0);
            format!("提前 {} 分钟发布", mins)
        }
        _ => action_type.to_string(),
    }
}

pub(crate) fn adjustment_action_config(config: &Value) -> &Value {
    config.get("config").filter(|value| value.is_object()).unwrap_or(config)
}

pub(crate) fn rule_matches_preview(
    event_type: &str,
    event_patterns: &[String],
    conditions: &Option<Value>,
    payload: &Value,
) -> bool {
    event_patterns.iter().any(|pattern| pattern == event_type) && evaluate_rule_conditions(conditions, payload)
}

pub(crate) fn evaluate_rule_conditions(conditions: &Option<Value>, payload: &Value) -> bool {
    let Some(conditions) = conditions else {
        return true;
    };

    if let Some(operator) = conditions.get("operator").and_then(Value::as_str) {
        let children = conditions
            .get("children")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        return match operator {
            "AND" => children
                .iter()
                .all(|condition| evaluate_single_condition(condition, payload)),
            "OR" => children
                .iter()
                .any(|condition| evaluate_single_condition(condition, payload)),
            _ => true,
        };
    }

    evaluate_single_condition(conditions, payload)
}

pub(crate) fn evaluate_single_condition(condition: &Value, payload: &Value) -> bool {
    let Some(field) = condition.get("field").and_then(Value::as_str) else {
        return true;
    };
    let Some(op) = condition.get("op").and_then(Value::as_str) else {
        return true;
    };
    let Some(expected) = condition.get("value") else {
        return true;
    };
    let Some(actual) = payload
        .get(field)
        .or_else(|| payload.get("data").and_then(|data| data.get(field)))
    else {
        return false;
    };

    match op {
        "eq" => actual == expected,
        "neq" => actual != expected,
        "gt" => compare_numbers(actual, expected, |left, right| left > right),
        "gte" => compare_numbers(actual, expected, |left, right| left >= right),
        "lt" => compare_numbers(actual, expected, |left, right| left < right),
        "lte" => compare_numbers(actual, expected, |left, right| left <= right),
        "in" => expected.as_array().map(|items| items.contains(actual)).unwrap_or(false),
        "nin" => expected.as_array().map(|items| !items.contains(actual)).unwrap_or(true),
        "contains" => actual
            .as_str()
            .zip(expected.as_str())
            .map(|(text, needle)| text.contains(needle))
            .unwrap_or(false),
        _ => true,
    }
}

pub(crate) fn build_generation_order_preview(
    rule: &GenerationRuleRecord,
    payload: &RulePreviewRequest,
) -> Result<Option<Value>, ApiError> {
    let event = DomainEventEnvelope {
        event_id: "preview".to_string(),
        source_change_id: None,
        aggregate_type: "flight".to_string(),
        aggregate_id: resolve_preview_flight_id(payload),
        event_type: payload.event_type.clone(),
        payload: payload.payload.clone(),
        stream_message_id: "preview".to_string(),
    };

    let Some(generated_order) =
        DispatchOrderAdjusterHandler::apply_generation_rule(rule, &event).map_err(ApiError::from)?
    else {
        return Ok(None);
    };

    serde_json::to_value(generated_order.order)
        .map(Some)
        .map_err(|error| ApiError::from(fms_domain::error::DomainError::Internal(error.to_string())))
}

pub(crate) fn resolve_preview_flight_id(payload: &RulePreviewRequest) -> String {
    payload
        .flight_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| preview_payload_text(&payload.payload, "flight_id"))
        .unwrap_or_else(|| "preview-flight".to_string())
}

pub(crate) fn preview_payload_text(payload: &Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .or_else(|| payload.get("data").and_then(|data| data.get(field)))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
