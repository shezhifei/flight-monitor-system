package com.flightmonitor.mobile.api.model

data class NotificationItem(
    val notification_id: String,
    val user_id: String,
    val title: String,
    val body: String,
    val category: String,
    val severity: String,
    val is_read: Boolean,
    val read_status: String = "unread",
    val delivery_status: String = "sent",
    val delivered_at: String? = null,
    val origin_type: String = "manual",
    val origin_label: String = "人工",
    val receipt_required: Boolean = false,
    val receipt_group_id: String? = null,
    val ack_status: String = "pending",
    val ack_at: String? = null,
    val ack_note: String? = null,
    val related_entity_type: String? = null,
    val related_entity_id: String? = null,
    val created_at: String,
    val read_at: String? = null,
)

data class NotificationListResponse(
    val items: List<NotificationItem> = emptyList(),
    val total: Int = 0,
    val limit: Int = 0,
    val offset: Int = 0,
)

data class NotificationUnreadCountResponse(
    val unread_count: Int = 0,
)

data class NotificationAcknowledgeRequest(
    val action: String,
    val note: String? = null,
)

data class NotificationReceipt(
    val notification_id: String,
    val user_id: String,
    val title: String? = null,
    val origin_type: String = "manual",
    val origin_label: String = "人工",
    val receipt_group_id: String? = null,
    val delivery_status: String,
    val delivered_at: String? = null,
    val read_status: String,
    val read_at: String? = null,
    val ack_status: String,
    val ack_at: String? = null,
    val ack_note: String? = null,
    val updated_at: String,
)

data class NotificationReceiptSummary(
    val total_count: Int = 0,
    val pending_count: Int = 0,
    val acknowledged_count: Int = 0,
    val rejected_count: Int = 0,
    val latest_updated_at: String? = null,
)

data class NotificationReceiptGroup(
    val receipt_group_id: String,
    val title: String? = null,
    val flight_id: String? = null,
    val dispatch_order_id: String? = null,
    val group_id: String? = null,
    val origin_type: String = "manual",
    val origin_label: String = "人工",
    val receipt_required: Boolean = true,
    val summary: NotificationReceiptSummary,
    val items: List<NotificationReceipt> = emptyList(),
)
