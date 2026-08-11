use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

#[cfg(test)]
mod tests;

pub mod list_notifications;
/// 注册通知路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/notifications")
            .route("", web::get().to(list_notifications::list_notifications))
            .route("/unread-count", web::get().to(list_notifications::get_unread_count))
            .route("/read-all", web::post().to(list_notifications::mark_all_read))
            .route("/preferences", web::get().to(list_notifications::get_preferences))
            .route("/preferences", web::patch().to(list_notifications::update_preferences))
            .route(
                "/dispatch/online-users",
                web::get().to(list_notifications::list_dispatch_online_users),
            )
            .route(
                "/dispatch/send",
                web::post().to(list_notifications::send_dispatch_manual_notification),
            )
            .route(
                "/receipt-groups/{receipt_group_id}",
                web::get().to(list_notifications::get_receipt_group),
            )
            .route(
                "/sent-receipt-groups",
                web::get().to(list_notifications::list_sent_receipt_groups),
            )
            .route("/{notification_id}/read", web::post().to(list_notifications::mark_read))
            .route(
                "/{notification_id}/ack",
                web::post().to(list_notifications::ack_notification),
            )
            .route(
                "/{notification_id}/receipts",
                web::get().to(list_notifications::get_receipts),
            ),
    );
}
