use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

#[cfg(test)]
mod tests;

pub mod list_models;
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/micro-models")
            .route("", web::get().to(list_models::list_models))
            .route("/{model_id}", web::get().to(list_models::get_model))
            .route("/{model_id}/execute", web::post().to(list_models::execute_model)),
    );
}
