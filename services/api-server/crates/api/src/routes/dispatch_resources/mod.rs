//! 派工资源与规则路由。

use actix_web::{http::header, web, HttpRequest, HttpResponse};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use fms_application::services::auth_service::AuthService;

pub mod alerts;
pub mod analytics;
pub mod departments;
pub mod equipment;
pub mod orders;
pub mod rules;
pub mod schedule;
pub mod stands;
pub mod task_types;
pub mod team_types;
pub mod teams;
pub mod terminal_directory;

#[derive(Debug, serde::Serialize)]
pub struct MessageResponse {
    pub message: String,
}

const PROTOBUF_MEDIA_TYPE: &str = "application/x-protobuf";

pub fn request_id(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn ok_resp(req: &HttpRequest, data: impl Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "error": null,
        "request_id": request_id(req),
    }))
}

pub fn created_resp(req: &HttpRequest, data: impl Serialize) -> HttpResponse {
    HttpResponse::Created().json(json!({
        "success": true,
        "data": data,
        "error": null,
        "request_id": request_id(req),
    }))
}

pub async fn department_scope(auth_svc: &Arc<AuthService>, claims: &JwtAuth) -> Result<Option<String>, ApiError> {
    if claims.0.is_admin.unwrap_or(false) {
        return Ok(None);
    }

    let Some(user_id) = claims.0.sub.as_deref() else {
        return Ok(Some("__NO_DEPARTMENT__".to_string()));
    };

    let department = auth_svc
        .find_user_by_id(user_id)
        .await?
        .and_then(|user| user.department)
        .filter(|value| !value.trim().is_empty());

    Ok(Some(department.unwrap_or_else(|| "__NO_DEPARTMENT__".to_string())))
}

pub fn request_wants_protobuf(req: &HttpRequest) -> bool {
    req.headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains(PROTOBUF_MEDIA_TYPE))
        .unwrap_or(false)
}

pub fn configure_dispatch_order_read_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::get().to(orders::list_orders))
        .route("/timeline", web::get().to(orders::get_timeline))
        .route("/conflicts", web::get().to(orders::list_conflicts))
        .route("/cascade-preview", web::get().to(orders::cascade_preview))
        .route("/my/assigned", web::get().to(orders::list_my_orders))
        .route("/{order_id}/timeline", web::get().to(orders::get_order_timeline));
}

pub fn configure_dispatch_direct_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/v2/dispatch/alerts", web::get().to(alerts::list_alerts))
        .route(
            "/api/v2/dispatch/alerts/{id}/acknowledge",
            web::post().to(alerts::acknowledge_alert),
        )
        .route(
            "/api/v2/dispatch/alerts/{id}/resolve",
            web::post().to(alerts::resolve_alert),
        )
        .route(
            "/api/v2/dispatch/schedule/templates",
            web::get().to(schedule::list_schedule_templates),
        )
        .route(
            "/api/v2/dispatch/schedule/templates",
            web::post().to(schedule::create_schedule_template),
        )
        .route(
            "/api/v2/dispatch/schedule/instances",
            web::get().to(schedule::list_schedule_instances),
        )
        .route(
            "/api/v2/dispatch/schedule/instances",
            web::post().to(schedule::create_schedule_instance),
        )
        .route(
            "/api/v2/dispatch/schedule/exceptions",
            web::get().to(schedule::list_schedule_exceptions),
        )
        .route(
            "/api/v2/dispatch/schedule/exceptions",
            web::post().to(schedule::create_schedule_exception),
        )
        .route(
            "/api/v2/dispatch/schedule/availability",
            web::get().to(schedule::get_schedule_availability),
        )
        .route(
            "/api/v2/dispatch/analytics/summary",
            web::get().to(analytics::get_dispatch_analytics_summary),
        )
        .route(
            "/api/v2/dispatch/analytics/breakdown",
            web::get().to(analytics::get_dispatch_analytics_breakdown),
        )
        .route(
            "/api/v2/dispatch/analytics/trend",
            web::get().to(analytics::get_dispatch_analytics_trend),
        )
        .route(
            "/api/v2/dispatch/scenarios/preview",
            web::post().to(analytics::preview_dispatch_scenario),
        )
        .route(
            "/api/v2/dispatch/task-types",
            web::get().to(task_types::list_task_types),
        )
        .route(
            "/api/v2/dispatch/task-types",
            web::post().to(task_types::create_task_type),
        )
        .route(
            "/api/v2/dispatch/task-types/{task_type_id}",
            web::delete().to(task_types::delete_task_type),
        )
        .route(
            "/api/v2/dispatch/team-types",
            web::get().to(team_types::list_team_types),
        )
        .route(
            "/api/v2/dispatch/team-types/{team_type_id}",
            web::get().to(team_types::get_team_type),
        )
        .route(
            "/api/v2/dispatch/team-types",
            web::post().to(team_types::create_team_type),
        )
        .route(
            "/api/v2/dispatch/team-types/{team_type_id}",
            web::put().to(team_types::update_team_type),
        )
        .route(
            "/api/v2/dispatch/team-types/{team_type_id}",
            web::delete().to(team_types::delete_team_type),
        )
        .route("/api/v2/dispatch/teams", web::get().to(teams::list_teams))
        .route("/api/v2/dispatch/teams/{team_id}", web::get().to(teams::get_team))
        .route("/api/v2/dispatch/teams", web::post().to(teams::create_team))
        .route("/api/v2/dispatch/teams/{team_id}", web::put().to(teams::update_team))
        .route("/api/v2/dispatch/teams/{team_id}", web::delete().to(teams::delete_team))
        .route(
            "/api/v2/dispatch/teams/{team_id}/position",
            web::put().to(teams::update_team_position),
        )
        .route(
            "/api/v2/dispatch/teams/{team_id}/status",
            web::put().to(teams::update_team_status),
        )
        .route(
            "/api/v2/dispatch/teams/{team_id}/members",
            web::get().to(teams::list_team_members),
        )
        .route(
            "/api/v2/dispatch/teams/{team_id}/members",
            web::post().to(teams::add_team_member),
        )
        .route(
            "/api/v2/dispatch/teams/{team_id}/members/{user_id}",
            web::delete().to(teams::remove_team_member),
        )
        .route(
            "/api/v2/dispatch/equipment-types",
            web::get().to(equipment::list_equipment_types),
        )
        .route(
            "/api/v2/dispatch/equipment-types",
            web::post().to(equipment::create_equipment_type),
        )
        .route(
            "/api/v2/dispatch/equipment-types/{equipment_type_id}",
            web::put().to(equipment::update_equipment_type),
        )
        .route(
            "/api/v2/dispatch/equipment-types/{equipment_type_id}",
            web::delete().to(equipment::delete_equipment_type),
        )
        .route("/api/v2/dispatch/equipment", web::get().to(equipment::list_equipment))
        .route(
            "/api/v2/dispatch/equipment/{equipment_id}",
            web::get().to(equipment::get_equipment),
        )
        .route(
            "/api/v2/dispatch/equipment",
            web::post().to(equipment::create_equipment),
        )
        .route(
            "/api/v2/dispatch/equipment/{equipment_id}",
            web::put().to(equipment::update_equipment),
        )
        .route(
            "/api/v2/dispatch/equipment/{equipment_id}/position",
            web::put().to(equipment::update_equipment_position),
        )
        .route(
            "/api/v2/dispatch/equipment/{equipment_id}/status",
            web::put().to(equipment::update_equipment_status),
        );
}

pub fn configure_terminal_directory_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/v2/dispatch/terminals", web::get().to(terminal_directory::list_terminals))
        .route(
            "/api/v2/dispatch/terminals",
            web::post().to(terminal_directory::create_terminal),
        )
        .route(
            "/api/v2/dispatch/terminals/{terminal_id}",
            web::get().to(terminal_directory::get_terminal),
        )
        .route(
            "/api/v2/dispatch/terminals/{terminal_id}",
            web::patch().to(terminal_directory::update_terminal),
        )
        .route(
            "/api/v2/dispatch/terminals/{terminal_id}/deactivate",
            web::post().to(terminal_directory::deactivate_terminal),
        )
        .route(
            "/api/v2/dispatch/terminals/{terminal_id}/context",
            web::get().to(terminal_directory::get_context),
        )
        .route(
            "/api/v2/dispatch/terminals/{terminal_id}/stands/{stand_id}",
            web::post().to(terminal_directory::add_stand_member),
        )
        .route(
            "/api/v2/dispatch/terminals/stands/{stand_id}",
            web::delete().to(terminal_directory::remove_stand_member),
        )
        .route(
            "/api/v2/dispatch/terminals/{terminal_id}/gates/{gate_id}",
            web::post().to(terminal_directory::add_gate_member),
        )
        .route(
            "/api/v2/dispatch/terminals/gates/{gate_id}",
            web::delete().to(terminal_directory::remove_gate_member),
        )
        .route(
            "/api/v2/dispatch/terminals/{terminal_id}/carousels/{carousel_id}",
            web::post().to(terminal_directory::add_carousel_member),
        )
        .route(
            "/api/v2/dispatch/terminals/carousels/{carousel_id}",
            web::delete().to(terminal_directory::remove_carousel_member),
        )
        .route("/api/v2/dispatch/gates", web::post().to(terminal_directory::create_gate))
        .route(
            "/api/v2/dispatch/gates/{gate_id}",
            web::patch().to(terminal_directory::update_gate),
        )
        .route(
            "/api/v2/dispatch/gates/{gate_id}/deactivate",
            web::post().to(terminal_directory::deactivate_gate),
        )
        .route(
            "/api/v2/dispatch/carousels",
            web::post().to(terminal_directory::create_carousel),
        )
        .route(
            "/api/v2/dispatch/carousels/{carousel_id}",
            web::patch().to(terminal_directory::update_carousel),
        )
        .route(
            "/api/v2/dispatch/carousels/{carousel_id}/deactivate",
            web::post().to(terminal_directory::deactivate_carousel),
        );
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/dispatch/resources")
            .route("/departments", web::get().to(departments::list_departments))
            .route(
                "/departments/{department_id}",
                web::get().to(departments::get_department),
            )
            .route("/departments", web::post().to(departments::create_department))
            .route(
                "/departments/{department_id}",
                web::put().to(departments::update_department),
            )
            .route("/team-types", web::get().to(team_types::list_team_types))
            .route("/team-types/{team_type_id}", web::get().to(team_types::get_team_type))
            .route("/team-types", web::post().to(team_types::create_team_type))
            .route(
                "/team-types/{team_type_id}",
                web::put().to(team_types::update_team_type),
            )
            .route(
                "/team-types/{team_type_id}",
                web::delete().to(team_types::delete_team_type),
            )
            .route("/teams", web::get().to(teams::list_teams))
            .route("/teams/{team_id}", web::get().to(teams::get_team))
            .route("/teams", web::post().to(teams::create_team))
            .route("/teams/{team_id}", web::put().to(teams::update_team))
            .route("/teams/{team_id}", web::delete().to(teams::delete_team))
            .route("/teams/{team_id}/position", web::put().to(teams::update_team_position))
            .route("/teams/{team_id}/status", web::put().to(teams::update_team_status))
            .route("/teams/{team_id}/members", web::get().to(teams::list_team_members))
            .route("/teams/{team_id}/members", web::post().to(teams::add_team_member))
            .route(
                "/teams/{team_id}/members/{user_id}",
                web::delete().to(teams::remove_team_member),
            )
            .route("/equipment-types", web::get().to(equipment::list_equipment_types))
            .route("/equipment-types", web::post().to(equipment::create_equipment_type))
            .route(
                "/equipment-types/{equipment_type_id}",
                web::put().to(equipment::update_equipment_type),
            )
            .route(
                "/equipment-types/{equipment_type_id}",
                web::delete().to(equipment::delete_equipment_type),
            )
            .route("/equipment", web::get().to(equipment::list_equipment))
            .route("/equipment/{equipment_id}", web::get().to(equipment::get_equipment))
            .route("/equipment", web::post().to(equipment::create_equipment))
            .route("/equipment/{equipment_id}", web::put().to(equipment::update_equipment))
            .route(
                "/equipment/{equipment_id}/position",
                web::put().to(equipment::update_equipment_position),
            )
            .route(
                "/equipment/{equipment_id}/status",
                web::put().to(equipment::update_equipment_status),
            )
            .route("/stands", web::get().to(stands::list_stands))
            .route("/stands/{stand_id}", web::get().to(stands::get_stand))
            .route("/stands", web::post().to(stands::create_stand)),
    );

    cfg.service(
        web::scope("/api/v2/dispatch/rules")
            .route(
                "/departments/{department_id}/qualifications",
                web::get().to(rules::list_department_qualifications),
            )
            .route(
                "/departments/{department_id}/qualifications",
                web::post().to(rules::create_department_qualification),
            )
            .route(
                "/departments/{department_id}/qualification-levels",
                web::get().to(rules::list_department_qualification_levels),
            )
            .route(
                "/departments/{department_id}/qualification-levels",
                web::post().to(rules::create_department_qualification_level),
            )
            .route(
                "/departments/{department_id}/qualification-grants",
                web::get().to(rules::list_department_qualification_grants),
            )
            .route(
                "/departments/{department_id}/qualification-grants",
                web::post().to(rules::create_department_qualification_grant),
            )
            .route(
                "/departments/{department_id}/task-type-requirements/versions",
                web::get().to(rules::list_department_task_type_requirement_versions),
            )
            .route(
                "/departments/{department_id}/task-type-requirements/drafts",
                web::post().to(rules::create_department_task_type_requirement_draft),
            )
            .route(
                "/departments/{department_id}/task-type-requirements/publish",
                web::post().to(rules::publish_department_task_type_requirement),
            )
            .route(
                "/departments/{department_id}/flight-generation-rules",
                web::get().to(rules::list_department_flight_generation_rules),
            )
            .route(
                "/departments/{department_id}/flight-generation-rules",
                web::post().to(rules::create_department_flight_generation_rule),
            )
            .route(
                "/departments/{department_id}/flight-generation-rules/{rule_id}/delete",
                web::post().to(rules::delete_department_flight_generation_rule),
            )
            .route(
                "/departments/{department_id}/generation-adjustment-rules",
                web::get().to(rules::list_department_generation_adjustment_rules),
            )
            .route(
                "/departments/{department_id}/generation-adjustment-rules",
                web::post().to(rules::create_department_generation_adjustment_rule),
            )
            .route(
                "/departments/{department_id}/temporary-task-templates",
                web::get().to(rules::list_department_temporary_task_templates),
            )
            .route(
                "/departments/{department_id}/temporary-task-templates",
                web::post().to(rules::create_department_temporary_task_template),
            )
            .route("/validate", web::post().to(rules::validate_department_dispatch_rules))
            .route("/preview", web::post().to(rules::preview_department_dispatch_rules)),
    );
}

#[cfg(test)]
mod tests;
