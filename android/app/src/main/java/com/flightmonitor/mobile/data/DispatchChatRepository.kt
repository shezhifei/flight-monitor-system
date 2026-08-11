package com.flightmonitor.mobile.data

import com.flightmonitor.mobile.api.DispatchChatApi
import com.flightmonitor.mobile.api.model.DispatchChatGroupListResponse
import com.flightmonitor.mobile.api.model.DispatchChatMarkReadRequest
import com.flightmonitor.mobile.api.model.DispatchChatMessage
import com.flightmonitor.mobile.api.model.DispatchChatMessageListResponse
import com.flightmonitor.mobile.api.model.DispatchChatReadResult
import com.flightmonitor.mobile.api.model.DispatchChatSendMessageRequest
import com.flightmonitor.mobile.network.SseStreamClient
import okhttp3.sse.EventSource

class DispatchChatRepository(
    private val dispatchChatApi: DispatchChatApi,
    private val sseStreamClient: SseStreamClient,
) {
    suspend fun listGroups(
        status: String = "all",
        limit: Int = 120,
        offset: Int = 0,
    ): DispatchChatGroupListResponse {
        return dispatchChatApi.listGroups(
            status = status,
            limit = limit,
            offset = offset,
        )
    }

    suspend fun listMessages(
        groupId: String,
        limit: Int = 50,
        beforeSeq: Int? = null,
    ): DispatchChatMessageListResponse {
        return dispatchChatApi.listMessages(
            groupId = groupId,
            limit = limit,
            beforeSeq = beforeSeq,
        )
    }

    suspend fun sendMessage(
        groupId: String,
        content: String,
        atAll: Boolean = false,
    ): DispatchChatMessage {
        return dispatchChatApi.sendMessage(
            groupId = groupId,
            payload = DispatchChatSendMessageRequest(
                content = content,
                at_all = atAll,
            ),
        )
    }

    suspend fun markRead(
        groupId: String,
        readSeq: Int? = null,
    ): DispatchChatReadResult {
        return dispatchChatApi.markRead(
            groupId = groupId,
            payload = DispatchChatMarkReadRequest(read_seq = readSeq),
        )
    }

    suspend fun connectStream(
        onOpen: () -> Unit,
        onEvent: (event: String?, data: String) -> Unit,
        onClosed: () -> Unit,
        onFailure: (message: String) -> Unit,
    ): EventSource {
        return sseStreamClient.connect(
            path = "/api/v2/dispatch-chat/stream",
            clientScope = "dispatch_chat",
            onOpen = onOpen,
            onEvent = onEvent,
            onClosed = onClosed,
            onFailure = onFailure,
        )
    }
}
