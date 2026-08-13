//! Notification exports.

use mobile_core::dto::notification as core;

use super::runtime;

/// Mirror of `NotificationItem`.
pub struct Notification {
    pub notification_id: String,
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub category: String,
    pub severity: String,
    pub is_read: bool,
    pub read_status: String,
    pub delivery_status: String,
    pub delivered_at: Option<String>,
    pub origin_type: String,
    pub origin_label: String,
    pub receipt_required: bool,
    pub receipt_group_id: Option<String>,
    pub ack_status: String,
    pub ack_at: Option<String>,
    pub ack_note: Option<String>,
    pub related_entity_type: Option<String>,
    pub related_entity_id: Option<String>,
    pub created_at: String,
    pub read_at: Option<String>,
}

impl From<core::NotificationItem> for Notification {
    fn from(n: core::NotificationItem) -> Self {
        Self {
            notification_id: n.notification_id,
            user_id: n.user_id,
            title: n.title,
            body: n.body,
            category: n.category,
            severity: n.severity,
            is_read: n.is_read,
            read_status: n.read_status,
            delivery_status: n.delivery_status,
            delivered_at: n.delivered_at,
            origin_type: n.origin_type,
            origin_label: n.origin_label,
            receipt_required: n.receipt_required,
            receipt_group_id: n.receipt_group_id,
            ack_status: n.ack_status,
            ack_at: n.ack_at,
            ack_note: n.ack_note,
            related_entity_type: n.related_entity_type,
            related_entity_id: n.related_entity_id,
            created_at: n.created_at,
            read_at: n.read_at,
        }
    }
}

/// Mirror of `NotificationListResponse`.
pub struct NotificationList {
    pub items: Vec<Notification>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

impl From<core::NotificationListResponse> for NotificationList {
    fn from(r: core::NotificationListResponse) -> Self {
        Self {
            items: r.items.into_iter().map(Into::into).collect(),
            total: r.total,
            limit: r.limit,
            offset: r.offset,
        }
    }
}

/// Mirror of `NotificationReceipt`.
pub struct NotificationReceipt {
    pub notification_id: String,
    pub user_id: String,
    pub title: Option<String>,
    pub origin_type: String,
    pub origin_label: String,
    pub receipt_group_id: Option<String>,
    pub delivery_status: String,
    pub delivered_at: Option<String>,
    pub read_status: String,
    pub read_at: Option<String>,
    pub ack_status: String,
    pub ack_at: Option<String>,
    pub ack_note: Option<String>,
    pub updated_at: String,
}

impl From<core::NotificationReceipt> for NotificationReceipt {
    fn from(r: core::NotificationReceipt) -> Self {
        Self {
            notification_id: r.notification_id,
            user_id: r.user_id,
            title: r.title,
            origin_type: r.origin_type,
            origin_label: r.origin_label,
            receipt_group_id: r.receipt_group_id,
            delivery_status: r.delivery_status,
            delivered_at: r.delivered_at,
            read_status: r.read_status,
            read_at: r.read_at,
            ack_status: r.ack_status,
            ack_at: r.ack_at,
            ack_note: r.ack_note,
            updated_at: r.updated_at,
        }
    }
}

/// Mirror of `NotificationReceiptSummary`.
pub struct ReceiptSummary {
    pub total_count: i64,
    pub pending_count: i64,
    pub acknowledged_count: i64,
    pub rejected_count: i64,
    pub latest_updated_at: Option<String>,
}

impl From<core::NotificationReceiptSummary> for ReceiptSummary {
    fn from(s: core::NotificationReceiptSummary) -> Self {
        Self {
            total_count: s.total_count,
            pending_count: s.pending_count,
            acknowledged_count: s.acknowledged_count,
            rejected_count: s.rejected_count,
            latest_updated_at: s.latest_updated_at,
        }
    }
}

/// Mirror of `NotificationReceiptGroup`.
pub struct ReceiptGroup {
    pub receipt_group_id: String,
    pub title: Option<String>,
    pub flight_id: Option<String>,
    pub dispatch_order_id: Option<String>,
    pub group_id: Option<String>,
    pub origin_type: String,
    pub origin_label: String,
    pub receipt_required: bool,
    pub summary: ReceiptSummary,
    pub items: Vec<NotificationReceipt>,
}

impl From<core::NotificationReceiptGroup> for ReceiptGroup {
    fn from(g: core::NotificationReceiptGroup) -> Self {
        Self {
            receipt_group_id: g.receipt_group_id,
            title: g.title,
            flight_id: g.flight_id,
            dispatch_order_id: g.dispatch_order_id,
            group_id: g.group_id,
            origin_type: g.origin_type,
            origin_label: g.origin_label,
            receipt_required: g.receipt_required,
            summary: g.summary.into(),
            items: g.items.into_iter().map(Into::into).collect(),
        }
    }
}

/// `GET /api/v2/notifications`.
pub async fn notifications(
    limit: i64,
    offset: i64,
    only_unread: bool,
) -> anyhow::Result<NotificationList> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::notification::notifications(&rt.client, limit, offset, only_unread)
            .await?
            .into(),
    )
}

/// `GET /api/v2/notifications/unread-count`.
pub async fn unread_count() -> anyhow::Result<i64> {
    let rt = runtime()?;
    Ok(mobile_core::api::notification::unread_count(&rt.client).await?)
}

/// `POST /api/v2/notifications/{id}/read`.
pub async fn notification_read(id: String) -> anyhow::Result<()> {
    let rt = runtime()?;
    mobile_core::api::notification::notification_read(&rt.client, &id).await?;
    Ok(())
}

/// `POST /api/v2/notifications/read-all`.
pub async fn notification_read_all() -> anyhow::Result<()> {
    let rt = runtime()?;
    mobile_core::api::notification::notification_read_all(&rt.client).await?;
    Ok(())
}

/// `POST /api/v2/notifications/{id}/ack`. `action` is `ack` or `reject`.
pub async fn notification_ack(
    id: String,
    action: String,
    note: Option<String>,
) -> anyhow::Result<NotificationReceipt> {
    let rt = runtime()?;
    Ok(mobile_core::api::notification::notification_ack(
        &rt.client,
        &id,
        &action,
        note.as_deref(),
    )
    .await?
    .into())
}

/// `GET /api/v2/notifications/receipt-groups/{receipt_group_id}`.
pub async fn receipt_group(receipt_group_id: String) -> anyhow::Result<ReceiptGroup> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::notification::receipt_group(&rt.client, &receipt_group_id)
            .await?
            .into(),
    )
}
