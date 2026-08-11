package com.flightmonitor.mobile.api.model

data class DispatchOrderItem(
    val id: String,
    val flight_id: String,
    val step_code: String,
    val status: String,
    val terminal: String? = null,
    val stand_id: String? = null,
    val gate: String? = null,
    val estimated_completion_time: String? = null,
    val origin_type: String = "manual",
    val origin_label: String = "人工",
    val notification_receipt_summary: Map<String, Any?> = emptyMap(),
)

data class DispatchOrderAcceptRequest(
    val note: String? = null,
    val client_action_id: String? = null,
)

data class DispatchOrderCheckInRequest(
    val qr_code: String? = null,
    val lat: Double? = null,
    val lng: Double? = null,
    val accuracy_m: Double? = null,
    val note: String? = null,
    val client_action_id: String? = null,
)

data class DispatchOrderCheckOutRequest(
    val lat: Double? = null,
    val lng: Double? = null,
    val note: String? = null,
    val client_action_id: String? = null,
    val recorded_at: String? = null,
)

data class DispatchOrderStartRequest(
    val actual_start_time: String? = null,
    val notes: String? = null,
)

data class DispatchOrderCompleteRequest(
    val actual_end_time: String? = null,
    val completion_notes: String? = null,
    val issues: List<String> = emptyList(),
)

data class DispatchOrderEtaReportRequest(
    val estimated_completion_time: String,
    val note: String? = null,
    val client_action_id: String? = null,
)

data class DispatchIssueReportRequest(
    val title: String,
    val description: String? = null,
    val severity: String = "medium",
    val issue_type: String = "dispatch_issue",
    val note: String? = null,
    val lat: Double? = null,
    val lng: Double? = null,
    val attachments: List<String> = emptyList(),
    val client_action_id: String? = null,
)

data class DispatchSyncAction(
    val client_action_id: String,
    val action_type: String,
    val dispatch_order_id: String,
    val action_timestamp: String,
    val payload: Map<String, Any?> = emptyMap(),
)

data class DispatchSyncRequest(
    val actions: List<DispatchSyncAction>,
)

data class DispatchSyncResult(
    val client_action_id: String,
    val dispatch_order_id: String,
    val action_type: String,
    val status: String,
    val message: String,
    val server_timestamp: String,
)

data class DispatchSyncResponse(
    val total: Int,
    val applied: Int,
    val duplicates: Int,
    val failed: Int,
    val results: List<DispatchSyncResult> = emptyList(),
)

data class DispatchActionOutcome(
    val actionType: String,
    val orderId: String,
    val queued: Boolean,
    val message: String,
)

data class DispatchSyncOutcome(
    val total: Int,
    val applied: Int,
    val duplicates: Int,
    val failed: Int,
    val remainingQueueSize: Int,
)
