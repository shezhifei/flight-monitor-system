use actix_web::web;
pub mod shared;

#[cfg(test)]
mod tests;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/business-case-types")
            .route("", web::get().to(shared::list_case_types))
            .route("", web::post().to(shared::create_case_type))
            .route("/{code}/bpmn", web::put().to(shared::save_case_type_bpmn))
            .route("/{code}/status", web::put().to(shared::update_case_type_status))
            .route(
                "/{code}/ai-extraction-config",
                web::put().to(shared::update_case_type_ai_extraction_config),
            )
            .route(
                "/{code}/case-properties",
                web::put().to(shared::update_case_type_case_properties),
            ),
    );
}
