package com.flightmonitor.mobile.api

import com.flightmonitor.mobile.api.model.GenericApiResponse
import com.flightmonitor.mobile.api.model.ShiftHandover
import com.flightmonitor.mobile.api.model.ShiftHandoverItem
import com.flightmonitor.mobile.api.model.ShiftHandoverItemAcknowledgeRequest
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Query

interface ShiftHandoverApi {
    @GET("/api/v2/shift-handovers")
    suspend fun listHandovers(
        @Query("shift_date") shiftDate: String? = null,
        @Query("shift_code") shiftCode: String? = null,
        @Query("status") status: String? = null,
        @Query("from_user_id") fromUserId: String? = null,
        @Query("to_user_id") toUserId: String? = null,
        @Query("limit") limit: Int = 50,
        @Query("offset") offset: Int = 0,
    ): List<ShiftHandover>

    @GET("/api/v2/shift-handovers/{handoverId}")
    suspend fun getHandover(
        @Path("handoverId") handoverId: String,
    ): ShiftHandover

    @POST("/api/v2/shift-handovers/{handoverId}/items/{itemId}/ack")
    suspend fun acknowledgeItem(
        @Path("handoverId") handoverId: String,
        @Path("itemId") itemId: String,
        @Body payload: ShiftHandoverItemAcknowledgeRequest,
    ): ShiftHandoverItem

    @POST("/api/v2/shift-handovers/{handoverId}/ack")
    suspend fun acknowledgeHandover(
        @Path("handoverId") handoverId: String,
    ): GenericApiResponse<ShiftHandover>
}
