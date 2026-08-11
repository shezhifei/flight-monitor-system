use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

#[cfg(test)]
mod tests;

pub mod list_adjustment_rules;
pub fn configure_event_rules_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/adjustment-rules")
            .route("", web::get().to(list_adjustment_rules::list_adjustment_rules))
            .route("", web::post().to(list_adjustment_rules::create_adjustment_rule))
            .route("/{id}", web::get().to(list_adjustment_rules::get_adjustment_rule))
            .route("/{id}", web::put().to(list_adjustment_rules::update_adjustment_rule))
            .route("/{id}", web::delete().to(list_adjustment_rules::delete_adjustment_rule))
            .route(
                "/{id}/enable",
                web::patch().to(list_adjustment_rules::enable_adjustment_rule),
            )
            .route(
                "/{id}/disable",
                web::patch().to(list_adjustment_rules::disable_adjustment_rule),
            ),
    )
    .service(
        web::scope("/generation-rules")
            .route("", web::get().to(list_adjustment_rules::list_generation_rules))
            .route("", web::post().to(list_adjustment_rules::create_generation_rule))
            .route("/{id}", web::get().to(list_adjustment_rules::get_generation_rule))
            .route("/{id}", web::put().to(list_adjustment_rules::update_generation_rule))
            .route("/{id}", web::delete().to(list_adjustment_rules::delete_generation_rule))
            .route(
                "/{id}/enable",
                web::patch().to(list_adjustment_rules::enable_generation_rule),
            )
            .route(
                "/{id}/disable",
                web::patch().to(list_adjustment_rules::disable_generation_rule),
            ),
    )
    .route("/rules/preview", web::post().to(list_adjustment_rules::preview_rules));
}
