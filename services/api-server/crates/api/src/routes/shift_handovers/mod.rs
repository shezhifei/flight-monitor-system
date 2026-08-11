use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

pub mod create_handover;
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/shift-handovers")
            .route("", web::post().to(create_handover::create_handover))
            .route("", web::get().to(create_handover::list_handovers))
            .route(
                "/system-draft-preview",
                web::get().to(create_handover::preview_system_draft),
            )
            .route("/candidates", web::get().to(create_handover::list_candidates))
            .route("/{handover_id}", web::get().to(create_handover::get_handover))
            .route(
                "/{handover_id}/submit",
                web::post().to(create_handover::submit_handover),
            )
            .route(
                "/{handover_id}/items/{item_id}/ack",
                web::post().to(create_handover::acknowledge_item),
            )
            .route(
                "/{handover_id}/ack",
                web::post().to(create_handover::acknowledge_handover),
            ),
    );
}
