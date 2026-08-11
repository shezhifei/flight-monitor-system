//! AI 助手路由
//!
//! 配置中心、工具元数据与执行态接口均对齐到 Rust 实现。
//! 业务编排已下沉至 `fms_application::services::ai_route_service`。

use actix_web::web;

pub mod capabilities_and_tools;
pub mod executions;
pub mod metrics_and_stream;
pub mod pending_actions;
pub mod shared;
#[cfg(test)]
mod tests;

/// 注册 AI 路由
///
/// This module owns the single `/api/v2/ai` scope. The config-v2 proxy and the
/// execution-readiness surfaces share the same prefix; actix would shadow them if
/// they were registered as separate `web::scope("/api/v2/ai")` services (the first
/// matching scope wins with no fallthrough), so they are composed into this one
/// scope via `Scope::configure`. Do NOT also register their standalone `configure`
/// fns in the same app.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai")
            .configure(super::ai_config_v2::register_scoped_routes)
            .configure(super::ai_execution_readiness::register_scoped_routes)
            .route("/capabilities", web::get().to(capabilities_and_tools::capabilities))
            .route("/tools", web::get().to(capabilities_and_tools::list_tools))
            .route("/tools/execute", web::post().to(capabilities_and_tools::execute_tool))
            .route(
                "/tools/categories",
                web::get().to(capabilities_and_tools::list_tool_categories),
            )
            .route("/pending-actions", web::get().to(pending_actions::list_pending_actions))
            .route(
                "/pending-actions/batch-approve",
                web::post().to(pending_actions::batch_approve),
            )
            .route(
                "/pending-actions/batch-reject",
                web::post().to(pending_actions::batch_reject),
            )
            .route(
                "/pending-actions/{action_id}/diff",
                web::get().to(pending_actions::get_action_diff),
            )
            .route(
                "/pending-actions/{action_id}/result",
                web::get().to(pending_actions::get_action_result),
            )
            .route(
                "/pending-actions/{action_id}/approve",
                web::post().to(pending_actions::approve_action),
            )
            .route(
                "/pending-actions/{action_id}/reject",
                web::post().to(pending_actions::reject_action),
            )
            .route(
                "/pending-actions/{action_id}/approve-with-modification",
                web::post().to(pending_actions::approve_modified),
            )
            .route("/entities", web::get().to(capabilities_and_tools::list_entities))
            .route(
                "/entities/{entity_id}",
                web::get().to(capabilities_and_tools::get_entity),
            )
            .route(
                "/entities/{entity_id}",
                web::put().to(capabilities_and_tools::update_entity),
            )
            .route(
                "/entities/{entity_id}",
                web::post().to(capabilities_and_tools::update_entity),
            )
            .route(
                "/entities/{entity_id}/prompt",
                web::get().to(capabilities_and_tools::get_entity_prompt),
            )
            .route(
                "/entities/{entity_id}/prompt",
                web::post().to(capabilities_and_tools::update_entity_prompt),
            )
            .route(
                "/entities/{entity_id}/tools",
                web::get().to(capabilities_and_tools::get_entity_tools),
            )
            .route(
                "/entities/{entity_id}/tools",
                web::post().to(capabilities_and_tools::update_entity_tools),
            )
            .route(
                "/connection/test",
                web::post().to(capabilities_and_tools::test_connection),
            )
            .route("/models", web::get().to(capabilities_and_tools::list_models))
            .route(
                "/registry/status",
                web::get().to(capabilities_and_tools::registry_status),
            )
            .route(
                "/registry/initialize",
                web::post().to(capabilities_and_tools::registry_initialize),
            )
            .route("/todos/{todo_id}/execute", web::post().to(executions::execute_todo))
            .route(
                "/todos/{todo_id}/execute-tree",
                web::post().to(executions::execute_todo_tree),
            )
            .route("/chains/from-template", web::post().to(executions::create_chain))
            .route("/chains/templates", web::get().to(executions::list_chain_templates))
            .route(
                "/chains/{root_todo_id}/status",
                web::get().to(executions::get_chain_status),
            )
            .route("/executions", web::get().to(executions::list_executions))
            .route("/executions/{run_id}", web::get().to(executions::get_execution))
            .route(
                "/executions/{run_id}/cancel",
                web::post().to(executions::cancel_execution),
            )
            .route(
                "/rate-limit/status",
                web::get().to(metrics_and_stream::rate_limit_status),
            )
            .route(
                "/metrics/query-routing",
                web::get().to(metrics_and_stream::query_routing_metrics),
            )
            .route(
                "/metrics/report-schema",
                web::get().to(metrics_and_stream::report_schema_metrics),
            )
            .route(
                "/metrics/execution-visibility",
                web::get().to(metrics_and_stream::execution_visibility_metrics),
            )
            .route(
                "/metrics/todo-graph-pilot",
                web::get().to(metrics_and_stream::todo_graph_pilot_metrics),
            )
            .route("/generate_plan", web::post().to(metrics_and_stream::generate_plan))
            .route("/events/stream", web::get().to(metrics_and_stream::events_stream)),
    );
}
