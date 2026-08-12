package com.flightmonitor.mobile.api

import com.flightmonitor.mobile.api.model.GenericApiResponse
import com.flightmonitor.mobile.api.model.MobileDeviceHeartbeatRequest
import com.flightmonitor.mobile.api.model.MobileDeviceRegisterRequest
import com.flightmonitor.mobile.api.model.MobileDeviceResponse
import com.flightmonitor.mobile.api.model.MobileOperationsEventsResponse
import com.flightmonitor.mobile.api.model.MobileUploadAsset
import com.flightmonitor.mobile.api.model.MobileWorkbenchResponse
import okhttp3.MultipartBody
import okhttp3.RequestBody
import retrofit2.http.Body
import retrofit2.http.DELETE
import retrofit2.http.GET
import retrofit2.http.Multipart
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Part
import retrofit2.http.Query

interface MobileApi {
    @GET("/api/v2/mobile/workbench")
    suspend fun getWorkbench(
        @Query("pending_sync_action_count") pendingSyncActionCount: Int = 0,
        @Query("max_orders") maxOrders: Int = 50,
    ): GenericApiResponse<MobileWorkbenchResponse>

    @GET("/api/v2/mobile/operations/events")
    suspend fun getOperationsEvents(
        @Query("limit") limit: Int = 120,
    ): GenericApiResponse<MobileOperationsEventsResponse>

    @Multipart
    @POST("/api/v2/mobile/uploads")
    suspend fun uploadAsset(
        @Part file: MultipartBody.Part,
        @Part("category") category: RequestBody,
    ): GenericApiResponse<MobileUploadAsset>

    @POST("/api/v2/mobile/devices/register")
    suspend fun registerDevice(
        @Body payload: MobileDeviceRegisterRequest,
    ): GenericApiResponse<MobileDeviceResponse>

    @POST("/api/v2/mobile/devices/{deviceId}/heartbeat")
    suspend fun heartbeat(
        @Path("deviceId") deviceId: String,
        @Body payload: MobileDeviceHeartbeatRequest,
    ): GenericApiResponse<MobileDeviceResponse>

    @DELETE("/api/v2/mobile/devices/{deviceId}")
    suspend fun unregisterDevice(
        @Path("deviceId") deviceId: String,
    ): GenericApiResponse<Map<String, Any>>
}
