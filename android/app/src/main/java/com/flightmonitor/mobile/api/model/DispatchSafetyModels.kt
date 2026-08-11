package com.flightmonitor.mobile.api.model

data class DispatchSafetyChecklistItemStatus(
    val item_code: String,
    val title: String,
    val required: Boolean = false,
    val allow_na: Boolean = false,
    val order: Int = 0,
    val result: String? = null,
    val checked_by: String? = null,
    val checked_by_username: String? = null,
    val checked_at: String? = null,
    val note: String? = null,
    val status: String = "pending",
)

data class DispatchSafetyChecklistStatus(
    val dispatch_order_id: String,
    val step_code: String,
    val template_id: String? = null,
    val template_version: String? = null,
    val enforced: Boolean = false,
    val ready: Boolean = true,
    val required_total: Int = 0,
    val completed_required: Int = 0,
    val pending_required_items: List<String> = emptyList(),
    val failed_required_items: List<String> = emptyList(),
    val items: List<DispatchSafetyChecklistItemStatus> = emptyList(),
)

data class DispatchSafetyChecklistItemResultRequest(
    val result: String,
    val note: String? = null,
)

data class DispatchSafetyChecklistRecord(
    val record_id: String,
    val dispatch_order_id: String,
    val item_code: String,
    val result: String,
    val checked_by: String? = null,
    val checked_by_username: String? = null,
    val checked_at: String,
    val note: String? = null,
    val template_version: String? = null,
    val created_at: String,
    val updated_at: String,
)
