package com.flightmonitor.mobile.api

import com.flightmonitor.mobile.api.model.GenericApiResponse
import com.flightmonitor.mobile.api.model.NotificationAcknowledgeRequest
import com.flightmonitor.mobile.api.model.NotificationListResponse
import com.flightmonitor.mobile.api.model.NotificationReceipt
import com.flightmonitor.mobile.api.model.NotificationReceiptGroup
import com.flightmonitor.mobile.api.model.NotificationUnreadCountResponse
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Query

interface NotificationApi {
    @GET("/api/v2/notifications")
    suspend fun listNotifications(
        @Query("unread_only") unreadOnly: Boolean = false,
        @Query("limit") limit: Int = 50,
        @Query("offset") offset: Int = 0,
    ): NotificationListResponse

    @GET("/api/v2/notifications/unread-count")
    suspend fun getUnreadCount(): NotificationUnreadCountResponse

    @POST("/api/v2/notifications/{notificationId}/read")
    suspend fun markRead(
        @Path("notificationId") notificationId: String,
    ): GenericApiResponse<Map<String, Any?>>

    @POST("/api/v2/notifications/{notificationId}/ack")
    suspend fun acknowledge(
        @Path("notificationId") notificationId: String,
        @Body payload: NotificationAcknowledgeRequest,
    ): GenericApiResponse<NotificationReceipt>

    @POST("/api/v2/notifications/read-all")
    suspend fun markAllRead(): GenericApiResponse<Map<String, Any?>>

    @GET("/api/v2/notifications/{notificationId}/receipts")
    suspend fun getReceipt(
        @Path("notificationId") notificationId: String,
    ): NotificationReceipt

    @GET("/api/v2/notifications/receipt-groups/{receiptGroupId}")
    suspend fun getReceiptGroup(
        @Path("receiptGroupId") receiptGroupId: String,
    ): NotificationReceiptGroup
}
