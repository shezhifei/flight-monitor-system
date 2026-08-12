package com.flightmonitor.mobile.api.model

data class GenericApiResponse<T>(
    val success: Boolean,
    val data: T?,
    val message: String? = null,
    val error: Any? = null,
    val request_id: String? = null,
)

data class MobileWorkbenchResponse(
    val user_id: String,
    val generated_at: String,
    val my_orders: List<MobileWorkbenchOrderItem> = emptyList(),
    val order_counts: MobileWorkbenchCounts,
    val notification_unread_count: Int = 0,
    val chat_unread_total: Int = 0,
    val pending_shift_handover_count: Int = 0,
    val pending_sync_action_count: Int = 0,
    val channel_recommendation: Map<String, Boolean> = emptyMap(),
)

data class MobileOperationsEventItem(
    val event_id: String,
    val event_type: String,
    val severity: String = "info",
    val status: String = "open",
    val title: String,
    val flight_id: String? = null,
    val occurred_at: String,
    val source: String,
    val payload: Map<String, Any?> = emptyMap(),
)

data class MobileOperationsEventsResponse(
    val user_id: String,
    val generated_at: String,
    val total: Int = 0,
    val event_type_counts: Map<String, Int> = emptyMap(),
    val severity_counts: Map<String, Int> = emptyMap(),
    val events: List<MobileOperationsEventItem> = emptyList(),
)

data class MobileUploadAsset(
    val upload_id: String,
    val original_filename: String,
    val content_type: String? = null,
    val file_size: Long = 0,
    val checksum_sha256: String? = null,
    val created_at: String,
    val attachment_url: String,
    val metadata: Map<String, Any?> = emptyMap(),
)

data class MobileWorkbenchOrderItem(
    val order_id: String,
    val flight_id: String,
    val step_code: String,
    val status: String,
    val terminal: String? = null,
    val stand_id: String? = null,
    val gate: String? = null,
    val planned_start_time: String? = null,
    val planned_end_time: String? = null,
    val actual_start_time: String? = null,
    val assignment_deadline: String? = null,
    val supervisor_notified: Boolean = false,
)

data class MobileWorkbenchCounts(
    val pending: Int = 0,
    val assigned: Int = 0,
    val in_progress: Int = 0,
    val completed: Int = 0,
    val cancelled: Int = 0,
    val total: Int = 0,
)

data class MobileDeviceRegisterRequest(
    val device_id: String,
    val platform: String = "android",
    val push_channel: String = "none",
    val push_token: String? = null,
    val app_version: String? = null,
    val os_version: String? = null,
    val device_model: String? = null,
    val manufacturer: String? = null,
    val metadata: Map<String, Any?> = emptyMap(),
)

data class MobileDeviceHeartbeatRequest(
    val network_status: String? = null,
    val battery_level: Int? = null,
    val metadata: Map<String, Any?> = emptyMap(),
)

data class MobileDeviceResponse(
    val device_id: String,
    val user_id: String,
    val is_active: Boolean,
    val last_heartbeat_at: String,
)
