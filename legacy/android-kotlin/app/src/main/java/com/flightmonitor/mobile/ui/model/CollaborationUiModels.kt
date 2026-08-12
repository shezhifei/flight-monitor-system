package com.flightmonitor.mobile.ui.model

enum class OriginKind {
    WORKFLOW,
    MANUAL,
}

data class OriginUiModel(
    val kind: OriginKind,
    val label: String,
)

enum class ReceiptStatusKind {
    PENDING,
    ACKNOWLEDGED,
    REJECTED,
}

data class ReceiptStatusUiModel(
    val kind: ReceiptStatusKind,
    val label: String,
)

data class ReceiptSummaryUiModel(
    val totalCount: Int,
    val pendingCount: Int,
    val acknowledgedCount: Int,
    val rejectedCount: Int,
    val latestUpdatedAt: String?,
    val receiptGroupIds: List<String>,
)

data class NotificationActionUiState(
    val canSubmit: Boolean,
    val rejectionNoteRequired: Boolean,
    val finalStatusLabel: String,
)

data class ChatComposerUiState(
    val canSend: Boolean,
    val readOnlyHint: String?,
)

data class NotificationReceiptGroupItemUiModel(
    val userId: String,
    val title: String?,
    val origin: OriginUiModel,
    val receiptGroupId: String?,
    val deliveryStatus: String,
    val readStatus: String,
    val receiptStatus: ReceiptStatusUiModel,
    val ackNote: String?,
    val updatedAt: String,
)

data class NotificationReceiptGroupUiModel(
    val receiptGroupId: String,
    val title: String?,
    val origin: OriginUiModel,
    val receiptRequired: Boolean,
    val summary: ReceiptSummaryUiModel,
    val items: List<NotificationReceiptGroupItemUiModel>,
)
