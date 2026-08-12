package com.flightmonitor.mobile.api.model

data class DispatchChatGroupSummary(
    val group_id: String,
    val channel_type: String,
    val flight_id: String,
    val group_name: String,
    val status: String = "active",
    val read_only: Boolean = false,
    val member_count: Int = 0,
    val unread_count: Int = 0,
    val last_message_seq: Int? = null,
    val last_message_preview: String? = null,
    val last_message_at: String? = null,
)

data class DispatchChatGroupListResponse(
    val items: List<DispatchChatGroupSummary> = emptyList(),
    val total: Int = 0,
    val limit: Int = 0,
    val offset: Int = 0,
    val unread_total: Int = 0,
)

data class DispatchChatMessage(
    val message_id: String,
    val seq_no: Int,
    val group_id: String,
    val sender_user_id: String? = null,
    val sender_username: String? = null,
    val message_type: String = "text",
    val content: String,
    val is_at_all: Boolean = false,
    val metadata: Map<String, Any?> = emptyMap(),
    val sent_at: String,
)

data class DispatchChatMessageListResponse(
    val items: List<DispatchChatMessage> = emptyList(),
    val total: Int = 0,
    val limit: Int = 0,
    val before_seq: Int? = null,
    val has_more: Boolean = false,
    val next_before_seq: Int? = null,
)

data class DispatchChatSendMessageRequest(
    val content: String,
    val at_all: Boolean = false,
)

data class DispatchChatMarkReadRequest(
    val read_seq: Int? = null,
)

data class DispatchChatReadResult(
    val group_id: String? = null,
    val unread_count: Int = 0,
    val unread_total: Int = 0,
    val read_seq: Int? = null,
    val read_at: String? = null,
)
