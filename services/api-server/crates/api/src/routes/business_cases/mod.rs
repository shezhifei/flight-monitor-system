mod crud;
mod operations;
mod shared;

#[cfg(test)]
mod tests;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/business-cases")
            .route("", web::get().to(crud::list_business_cases))
            .route("", web::post().to(crud::create_business_case))
            .route("/{case_id}", web::get().to(crud::get_business_case))
            .route("/{case_id}", web::put().to(crud::update_business_case))
            .route(
                "/{case_id}/status",
                web::patch().to(operations::update_business_case_status),
            )
            .route("/{case_id}", web::delete().to(operations::delete_business_case))
            .route("/{case_id}/appends", web::post().to(operations::append_to_case))
            .route(
                "/{case_id}/appends/{append_id}/acknowledge",
                web::post().to(operations::acknowledge_append),
            ),
    );
}
