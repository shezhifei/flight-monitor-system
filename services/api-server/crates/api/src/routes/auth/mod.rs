use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

// Re-export items consumed by the fms-server crate with public visibility.
pub use shared::{LoginFailureRateLimiter, LoginRateLimitDecision};

#[cfg(test)]
mod tests;

pub mod login;
pub mod update_role;
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/auth")
            .route("/login", web::post().to(login::login))
            .route("/register", web::post().to(login::register))
            .route("/refresh", web::post().to(login::refresh))
            .route("/sse-token", web::post().to(login::sse_token))
            .route("/logout", web::post().to(login::logout))
            .route("/heartbeat", web::post().to(login::heartbeat))
            .route("/online-users", web::get().to(login::online_users))
            .route("/user-status/{user_id}", web::get().to(login::user_status))
            .route("/kick-user/{user_id}", web::post().to(login::kick_user))
            .route("/me", web::get().to(login::me))
            .route("/me/profile", web::patch().to(login::update_profile))
            .route("/me/operator-context", web::put().to(login::update_operator_context))
            .route("/change-password", web::post().to(login::change_password))
            .route("/users", web::get().to(login::list_users))
            .route("/users/{user_id}", web::get().to(login::get_user))
            .route("/users/{user_id}", web::put().to(login::update_user))
            .route("/users/{user_id}", web::delete().to(login::delete_user))
            .route("/permissions", web::get().to(login::list_permissions))
            .route("/roles", web::post().to(login::create_role))
            .route("/roles", web::get().to(login::list_roles))
            .route("/roles/{role_id}", web::put().to(update_role::update_role))
            .route("/roles/{role_id}", web::delete().to(update_role::delete_role))
            .route("/assign-role", web::post().to(update_role::assign_role))
            .route(
                "/roles/{role_id}/permissions",
                web::post().to(update_role::add_permission),
            )
            .route(
                "/roles/{role_id}/permissions/{permission}",
                web::delete().to(update_role::remove_permission),
            )
            .route("/protected", web::get().to(update_role::protected))
            .route("/admin-only", web::get().to(update_role::admin_only))
            .route("/online-status", web::get().to(update_role::online_status))
            .route("/online-history", web::get().to(update_role::online_history))
            .route("/force-offline/{user_id}", web::post().to(update_role::force_offline)),
    );
}
