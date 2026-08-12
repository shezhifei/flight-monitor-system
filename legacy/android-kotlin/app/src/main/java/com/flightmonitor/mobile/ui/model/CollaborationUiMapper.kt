package com.flightmonitor.mobile.ui.model

import com.flightmonitor.mobile.api.model.DispatchChatGroupSummary
import com.flightmonitor.mobile.api.model.NotificationReceipt
import com.flightmonitor.mobile.api.model.NotificationReceiptGroup

object CollaborationUiMapper {
    fun mapOrigin(originType: String?, originLabel: String?): OriginUiModel {
        val normalized = originType.orEmpty().trim().lowercase()
        return if (normalized == "workflow") {
            OriginUiModel(kind = OriginKind.WORKFLOW, label = originLabel?.takeIf { it.isNotBlank() } ?: "流程")
        } else {
            OriginUiModel(kind = OriginKind.MANUAL, label = originLabel?.takeIf { it.isNotBlank() } ?: "人工")
        }
    }

    fun mapReceiptStatus(status: String?): ReceiptStatusUiModel {
        return when (status.orEmpty().trim().lowercase()) {
            "acknowledged" -> ReceiptStatusUiModel(ReceiptStatusKind.ACKNOWLEDGED, "已确认")
            "rejected" -> ReceiptStatusUiModel(ReceiptStatusKind.REJECTED, "已拒绝")
            else -> ReceiptStatusUiModel(ReceiptStatusKind.PENDING, "待回执")
        }
    }

    fun mapReceiptSummary(summary: Map<String, Any?>?): ReceiptSummaryUiModel {
        fun readInt(key: String): Int {
            return when (val value = summary?.get(key)) {
                is Number -> value.toInt()
                is String -> value.toIntOrNull() ?: 0
                else -> 0
            }
        }

        fun readString(key: String): String? {
            return summary?.get(key)?.toString()?.trim()?.takeIf { it.isNotBlank() }
        }

        val groupIds = when (val raw = summary?.get("receipt_group_ids")) {
            is List<*> -> raw.mapNotNull { it?.toString()?.trim()?.takeIf { text -> text.isNotBlank() } }
            else -> emptyList()
        }

        return ReceiptSummaryUiModel(
            totalCount = readInt("total_count"),
            pendingCount = readInt("pending_count"),
            acknowledgedCount = readInt("acknowledged_count"),
            rejectedCount = readInt("rejected_count"),
            latestUpdatedAt = readString("latest_updated_at"),
            receiptGroupIds = groupIds,
        )
    }

    fun mapNotificationActionState(ackStatus: String?, receiptRequired: Boolean): NotificationActionUiState {
        val receiptStatus = mapReceiptStatus(ackStatus)
        val pending = receiptStatus.kind == ReceiptStatusKind.PENDING
        return NotificationActionUiState(
            canSubmit = receiptRequired && pending,
            rejectionNoteRequired = true,
            finalStatusLabel = receiptStatus.label,
        )
    }

    fun validateAcknowledgement(action: String, note: String?): String? {
        val normalized = action.trim().lowercase()
        if (normalized == "rejected" && note.orEmpty().trim().isEmpty()) {
            return "拒绝通知必须填写理由"
        }
        return null
    }

    fun mapChatComposerState(group: DispatchChatGroupSummary?): ChatComposerUiState {
        if (group == null) {
            return ChatComposerUiState(canSend = false, readOnlyHint = null)
        }
        return if (group.read_only || group.status.equals("archived", ignoreCase = true)) {
            ChatComposerUiState(
                canSend = false,
                readOnlyHint = "你已被转为只读成员，无法继续发送消息",
            )
        } else {
            ChatComposerUiState(canSend = true, readOnlyHint = null)
        }
    }

    fun mapReceiptGroup(group: NotificationReceiptGroup): NotificationReceiptGroupUiModel {
        return NotificationReceiptGroupUiModel(
            receiptGroupId = group.receipt_group_id,
            title = group.title,
            origin = mapOrigin(group.origin_type, group.origin_label),
            receiptRequired = group.receipt_required,
            summary = ReceiptSummaryUiModel(
                totalCount = group.summary.total_count,
                pendingCount = group.summary.pending_count,
                acknowledgedCount = group.summary.acknowledged_count,
                rejectedCount = group.summary.rejected_count,
                latestUpdatedAt = group.summary.latest_updated_at,
                receiptGroupIds = listOf(group.receipt_group_id),
            ),
            items = group.items.map { item -> mapReceiptGroupItem(item) },
        )
    }

    fun mapReceiptGroupItem(item: NotificationReceipt): NotificationReceiptGroupItemUiModel {
        return NotificationReceiptGroupItemUiModel(
            userId = item.user_id,
            title = item.title,
            origin = mapOrigin(item.origin_type, item.origin_label),
            receiptGroupId = item.receipt_group_id,
            deliveryStatus = item.delivery_status,
            readStatus = item.read_status,
            receiptStatus = mapReceiptStatus(item.ack_status),
            ackNote = item.ack_note,
            updatedAt = item.updated_at,
        )
    }
}
