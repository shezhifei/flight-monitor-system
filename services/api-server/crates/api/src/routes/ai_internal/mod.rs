use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

#[cfg(test)]
mod tests;

pub mod ingest_run_event;
pub mod runtime_health;
pub mod tools_explain;
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/internal/ai/v1")
            .route("/jobs/lease", web::post().to(ingest_run_event::lease_job))
            .route(
                "/jobs/{job_id}/heartbeat",
                web::post().to(ingest_run_event::heartbeat_job),
            )
            .route("/jobs/{job_id}/runs", web::get().to(ingest_run_event::list_job_runs))
            .route(
                "/runs/{run_id}/events",
                web::post().to(ingest_run_event::ingest_run_event),
            )
            .route(
                "/runs/{run_id}/complete",
                web::post().to(ingest_run_event::complete_run),
            )
            .route("/runs/{run_id}/fail", web::post().to(ingest_run_event::fail_run))
            .route("/health", web::get().to(runtime_health::runtime_health))
            .route("/tools/explain", web::get().to(tools_explain::tools_explain)),
    );
}
