package com.flightmonitor.mobile.api

import com.flightmonitor.mobile.api.model.DispatchChatGroupListResponse
import com.flightmonitor.mobile.api.model.DispatchChatMarkReadRequest
import com.flightmonitor.mobile.api.model.DispatchChatMessage
import com.flightmonitor.mobile.api.model.DispatchChatMessageListResponse
import com.flightmonitor.mobile.api.model.DispatchChatReadResult
import com.flightmonitor.mobile.api.model.DispatchChatSendMessageRequest
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Query

interface DispatchChatApi {
    @GET("/api/v2/dispatch-chat/groups")
    suspend fun listGroups(
        @Query("status") status: String = "active",
        @Query("limit") limit: Int = 50,
        @Query("offset") offset: Int = 0,
    ): DispatchChatGroupListResponse

    @GET("/api/v2/dispatch-chat/groups/{groupId}/messages")
    suspend fun listMessages(
        @Path("groupId") groupId: String,
        @Query("limit") limit: Int = 50,
        @Query("before_seq") beforeSeq: Int? = null,
    ): DispatchChatMessageListResponse

    @POST("/api/v2/dispatch-chat/groups/{groupId}/messages")
    suspend fun sendMessage(
        @Path("groupId") groupId: String,
        @Body payload: DispatchChatSendMessageRequest,
    ): DispatchChatMessage

    @POST("/api/v2/dispatch-chat/groups/{groupId}/read")
    suspend fun markRead(
        @Path("groupId") groupId: String,
        @Body payload: DispatchChatMarkReadRequest,
    ): DispatchChatReadResult
}
