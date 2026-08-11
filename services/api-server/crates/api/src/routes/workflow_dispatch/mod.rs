mod handlers;
mod responses;
mod shared;

#[cfg(test)]
mod tests;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    let json_config = web::JsonConfig::default().error_handler(shared::workflow_dispatch_json_error_handler);
    cfg.service(
        web::scope("/api/v2/workflows/integrations/dispatch")
            .app_data(json_config)
            .route("/trigger", web::post().to(handlers::trigger_dispatch_from_workflow))
            .route(
                "/pending",
                web::get().to(handlers::list_pending_workflow_dispatch_orders),
            )
            .route(
                "/{dispatch_order_id}/assign",
                web::post().to(handlers::assign_workflow_dispatch_order),
            )
            .route(
                "/{dispatch_order_id}/recommendations",
                web::get().to(handlers::get_workflow_dispatch_recommendations),
            ),
    );
}
