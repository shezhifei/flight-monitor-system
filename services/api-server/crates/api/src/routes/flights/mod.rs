//! 航班 CRUD 路由
//!
//! 对齐 Python `flight_routes.py` 的主要接口面。

mod batch_cells;
mod cache;
mod crud;
mod list;
mod monitor_rows;
mod proto;
mod shared;
mod sse;
mod timeline;

#[cfg(test)]
mod tests;

use actix_web::web;

pub use cache::invalidate_flight_list_response_cache;
pub use shared::FlightListResponseCacheInvalidatorAdapter;

/// 注册航班路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/flights")
            .route("", web::get().to(list::list_flights))
            .route("", web::post().to(crud::create_flight))
            .route("/search", web::get().to(list::search_flights))
            .route("/monitor-rows", web::get().to(monitor_rows::list))
            .route("/updates/recent", web::get().to(list::recent_updates))
            .route("/stream", web::get().to(shared::removed_public_route))
            .route("/ws", web::get().to(shared::removed_public_route))
            // Static path must be registered BEFORE `/{flight_id}` catch-all.
            .route("/batch-cells", web::patch().to(batch_cells::batch_update_cells))
            .route("/{flight_id}", web::get().to(crud::get_flight))
            .route("/{flight_id}", web::put().to(crud::update_flight))
            .route("/{flight_id}", web::patch().to(crud::patch_flight))
            .route(
                "/{flight_id}/dispatch-timeline",
                web::get().to(timeline::get_dispatch_timeline),
            )
            .route(
                "/{flight_id}/dispatch-timeline/events",
                web::post().to(timeline::create_dispatch_timeline_event),
            )
            .route(
                "/{flight_id}/dispatch-timeline/events/{timeline_id}",
                web::delete().to(timeline::delete_dispatch_timeline_event),
            )
            .route("/{flight_id}/history", web::get().to(crud::get_flight_history))
            .route(
                "/{flight_id}/history-report",
                web::get().to(crud::get_flight_history_report),
            )
            .route("/{flight_id}/event-journey", web::get().to(crud::get_event_journey))
            .route(
                "/{flight_id}/confirm-draft",
                web::post().to(crud::confirm_draft_flight),
            ),
    );
}
