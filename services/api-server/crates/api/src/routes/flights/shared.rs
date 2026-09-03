//! 航班路由共享类型、辅助函数和查询参数定义。

use actix_web::HttpResponse;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use fms_application::schemas::flight_schemas::{DispatchTimelineEventResponse, FlightResponse};
use fms_domain::error::DomainError;

pub fn ok_resp(message: impl Into<String>, data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "message": message.into(),
    }))
}

pub async fn removed_public_route() -> HttpResponse {
    HttpResponse::NotFound().finish()
}

pub fn actor_id(claims: &JwtAuth) -> &str {
    claims
        .0
        .username
        .as_deref()
        .or(claims.0.sub.as_deref())
        .unwrap_or("System")
}

pub fn viewer_department_id(claims: &JwtAuth) -> Option<&str> {
    claims
        .0
        .department_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn viewer_department_name(claims: &JwtAuth) -> Option<&str> {
    claims
        .0
        .department
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn map_flight_write_error(error: DomainError) -> ApiError {
    match error {
        DomainError::NotFound { entity_type, id } => ApiError::NotFound(format!("{entity_type} (id={id}) 未找到")),
        DomainError::ValidationError(message) | DomainError::BusinessRuleViolation(message) => {
            ApiError::ValidationError(message)
        }
        DomainError::BusinessRuleViolationWithDetails { message, .. } => ApiError::ValidationError(message),
        DomainError::InvalidStateTransition { from, to } => {
            ApiError::ValidationError(format!("非法状态转换: {from} → {to}"))
        }
        DomainError::PermissionDenied(message) => ApiError::Forbidden(message),
        DomainError::Unauthorized(message) => ApiError::Unauthorized(message),
        DomainError::Conflict(message) | DomainError::ConcurrencyConflict(message) => ApiError::Conflict(message),
        DomainError::Internal(message) => ApiError::Internal(message),
    }
}

pub fn update_changed_fields(dto: &fms_application::schemas::flight_schemas::FlightUpdate) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if dto.status.is_some() {
        fields.push("status");
    }
    // PR3：stand/gate/terminal/baggage_carousel 为只读展示列，已从 FlightUpdate
    // 删除（serde 422 拒绝），不再出现在 PATCH 变更字段里。
    if dto.position.is_touched() {
        fields.push("position");
    }
    if dto.scheduled_departure.is_touched() {
        fields.push("scheduled_departure");
    }
    if dto.scheduled_arrival.is_touched() {
        fields.push("scheduled_arrival");
    }
    if dto.estimated_departure.is_touched() {
        fields.push("estimated_departure");
    }
    if dto.estimated_arrival.is_touched() {
        fields.push("estimated_arrival");
    }
    if dto.actual_departure.is_touched() {
        fields.push("actual_departure");
    }
    if dto.actual_arrival.is_touched() {
        fields.push("actual_arrival");
    }
    if dto.cobt_time.is_touched() {
        fields.push("cobt_time");
    }
    if dto.aircraft_type_detail.is_touched() {
        fields.push("aircraft_type_detail");
    }
    if dto.registration.is_touched() {
        fields.push("registration");
    }
    if dto.has_boarding_restriction.is_some() {
        fields.push("has_boarding_restriction");
    }
    if dto.is_quick_turnaround.is_some() {
        fields.push("is_quick_turnaround");
    }
    if dto.is_commercial_signed.is_some() {
        fields.push("is_commercial_signed");
    }
    if dto.inbound_leg.is_touched() {
        fields.push("inbound_leg");
    }
    if dto.outbound_leg.is_touched() {
        fields.push("outbound_leg");
    }
    if dto.flight_remarks.is_touched() {
        fields.push("flight_remarks");
    }
    if dto.load_planning_remarks.is_touched() {
        fields.push("load_planning_remarks");
    }
    if dto.aircraft_maintenance_remarks.is_touched() {
        fields.push("aircraft_maintenance_remarks");
    }
    if dto.aircraft_check_remarks.is_touched() {
        fields.push("aircraft_check_remarks");
    }
    fields
}

#[allow(dead_code)]
pub fn flight_update_patch_payload<S: AsRef<str>>(flight: &FlightResponse, changed_fields: &[S]) -> Value {
    let mut patch = Map::new();
    patch.insert("flight_id".to_string(), json!(flight.flight_id));
    patch.insert("version".to_string(), json!(flight.version));
    patch.insert("updated_at".to_string(), json!(flight.updated_at));

    for field in changed_fields {
        let field = field.as_ref();
        let value = match field {
            "status" => json!(flight.status),
            "gate" => json!(flight.gate),
            "terminal" => json!(flight.terminal),
            "stand" => json!(flight.stand),
            "position" => json!(flight.position),
            "baggage_carousel" => json!(flight.baggage_carousel),
            "scheduled_departure" => json!(flight.scheduled_departure),
            "scheduled_arrival" => json!(flight.scheduled_arrival),
            "estimated_departure" => json!(flight.estimated_departure),
            "estimated_arrival" => json!(flight.estimated_arrival),
            "actual_departure" => json!(flight.actual_departure),
            "actual_arrival" => json!(flight.actual_arrival),
            "cobt_time" => json!(flight.cobt_time),
            "codt" => json!(flight.codt),
            "on_blocks_time" => json!(flight.on_blocks_time),
            "cabin_door_open_time" => json!(flight.cabin_door_open_time),
            "deboarding_complete_time" => json!(flight.deboarding_complete_time),
            "cleaning_start_time" => json!(flight.cleaning_start_time),
            "cleaning_end_time" => json!(flight.cleaning_end_time),
            "boarding_allowed_time" => json!(flight.boarding_allowed_time),
            "start_boarding_time" => json!(flight.start_boarding_time),
            "passenger_ready_time" => json!(flight.passenger_ready_time),
            "end_boarding_time" => json!(flight.end_boarding_time),
            "cabin_door_close_time" => json!(flight.cabin_door_close_time),
            "cargo_door_close_time" => json!(flight.cargo_door_close_time),
            "loading_complete_time" => json!(flight.loading_complete_time),
            "off_blocks_time" => json!(flight.off_blocks_time),
            "aircraft_type_detail" => json!(flight.aircraft_type_detail),
            "registration" => json!(flight.registration),
            "has_boarding_restriction" => json!(flight.has_boarding_restriction),
            "is_quick_turnaround" => json!(flight.is_quick_turnaround),
            "is_commercial_signed" => json!(flight.is_commercial_signed),
            "inbound_leg" => json!(flight.inbound_leg),
            "outbound_leg" => json!(flight.outbound_leg),
            "flight_remarks" => json!(flight.flight_remarks),
            "load_planning_remarks" => json!(flight.load_planning_remarks),
            "aircraft_maintenance_remarks" => json!(flight.aircraft_maintenance_remarks),
            "aircraft_check_remarks" => json!(flight.aircraft_check_remarks),
            _ => continue,
        };
        patch.insert(field.to_string(), value);
    }

    Value::Object(patch)
}

#[allow(dead_code)]
pub fn dispatch_timeline_patch_payload(
    flight: Option<&FlightResponse>,
    event: &DispatchTimelineEventResponse,
) -> Value {
    let field = event.milestone_code.trim().to_string();
    if field.is_empty() {
        let mut patch = Map::new();
        patch.insert("flight_id".to_string(), json!(event.flight_id));
        return Value::Object(patch);
    }

    let mut patch = flight
        .map(|flight| flight_update_patch_payload(flight, std::slice::from_ref(&field)))
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_else(|| {
            let mut patch = Map::new();
            patch.insert("flight_id".to_string(), json!(event.flight_id));
            patch
        });

    patch
        .entry("flight_id".to_string())
        .or_insert_with(|| json!(event.flight_id));
    patch.entry(field).or_insert_with(|| json!(event.occurred_at));
    Value::Object(patch)
}

#[allow(dead_code)]
pub fn dispatch_timeline_flight_updated_payload(
    flight_id: &str,
    patch: Value,
    event: &DispatchTimelineEventResponse,
) -> Value {
    let mut payload = serde_json::json!({
        "type": "flight_updated",
        "flight_id": flight_id,
        "changed_fields": [event.milestone_code.clone()],
        "timeline_event": event,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    payload["flight"] = patch.clone();
    payload["patch"] = patch;
    payload
}

pub struct FlightListResponseCacheInvalidatorAdapter;

#[async_trait::async_trait]
impl fms_application::services::cache_invalidation_service::FlightListResponseCacheInvalidator
    for FlightListResponseCacheInvalidatorAdapter
{
    async fn invalidate_flight_list_response_cache(&self) {
        super::cache::invalidate_flight_list_response_cache().await;
    }
}

#[derive(Deserialize)]
pub struct FlightListQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub has_open_anomaly: Option<bool>,
}

#[derive(Deserialize)]
pub struct FlightSearchQuery {
    pub flight_no: Option<String>,
    pub status: Option<String>,
    pub origin: Option<String>,
    pub destination: Option<String>,
    pub has_open_anomaly: Option<bool>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Deserialize)]
pub struct RecentUpdatesQuery {
    pub minutes: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct FlightHistoryQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Deserialize)]
pub struct FlightInsightQuery {
    pub hours: Option<i64>,
    pub incident_type: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct FlightWsQuery {
    pub access_token: String,
    pub format: Option<String>,
}
