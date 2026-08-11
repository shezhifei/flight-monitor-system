use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

#[cfg(test)]
mod tests;

pub mod generate_proposal;
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/proposals")
            .route("", web::post().to(generate_proposal::generate_proposal))
            .route("", web::get().to(generate_proposal::list_proposals))
            .route("/stats", web::get().to(generate_proposal::get_proposal_stats))
            .route(
                "/expire-stale",
                web::post().to(generate_proposal::expire_stale_proposals),
            )
            .route("/validate", web::post().to(generate_proposal::validate_proposal))
            .route("/{proposal_id}", web::get().to(generate_proposal::get_proposal))
            .route(
                "/{proposal_id}/approve",
                web::post().to(generate_proposal::approve_proposal),
            )
            .route(
                "/{proposal_id}/reject",
                web::post().to(generate_proposal::reject_proposal),
            )
            .route(
                "/{proposal_id}/execute",
                web::post().to(generate_proposal::execute_proposal),
            ),
    );
}
