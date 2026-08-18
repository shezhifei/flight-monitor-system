//! Actix-Web 路由与共享状态配置模块
//!
//! 负责将组装好的 DiContainer 状态资源及所有 API 路由注册到 Actix-Web 服务配置中。

use crate::di::DiContainer;
use actix_web::web;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Flight Monitor System API",
        version = "0.1.0",
        description = "机场航班运营平台 API"
    ),
    paths(fms_api::routes::ping::ping),
    components(schemas(fms_api::routes::ping::PingResponse)),
    tags(
        (name = "health", description = "Health check endpoints")
    )
)]
struct ApiDoc;

async fn redirect_root_to_login() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Found()
        .insert_header(("Location", "/frontend/login.html"))
        .finish()
}

pub fn configure_app(cfg: &mut web::ServiceConfig, di: &DiContainer) {
    // 1. Enforce payload size limits for JSON and Query configs
    let json_config = web::JsonConfig::default().error_handler(fms_api::error::default_json_payload_error_handler);
    let query_config = web::QueryConfig::default().error_handler(fms_api::error::default_query_payload_error_handler);
    let payload_limit = web::PayloadConfig::new(crate::config::max_request_size_bytes());

    cfg.app_data(json_config).app_data(query_config).app_data(payload_limit);

    // 2. Register optional Redis and AntiReplay states
    if let Some(redis_data) = di.redis_pool.clone() {
        cfg.app_data(redis_data);
    }
    if let Some(store_data) = di.anti_replay_store.clone() {
        cfg.app_data(store_data);
    }

    // 3. Register Core Services & Repositories as Shared App Data
    cfg.app_data(web::Data::new(di.jwt_secret.clone()))
        .app_data(web::Data::new(di.jwt_audience.clone()))
        .app_data(web::Data::new(di.workflow_internal_token.clone()))
        .app_data(web::Data::new(di.pool.clone()))
        .app_data(web::Data::new(di.flight_svc.clone()))
        .app_data(web::Data::new(di.flight_batch_cell_svc.clone()))
        .app_data(web::Data::new(di.cache_invalidation_svc.clone()))
        .app_data(web::Data::new(di.label_svc.clone()))
        .app_data(web::Data::new(di.flight_import_svc.clone()))
        .app_data(web::Data::new(di.flight_archive_svc.clone()))
        .app_data(web::Data::new(di.ontology_svc.clone()))
        .app_data(web::Data::new(di.auth_svc.clone()))
        .app_data(web::Data::new(di.login_failure_limiter.clone()))
        .app_data(web::Data::new(di.auth_validation_cache.clone()))
        .app_data(web::Data::new(di.todo_svc.clone()))
        .app_data(web::Data::new(di.auth_admin_query_svc.clone()))
        .app_data(web::Data::new(di.auth_admin_command_svc.clone()))
        .app_data(web::Data::new(di.online_history_svc.clone()))
        .app_data(web::Data::new(di.online_status_svc.clone()))
        .app_data(web::Data::new(di.operator_identity_svc.clone()))
        .app_data(web::Data::new(di.dispatch_svc.clone()))
        .app_data(web::Data::new(di.dispatch_query_svc.clone()))
        .app_data(web::Data::new(di.dispatch_overrun_warning_svc.clone()))
        .app_data(web::Data::new(di.dispatch_frontend_replan_svc.clone()))
        .app_data(web::Data::new(di.llm_eval_svc.clone()))
        .app_data(web::Data::new(di.dispatch_collaboration_repo.clone()))
        .app_data(web::Data::new(di.dispatch_collaboration_query_svc.clone()))
        .app_data(web::Data::new(di.dispatch_chat_svc.clone()))
        .app_data(web::Data::new(di.dispatch_rule_svc.clone()))
        .app_data(web::Data::new(di.event_rule_admin_svc.clone()))
        .app_data(web::Data::new(di.dispatch_schedule_svc.clone()))
        .app_data(web::Data::new(di.dispatch_analytics_svc.clone()))
        .app_data(web::Data::new(di.dispatch_scenario_svc.clone()))
        .app_data(web::Data::new(di.mobile_device_svc.clone()))
        .app_data(web::Data::new(di.mobile_upload_svc.clone()))
        .app_data(web::Data::new(di.mobile_workbench_svc.clone()))
        .app_data(web::Data::new(di.dashboard_workbench_svc.clone()))
        .app_data(web::Data::new(di.mobile_operations_svc.clone()))
        .app_data(web::Data::new(di.nl_query_svc.clone()))
        .app_data(web::Data::new(di.notification_svc.clone()))
        .app_data(web::Data::new(di.anomaly_svc.clone()))
        .app_data(web::Data::new(di.ai_admin_svc.clone()))
        .app_data(web::Data::new(di.ai_route_svc.clone()))
        .app_data(web::Data::new(di.ai_media_svc.clone()))
        .app_data(web::Data::new(di.ai_business_case_copilot_svc.clone()))
        .app_data(web::Data::new(di.ai_realtime_audio_svc.clone()))
        .app_data(web::Data::new(di.ai_runtime_svc.clone()))
        .app_data(web::Data::new(di.ai_runtime_client.clone()))
        .app_data(web::Data::new(di.ai_action_proposal_svc.clone()))
        .app_data(web::Data::new(di.micro_model_registry.clone()))
        .app_data(web::Data::new(di.ai_job_svc.clone()))
        .app_data(web::Data::new(di.ai_output_validator.clone()))
        .app_data(web::Data::new(di.ai_ontology_repo.clone()))
        .app_data(web::Data::new(di.ontology_actions.clone()))
        .app_data(web::Data::new(di.ai_proposal_ingest_svc.clone()))
        .app_data(web::Data::new(di.ai_execution_readiness_svc.clone()))
        .app_data(web::Data::new(di.ai_execution_metrics_svc.clone()))
        .app_data(web::Data::new(di.ai_rollout_status_svc.clone()))
        .app_data(web::Data::new(di.ai_context_svc.clone()))
        .app_data(web::Data::new(di.ai_control_svc.clone()))
        .app_data(web::Data::new(di.ai_rollback_svc.clone()))
        .app_data(web::Data::new(di.ai_run_auth_loader.clone()))
        .app_data(web::Data::new(di.system_flags_svc.clone()))
        .app_data(web::Data::new(di.business_case_type_svc.clone()))
        .app_data(web::Data::new(di.flight_cache_svc.clone()))
        .app_data(web::Data::new(di.flight_runtime_svc.clone()))
        .app_data(web::Data::new(di.workflow_dispatch_svc.clone()))
        .app_data(web::Data::new(di.workflow_form_svc.clone()))
        .app_data(web::Data::new(di.flowable_svc.clone()))
        .app_data(web::Data::new(di.flowable_draft_svc.clone()))
        .app_data(web::Data::new(di.shift_handover_svc.clone()))
        .app_data(web::Data::new(di.kpi_aggregation_svc.clone()))
        .app_data(web::Data::new(di.system_ops_svc.clone()))
        .app_data(web::Data::new(di.business_case_svc.clone()))
        .app_data(web::Data::new(di.business_case_workflow_svc.clone()))
        .app_data(web::Data::new(di.dispatch_resource_svc.clone()))
        .app_data(web::Data::new(di.resource_utilization_svc.clone()))
        .app_data(web::Data::new(di.sse_hub.clone()))
        .app_data(web::Data::new(di.performance_metrics.clone()))
        .app_data(web::Data::new(di.runtime_error_monitor.clone()))
        .app_data(web::Data::new(di.scheduler_runtime_svc.clone()));

    // 4. Configure API Routes
    cfg.configure(fms_api::routes::metrics::configure)
        .configure(fms_api::routes::health::configure)
        .configure(fms_api::routes::auth_admin::configure)
        .configure(fms_api::routes::auth::configure)
        .configure(fms_api::routes::flights::configure)
        .configure(fms_api::routes::labels::configure)
        .configure(fms_api::routes::ontology::configure)
        .configure(fms_api::routes::todos::configure)
        .configure(fms_api::routes::dispatch::configure)
        .configure(fms_api::routes::dispatch_resources::configure)
        .configure(fms_api::routes::dispatch_resources::configure_dispatch_direct_routes)
        .configure(fms_api::routes::dispatch_collaboration::configure)
        .configure(fms_api::routes::dispatch_chat::configure)
        .configure(fms_api::routes::notifications::configure)
        .configure(fms_api::routes::business_cases::configure)
        .configure(fms_api::routes::business_case_workflows::configure)
        .configure(fms_api::routes::business_case_types::configure)
        .configure(fms_api::routes::reference::configure)
        .configure(fms_api::routes::anomalies::configure)
        .configure(fms_api::routes::kpi::configure)
        .configure(fms_api::routes::shift_handovers::configure)
        .configure(fms_api::routes::scheduler::configure)
        .configure(fms_api::routes::system::configure)
        .configure(fms_api::routes::archive::configure)
        .configure(fms_api::routes::mobile::configure)
        .configure(fms_api::routes::dashboard::configure)
        .configure(fms_api::routes::resource_utilization::configure)
        .configure(fms_api::routes::workflow_dispatch::configure)
        .configure(fms_api::routes::ai_eval::configure)
        .configure(fms_api::routes::nl_query::configure)
        .configure(fms_api::routes::ai_ontology::configure)
        .configure(fms_api::routes::ai_proposals::configure)
        .configure(fms_api::routes::ai_media::configure)
        .configure(fms_api::routes::ai_copilot::configure)
        .configure(fms_api::routes::ai_realtime_audio::configure)
        .configure(fms_api::routes::ai_micro_models::configure)
        .configure(fms_api::routes::ai_jobs::configure)
        .configure(fms_api::routes::ai::configure)
        .configure(fms_api::routes::ai_internal::configure)
        .configure(fms_api::routes::ai_resume::configure)
        .configure(fms_api::routes::ai_rollback::configure)
        .configure(fms_api::sse::handler::configure)
        .configure(fms_api::routes::flowable::configure)
        .configure(fms_api::routes::workflow_forms::configure)
        .configure(fms_api::routes::static_files::configure)
        .route(
            "/api/v2/openapi.json",
            web::get().to(|| async {
                let openapi = ApiDoc::openapi();
                actix_web::HttpResponse::Ok().json(openapi)
            }),
        )
        .service(utoipa_swagger_ui::SwaggerUi::new("/swagger-ui/{_:.*}").url("/api/v2/openapi.json", ApiDoc::openapi()))
        .route("/", web::get().to(redirect_root_to_login));
}

#[cfg(test)]
mod tests {
    use actix_web::{http::StatusCode, test, web, App};

    use super::redirect_root_to_login;

    #[actix_web::test]
    async fn root_redirects_to_canonical_vue_login() {
        let app = test::init_service(App::new().route("/", web::get().to(redirect_root_to_login))).await;
        let response = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get("Location").and_then(|value| value.to_str().ok()),
            Some("/frontend/login.html")
        );
    }
}
