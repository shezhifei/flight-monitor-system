package com.flightmonitor.mobile.api

import com.flightmonitor.mobile.api.model.BusinessCase
import com.flightmonitor.mobile.api.model.BusinessCaseAppendAcknowledgement
import com.flightmonitor.mobile.api.model.BusinessCaseAppendRequest
import com.flightmonitor.mobile.api.model.BusinessCaseCreateRequest
import com.flightmonitor.mobile.api.model.BusinessCaseEnvelope
import com.flightmonitor.mobile.api.model.BusinessCaseStatusUpdateRequest
import com.flightmonitor.mobile.api.model.BusinessCaseType
import com.flightmonitor.mobile.api.model.BusinessCaseWorkflowRunDetail
import com.flightmonitor.mobile.api.model.BusinessCaseWorkflowStartData
import com.flightmonitor.mobile.api.model.BusinessCaseWorkflowStartRequest
import com.flightmonitor.mobile.api.model.GenericApiResponse
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.PATCH
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Query

interface BusinessCaseApi {
    @GET("/api/v2/business-cases")
    suspend fun listBusinessCases(
        @Query("flight_id") flightId: String? = null,
        @Query("case_type") caseType: String? = null,
        @Query("status") status: String? = null,
    ): List<BusinessCaseEnvelope>

    @GET("/api/v2/business-cases/{caseId}")
    suspend fun getBusinessCase(
        @Path("caseId") caseId: String,
    ): GenericApiResponse<BusinessCase>

    @POST("/api/v2/business-cases")
    suspend fun createBusinessCase(
        @Body payload: BusinessCaseCreateRequest,
    ): GenericApiResponse<BusinessCase>

    @PATCH("/api/v2/business-cases/{caseId}/status")
    suspend fun updateBusinessCaseStatus(
        @Path("caseId") caseId: String,
        @Body payload: BusinessCaseStatusUpdateRequest,
    ): GenericApiResponse<BusinessCase>

    @POST("/api/v2/business-cases/{caseId}/appends")
    suspend fun appendBusinessCase(
        @Path("caseId") caseId: String,
        @Body payload: BusinessCaseAppendRequest,
    ): GenericApiResponse<BusinessCase>

    @POST("/api/v2/business-cases/{caseId}/appends/{appendId}/acknowledge")
    suspend fun acknowledgeAppend(
        @Path("caseId") caseId: String,
        @Path("appendId") appendId: String,
    ): GenericApiResponse<BusinessCaseAppendAcknowledgement>

    @GET("/api/v2/business-case-types")
    suspend fun listBusinessCaseTypes(
        @Query("active_only") activeOnly: Boolean = true,
    ): GenericApiResponse<List<BusinessCaseType>>

    @POST("/api/v2/business-case-workflows/{templateCode}/start")
    suspend fun startBusinessCaseWorkflow(
        @Path("templateCode") templateCode: String,
        @Body payload: BusinessCaseWorkflowStartRequest,
    ): GenericApiResponse<BusinessCaseWorkflowStartData>

    @GET("/api/v2/business_cases/{caseId}/workflow")
    suspend fun getWorkflowByCase(
        @Path("caseId") caseId: String,
    ): GenericApiResponse<BusinessCaseWorkflowRunDetail>
}
