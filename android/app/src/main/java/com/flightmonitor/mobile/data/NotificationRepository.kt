package com.flightmonitor.mobile.data

import com.flightmonitor.mobile.api.NotificationApi
import com.flightmonitor.mobile.api.model.NotificationAcknowledgeRequest
import com.flightmonitor.mobile.api.model.NotificationItem
import com.flightmonitor.mobile.api.model.NotificationReceipt
import com.flightmonitor.mobile.api.model.NotificationReceiptGroup
import com.flightmonitor.mobile.network.SseStreamClient
import com.flightmonitor.mobile.ui.model.CollaborationUiMapper
import okhttp3.sse.EventSource

class NotificationRepository(
    private val notificationApi: NotificationApi,
    private val sseStreamClient: SseStreamClient,
) {
    suspend fun listNotifications(
        unreadOnly: Boolean = false,
        limit: Int = 50,
        offset: Int = 0,
    ): List<NotificationItem> {
        return notificationApi.listNotifications(
            unreadOnly = unreadOnly,
            limit = limit,
            offset = offset,
        ).items
    }

    suspend fun unreadCount(): Int {
        return notificationApi.getUnreadCount().unread_count
    }

    suspend fun markRead(notificationId: String): Boolean {
        return notificationApi.markRead(notificationId = notificationId).success
    }

    suspend fun acknowledge(
        notificationId: String,
        action: String,
        note: String? = null,
    ): NotificationReceipt? {
        CollaborationUiMapper.validateAcknowledgement(action, note)?.let { error ->
            throw IllegalArgumentException(error)
        }
        val response = notificationApi.acknowledge(
            notificationId = notificationId,
            payload = NotificationAcknowledgeRequest(
                action = action,
                note = note,
            ),
        )
        return if (response.success) response.data else null
    }

    suspend fun markAllRead(): Boolean {
        return notificationApi.markAllRead().success
    }

    suspend fun getReceipt(notificationId: String): NotificationReceipt {
        return notificationApi.getReceipt(notificationId)
    }

    suspend fun getReceiptGroup(receiptGroupId: String): NotificationReceiptGroup {
        return notificationApi.getReceiptGroup(receiptGroupId)
    }

    suspend fun connectStream(
        onOpen: () -> Unit,
        onEvent: (event: String?, data: String) -> Unit,
        onClosed: () -> Unit,
        onFailure: (message: String) -> Unit,
    ): EventSource {
        return sseStreamClient.connect(
            path = "/api/v2/notifications/stream",
            clientScope = "notifications",
            onOpen = onOpen,
            onEvent = onEvent,
            onClosed = onClosed,
            onFailure = onFailure,
        )
    }
}
