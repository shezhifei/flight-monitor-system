use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

#[cfg(test)]
mod tests;

/// Standalone mount used by this module's isolation tests. Production composes the
/// routes into the shared `/api/v2/ai` scope via [`register_scoped_routes`].
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/v2/ai").configure(register_scoped_routes));
}
