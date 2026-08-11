package com.flightmonitor.mobile.api

import com.flightmonitor.mobile.api.model.DispatchIssueReportRequest
import com.flightmonitor.mobile.api.model.DispatchOrderAcceptRequest
import com.flightmonitor.mobile.api.model.DispatchOrderCheckInRequest
import com.flightmonitor.mobile.api.model.DispatchOrderCheckOutRequest
import com.flightmonitor.mobile.api.model.DispatchOrderCompleteRequest
import com.flightmonitor.mobile.api.model.DispatchOrderEtaReportRequest
import com.flightmonitor.mobile.api.model.DispatchOrderItem
import com.flightmonitor.mobile.api.model.DispatchOrderStartRequest
import com.flightmonitor.mobile.api.model.DispatchSafetyChecklistItemResultRequest
import com.flightmonitor.mobile.api.model.DispatchSafetyChecklistRecord
import com.flightmonitor.mobile.api.model.DispatchSafetyChecklistStatus
import com.flightmonitor.mobile.api.model.DispatchSyncRequest
import com.flightmonitor.mobile.api.model.DispatchSyncResponse
import com.flightmonitor.mobile.api.model.GenericApiResponse
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Query

interface DispatchApi {
    @GET("/api/v2/dispatch/orders/my/assigned")
    suspend fun listMyOrders(
        @Query("status") status: String? = null,
    ): GenericApiResponse<List<DispatchOrderItem>>

    @POST("/api/v2/dispatch-orders/{orderId}/accept")
    suspend fun acceptOrder(
        @Path("orderId") orderId: String,
        @Body payload: DispatchOrderAcceptRequest,
    ): GenericApiResponse<Map<String, Any?>>

    @POST("/api/v2/dispatch-orders/{orderId}/checkin")
    suspend fun checkInOrder(
        @Path("orderId") orderId: String,
        @Body payload: DispatchOrderCheckInRequest,
    ): GenericApiResponse<Map<String, Any?>>

    @POST("/api/v2/dispatch-orders/{orderId}/checkout")
    suspend fun checkoutOrder(
        @Path("orderId") orderId: String,
        @Body payload: DispatchOrderCheckOutRequest,
    ): GenericApiResponse<Map<String, Any?>>

    @POST("/api/v2/dispatch-orders/{orderId}/start")
    suspend fun startOrder(
        @Path("orderId") orderId: String,
        @Body payload: DispatchOrderStartRequest,
    ): GenericApiResponse<Map<String, Any?>>

    @POST("/api/v2/dispatch-orders/{orderId}/complete")
    suspend fun completeOrder(
        @Path("orderId") orderId: String,
        @Body payload: DispatchOrderCompleteRequest,
    ): GenericApiResponse<Map<String, Any?>>

    @POST("/api/v2/dispatch-orders/{orderId}/eta-report")
    suspend fun reportEstimatedCompletion(
        @Path("orderId") orderId: String,
        @Body payload: DispatchOrderEtaReportRequest,
    ): GenericApiResponse<Map<String, Any?>>

    @POST("/api/v2/dispatch-orders/{orderId}/report-issue")
    suspend fun reportIssue(
        @Path("orderId") orderId: String,
        @Body payload: DispatchIssueReportRequest,
    ): GenericApiResponse<Map<String, Any?>>

    @POST("/api/v2/dispatch-orders/mobile/sync/actions")
    suspend fun syncActions(
        @Body payload: DispatchSyncRequest,
    ): GenericApiResponse<DispatchSyncResponse>

    @GET("/api/v2/dispatch-orders/{orderId}/safety-checklist")
    suspend fun getSafetyChecklistStatus(
        @Path("orderId") orderId: String,
    ): GenericApiResponse<DispatchSafetyChecklistStatus>

    @POST("/api/v2/dispatch-orders/{orderId}/safety-checklist/items/{itemCode}")
    suspend fun submitSafetyChecklistItem(
        @Path("orderId") orderId: String,
        @Path("itemCode") itemCode: String,
        @Body payload: DispatchSafetyChecklistItemResultRequest,
    ): GenericApiResponse<DispatchSafetyChecklistRecord>
}
