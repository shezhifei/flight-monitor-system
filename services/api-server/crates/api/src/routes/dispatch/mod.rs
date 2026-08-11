use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

#[cfg(test)]
mod tests;

pub mod create_order;
pub mod replan;
/// 注册派工单路由 (25 endpoints — 全覆盖)
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/dispatch-orders")
            .configure(dispatch_resources::configure_dispatch_order_read_routes)
            .route("", web::post().to(create_order::create_order))
            .route("/publish", web::post().to(create_order::publish_orders))
            .route("/followup-queue", web::get().to(create_order::followup_queue))
            .route("/burden-metrics", web::get().to(create_order::burden_metrics))
            .route("/replan-snapshot", web::get().to(create_order::replan_snapshot))
            .route("/replan-apply", web::post().to(create_order::replan_apply))
            .route(
                "/mobile/sync/actions",
                web::post().to(create_order::mobile_sync_actions),
            )
            .route(
                "/safety-checklist/templates/{task_type}",
                web::get().to(create_order::get_safety_template),
            )
            .route(
                "/safety-checklist/templates/{task_type}",
                web::put().to(create_order::update_safety_template),
            )
            .route(
                "/safety-checklist/progress",
                web::post().to(create_order::safety_checklist_progress),
            )
            .route("/validate", web::post().to(create_order::validate_order))
            .route("/replan", web::post().to(replan::replan))
            .route("/auto", web::post().to(replan::auto_dispatch))
            .route("/generate-drafts", web::post().to(replan::generate_drafts))
            .route("/batch-publish-drafts", web::post().to(replan::batch_publish_drafts))
            .route("/batch", web::post().to(replan::batch_dispatch))
            .route("/optimal", web::post().to(replan::optimal_dispatch))
            .route("/{order_id}", web::get().to(create_order::get_order))
            .route("/{order_id}/publish", web::post().to(create_order::publish_order))
            .route("/{order_id}/accept", web::post().to(create_order::accept_order))
            .route("/{order_id}/start", web::post().to(create_order::start_order))
            .route("/{order_id}/complete", web::post().to(create_order::complete_order))
            .route("/{order_id}/cancel", web::post().to(create_order::cancel_order))
            .route("/{order_id}/checkin", web::post().to(create_order::checkin_order))
            .route("/{order_id}/checkout", web::post().to(create_order::checkout_order))
            .route("/{order_id}/eta-report", web::post().to(create_order::eta_report))
            .route("/{order_id}/report-issue", web::post().to(create_order::report_issue))
            .route(
                "/{order_id}/safety-checklist",
                web::get().to(create_order::get_order_safety_checklist),
            )
            .route(
                "/{order_id}/safety-checklist/items/{item_code}",
                web::post().to(create_order::safety_checklist_check_item),
            )
            .route(
                "/{order_id}/safety-checklist/batch-submit",
                web::post().to(create_order::safety_checklist_batch_submit),
            ),
    );
}
