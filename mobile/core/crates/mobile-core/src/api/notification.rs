//! Notification API wrappers (plan §0.5 Notifications).
//!
//! - List / unread-count / receipt / receipt-group → raw
//! - mark-read / ack / read-all → envelope

use crate::client::ApiClient;
use crate::dto::notification::{
    NotificationAcknowledgeRequest, NotificationItem, NotificationListResponse,
    NotificationReceipt, NotificationReceiptGroup, NotificationUnreadCountResponse,
};
use crate::error::CoreError;

/// `GET /api/v2/notifications`.
pub async fn notifications(
    client: &ApiClient,
    limit: i64,
    offset: i64,
    only_unread: bool,
) -> Result<NotificationListResponse, CoreError> {
    client
        .call_raw(
            "GET",
            &format!(
                "/api/v2/notifications?limit={limit}&offset={offset}&unread_only={only_unread}"
            ),
            Option::<&()>::None,
        )
        .await
}

/// `GET /api/v2/notifications/unread-count`.
pub async fn unread_count(client: &ApiClient) -> Result<i64, CoreError> {
    let resp: NotificationUnreadCountResponse = client
        .call_raw(
            "GET",
            "/api/v2/notifications/unread-count",
            Option::<&()>::None,
        )
        .await?;
    Ok(resp.unread_count)
}

/// `POST /api/v2/notifications/{id}/read` (envelope ack).
pub async fn notification_read(client: &ApiClient, id: &str) -> Result<(), CoreError> {
    let _: serde_json::Value = client
        .call_with_envelope(
            "POST",
            &format!("/api/v2/notifications/{id}/read"),
            Option::<&()>::None,
        )
        .await?;
    Ok(())
}

/// `POST /api/v2/notifications/read-all` (envelope ack).
pub async fn notification_read_all(client: &ApiClient) -> Result<(), CoreError> {
    let _: serde_json::Value = client
        .call_with_envelope(
            "POST",
            "/api/v2/notifications/read-all",
            Option::<&()>::None,
        )
        .await?;
    Ok(())
}

/// Backend only accepts `acknowledged` / `rejected` (see
/// `routes/notifications/shared.rs`). Map the shorter UI verbs.
pub fn normalize_ack_action(action: &str) -> String {
    match action.trim().to_ascii_lowercase().as_str() {
        "ack" | "acknowledge" | "acknowledged" => "acknowledged".to_string(),
        "reject" | "rejected" => "rejected".to_string(),
        other => other.to_string(),
    }
}

/// `POST /api/v2/notifications/{id}/ack` — `ack`/`reject` or full verbs.
pub async fn notification_ack(
    client: &ApiClient,
    id: &str,
    action: &str,
    note: Option<&str>,
) -> Result<NotificationReceipt, CoreError> {
    client
        .call_with_envelope(
            "POST",
            &format!("/api/v2/notifications/{id}/ack"),
            Some(&NotificationAcknowledgeRequest {
                action: normalize_ack_action(action),
                note: note.map(str::to_string),
            }),
        )
        .await
}

/// `GET /api/v2/notifications/{id}/receipts` → raw single receipt.
pub async fn notification_receipt(
    client: &ApiClient,
    id: &str,
) -> Result<NotificationReceipt, CoreError> {
    client
        .call_raw(
            "GET",
            &format!("/api/v2/notifications/{id}/receipts"),
            Option::<&()>::None,
        )
        .await
}

/// `GET /api/v2/notifications/receipt-groups/{receipt_group_id}`.
pub async fn receipt_group(
    client: &ApiClient,
    receipt_group_id: &str,
) -> Result<NotificationReceiptGroup, CoreError> {
    client
        .call_raw(
            "GET",
            &format!("/api/v2/notifications/receipt-groups/{receipt_group_id}"),
            Option::<&()>::None,
        )
        .await
}

// Re-export for callers that need the item type after list.
pub type ListedNotification = NotificationItem;

#[cfg(test)]
mod tests {
    use super::normalize_ack_action;

    #[test]
    fn ack_verbs_normalize_to_backend_values() {
        assert_eq!(normalize_ack_action("ack"), "acknowledged");
        assert_eq!(normalize_ack_action("ACK"), "acknowledged");
        assert_eq!(normalize_ack_action("acknowledged"), "acknowledged");
        assert_eq!(normalize_ack_action("reject"), "rejected");
        assert_eq!(normalize_ack_action("rejected"), "rejected");
    }
}
