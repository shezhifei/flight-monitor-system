use actix_web::{error::JsonPayloadError, HttpRequest, HttpResponse};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::error::{default_json_payload_error_handler, ApiError};
use crate::middleware::jwt::JwtAuth;
use crate::services::runtime_error_monitor::record_service_unavailable_background;
use fms_application::schemas::dispatch_schemas::WorkflowDispatchCreateRequest;
use fms_application::schemas::response::ApiErrorResponse;
use fms_application::services::auth_service::AuthService;
use fms_application::services::authorization_service::{AuthorizationService, PermissionCatalog};

#[derive(Debug, Deserialize)]
pub(crate) struct PublicWorkflowDispatchTriggerRequest {
    pub(crate) process_instance_id: String,
    pub(crate) process_task_id: String,
    pub(crate) process_definition_key: Option<String>,
    pub(crate) business_key: Option<String>,
    pub(crate) flight_id: String,
    pub(crate) task_type: String,
    pub(crate) stand_id: Option<String>,
    pub(crate) planned_start_time: Option<DateTime<Utc>>,
    pub(crate) planned_end_time: Option<DateTime<Utc>>,
    pub(crate) target_department: String,
    #[serde(default = "default_supervisor_title")]
    pub(crate) target_job_title: Option<String>,
    #[serde(default = "default_required_people")]
    pub(crate) required_people: i32,
    #[serde(default = "default_priority")]
    pub(crate) priority: String,
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) context: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PageQuery {
    pub(crate) page: Option<i64>,
    pub(crate) page_size: Option<i64>,
}

impl From<PublicWorkflowDispatchTriggerRequest> for WorkflowDispatchCreateRequest {
    fn from(value: PublicWorkflowDispatchTriggerRequest) -> Self {
        Self {
            process_instance_id: value.process_instance_id,
            process_task_id: value.process_task_id,
            process_definition_key: value.process_definition_key,
            business_key: value.business_key,
            flight_id: value.flight_id,
            task_type: value.task_type,
            stand_id: value.stand_id,
            planned_start_time: value.planned_start_time,
            planned_end_time: value.planned_end_time,
            assignment_deadline: None,
            target_department: value.target_department,
            target_job_title: value.target_job_title,
            required_people: value.required_people,
            priority: value.priority,
            description: value.description,
            context: value.context,
        }
    }
}

pub(crate) fn default_supervisor_title() -> Option<String> {
    Some("主管".to_string())
}

pub(crate) fn default_required_people() -> i32 {
    1
}

pub(crate) fn default_priority() -> String {
    "normal".to_string()
}

pub(crate) fn workflow_dispatch_json_error_handler(err: JsonPayloadError, req: &HttpRequest) -> actix_web::Error {
    default_json_payload_error_handler(err, req)
}

pub(crate) fn validation_error_response(detail: Vec<Value>) -> HttpResponse {
    HttpResponse::UnprocessableEntity().json(ApiErrorResponse::with_details(
        actix_web::http::StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
        "输入验证失败",
        serde_json::Value::Array(detail),
    ))
}

pub(crate) fn missing_field_detail(field: &str, input: &Value) -> Value {
    json!({
        "type": "missing",
        "loc": ["body", field],
        "msg": "Field required",
        "input": input.clone(),
    })
}

pub(crate) fn list_type_detail(field: &str, input: &Value) -> Value {
    json!({
        "type": "list_type",
        "loc": ["body", field],
        "msg": "Input should be a valid list",
        "input": input.clone(),
    })
}

pub(crate) fn string_type_detail(field: &str, input: &Value) -> Value {
    json!({
        "type": "string_type",
        "loc": ["body", field],
        "msg": "Input should be a valid string",
        "input": input.clone(),
    })
}

pub(crate) fn string_type_detail_with_index(field: &str, index: usize, input: &Value) -> Value {
    json!({
        "type": "string_type",
        "loc": ["body", field, index],
        "msg": "Input should be a valid string",
        "input": input.clone(),
    })
}

pub(crate) fn int_type_detail(field: &str, input: &Value) -> Value {
    json!({
        "type": "int_type",
        "loc": ["body", field],
        "msg": "Input should be a valid integer",
        "input": input.clone(),
    })
}

pub(crate) fn int_parsing_detail(field: &str, input: &Value) -> Value {
    json!({
        "type": "int_parsing",
        "loc": ["body", field],
        "msg": "Input should be a valid integer, unable to parse string as an integer",
        "input": input.clone(),
    })
}

pub(crate) fn datetime_type_detail(field: &str, input: &Value) -> Value {
    json!({
        "type": "datetime_type",
        "loc": ["body", field],
        "msg": "Input should be a valid datetime",
        "input": input.clone(),
    })
}

pub(crate) fn datetime_from_date_parsing_detail(field: &str, input: &Value, error: &str) -> Value {
    json!({
        "type": "datetime_from_date_parsing",
        "loc": ["body", field],
        "msg": format!("Input should be a valid datetime or date, {error}"),
        "input": input.clone(),
        "ctx": {
            "error": error,
        }
    })
}

pub(crate) fn dict_type_detail(field: &str, input: &Value) -> Value {
    json!({
        "type": "dict_type",
        "loc": ["body", field],
        "msg": "Input should be a valid dictionary",
        "input": input.clone(),
    })
}

pub(crate) fn model_type_detail(input: &Value) -> Value {
    json!({
        "type": "model_attributes_type",
        "loc": ["body"],
        "msg": "Input should be a valid dictionary or object to extract fields from",
        "input": input.clone(),
    })
}

pub(crate) fn serde_payload_detail(error: &serde_json::Error, input: &Value) -> Value {
    json!({
        "type": "json_invalid",
        "loc": ["body"],
        "msg": error.to_string(),
        "input": input.clone(),
    })
}

pub(crate) fn validate_trigger_payload_shape(payload: &Value) -> Vec<Value> {
    let mut detail = Vec::new();
    let Some(object) = payload.as_object() else {
        detail.push(model_type_detail(payload));
        return detail;
    };

    for field in [
        "process_instance_id",
        "process_task_id",
        "flight_id",
        "task_type",
        "target_department",
    ] {
        if !object.contains_key(field) {
            detail.push(missing_field_detail(field, payload));
        }
    }

    for field in [
        "process_instance_id",
        "process_task_id",
        "flight_id",
        "task_type",
        "target_department",
    ] {
        if let Some(value) = object.get(field) {
            if !value.is_string() {
                detail.push(string_type_detail(field, value));
            }
        }
    }

    if let Some(required_people) = object.get("required_people") {
        if required_people.as_i64().is_none() {
            if required_people.is_string() {
                detail.push(int_parsing_detail("required_people", required_people));
            } else {
                detail.push(int_type_detail("required_people", required_people));
            }
        }
    }

    for field in [
        "target_job_title",
        "priority",
        "description",
        "business_key",
        "process_definition_key",
        "stand_id",
    ] {
        if let Some(value) = object.get(field) {
            if !value.is_null() && !value.is_string() {
                detail.push(string_type_detail(field, value));
            }
        }
    }

    for field in ["planned_start_time", "planned_end_time"] {
        if let Some(value) = object.get(field) {
            if value.is_null() || value.is_number() {
                continue;
            }
            let Some(text) = value.as_str() else {
                detail.push(datetime_type_detail(field, value));
                continue;
            };

            if DateTime::parse_from_rfc3339(text).is_ok() || NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok() {
                continue;
            }

            let parsing_error = if text.len() < 10 {
                "input is too short"
            } else {
                "invalid date format"
            };
            detail.push(datetime_from_date_parsing_detail(field, value, parsing_error));
        }
    }

    if let Some(context) = object.get("context") {
        if !context.is_object() {
            detail.push(dict_type_detail("context", context));
        }
    }

    detail
}

pub(crate) fn validate_assign_payload_shape(payload: &Value) -> Vec<Value> {
    let mut detail = Vec::new();
    let Some(object) = payload.as_object() else {
        detail.push(model_type_detail(payload));
        return detail;
    };

    match object.get("assigned_user_ids") {
        None => detail.push(missing_field_detail("assigned_user_ids", payload)),
        Some(value) if !value.is_array() => detail.push(list_type_detail("assigned_user_ids", value)),
        Some(value) => {
            if let Some(items) = value.as_array() {
                for (index, item) in items.iter().enumerate() {
                    if !item.is_string() {
                        detail.push(string_type_detail_with_index("assigned_user_ids", index, item));
                    }
                }
            }
        }
    }

    if let Some(notes) = object.get("notes") {
        if !notes.is_null() && !notes.is_string() {
            detail.push(string_type_detail("notes", notes));
        }
    }

    if let Some(complete_process_task) = object.get("complete_process_task") {
        if !complete_process_task.is_boolean() {
            if complete_process_task.is_string() {
                detail.push(json!({
                    "type": "bool_parsing",
                    "loc": ["body", "complete_process_task"],
                    "msg": "Input should be a valid boolean, unable to interpret input",
                    "input": complete_process_task.clone(),
                }));
            } else {
                detail.push(json!({
                    "type": "bool_type",
                    "loc": ["body", "complete_process_task"],
                    "msg": "Input should be a valid boolean",
                    "input": complete_process_task.clone(),
                }));
            }
        }
    }

    detail
}

pub(crate) fn can_manage_dispatch_claims(claims: Option<&JwtAuth>) -> bool {
    let Some(claims) = claims else {
        return false;
    };

    claims.0.is_admin.unwrap_or(false)
        || AuthorizationService::has_any_grant(
            &claims.0,
            &[
                PermissionCatalog::DISPATCH_ORDER_UPDATE,
                PermissionCatalog::WORKFLOW_RUN_ACT,
            ],
        )
}

pub(crate) fn service_unavailable(detail: &str) -> HttpResponse {
    record_service_unavailable_background(detail, "workflow_dispatch_route", "infrastructure");
    HttpResponse::ServiceUnavailable().json(serde_json::json!({
        "detail": detail
    }))
}

pub(crate) async fn department_scope(
    auth_svc: Option<&Arc<AuthService>>,
    claims: &JwtAuth,
) -> Result<Option<String>, ApiError> {
    if claims.0.is_admin.unwrap_or(false) {
        return Ok(None);
    }

    let Some(user_id) = claims.0.sub.as_deref() else {
        return Ok(Some("__NO_DEPARTMENT__".to_string()));
    };

    let department = if let Some(auth_svc) = auth_svc {
        auth_svc
            .find_user_by_id(user_id)
            .await?
            .and_then(|user| user.department)
            .filter(|value| !value.trim().is_empty())
    } else {
        claims.0.department.clone().filter(|value| !value.trim().is_empty())
    };

    Ok(Some(department.unwrap_or_else(|| "__NO_DEPARTMENT__".to_string())))
}
