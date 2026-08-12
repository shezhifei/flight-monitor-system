package com.flightmonitor.mobile.api.model

data class ShiftHandoverItem(
    val item_id: String,
    val handover_id: String,
    val item_type: String,
    val title: String,
    val detail: String? = null,
    val owner_user_id: String? = null,
    val due_at: String? = null,
    val is_mandatory: Boolean = true,
    val acknowledged: Boolean = false,
    val acknowledged_at: String? = null,
    val acknowledged_by: String? = null,
    val created_at: String,
    val updated_at: String,
)

data class ShiftHandover(
    val handover_id: String,
    val shift_date: String,
    val shift_code: String,
    val from_user_id: String,
    val to_user_id: String,
    val from_operator_name: String? = null,
    val from_operator_job_title: String? = null,
    val from_operator_label: String? = null,
    val to_operator_name: String? = null,
    val to_operator_job_title: String? = null,
    val to_operator_label: String? = null,
    val status: String,
    val summary: String? = null,
    val risk_level: String = "medium",
    val signed_at: String? = null,
    val submitted_at: String? = null,
    val created_at: String,
    val updated_at: String,
    val items: List<ShiftHandoverItem> = emptyList(),
)

data class ShiftHandoverItemAcknowledgeRequest(
    val acknowledged: Boolean = true,
)
