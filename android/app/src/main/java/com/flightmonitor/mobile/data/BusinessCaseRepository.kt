package com.flightmonitor.mobile.data

import com.flightmonitor.mobile.api.BusinessCaseApi
import com.flightmonitor.mobile.api.model.BusinessCase
import com.flightmonitor.mobile.api.model.BusinessCaseAppendAcknowledgement
import com.flightmonitor.mobile.api.model.BusinessCaseAppendRequest
import com.flightmonitor.mobile.api.model.BusinessCaseCreateRequest
import com.flightmonitor.mobile.api.model.BusinessCaseType
import com.flightmonitor.mobile.api.model.BusinessCaseWorkflowRunDetail
import com.flightmonitor.mobile.api.model.BusinessCaseWorkflowStartData
import com.flightmonitor.mobile.api.model.BusinessCaseWorkflowStartRequest
import com.flightmonitor.mobile.api.model.GenericApiResponse
import com.google.gson.GsonBuilder
import retrofit2.HttpException

class BusinessCaseRepository(
    private val api: BusinessCaseApi,
) {
    private val gson = GsonBuilder().setPrettyPrinting().create()

    suspend fun listBusinessCases(
        flightId: String? = null,
        caseType: String? = null,
        status: String? = null,
    ): List<BusinessCase> {
        return api.listBusinessCases(
            flightId = flightId,
            caseType = caseType,
            status = status,
        ).mapNotNull { envelope ->
            if (envelope.success) envelope.data else null
        }.sortedByDescending { it.created_at }
    }

    suspend fun getBusinessCase(caseId: String): BusinessCase {
        return unwrap(api.getBusinessCase(caseId))
    }

    suspend fun listCaseTypes(activeOnly: Boolean = true): List<BusinessCaseType> {
        return unwrap(api.listBusinessCaseTypes(activeOnly)).sortedBy { it.code }
    }

    suspend fun createBusinessCase(payload: BusinessCaseCreateRequest): BusinessCase {
        return unwrap(api.createBusinessCase(payload))
    }

    suspend fun createAndStartWorkflow(
        templateCode: String,
        payload: BusinessCaseWorkflowStartRequest,
    ): BusinessCaseWorkflowStartData {
        return unwrap(api.startBusinessCaseWorkflow(templateCode, payload))
    }

    suspend fun appendBusinessCase(
        caseId: String,
        content: String,
        mentionUserIds: List<String>,
    ): BusinessCase {
        return unwrap(
            api.appendBusinessCase(
                caseId = caseId,
                payload = BusinessCaseAppendRequest(
                    content = content,
                    mention_user_ids = mentionUserIds,
                ),
            ),
        )
    }

    suspend fun acknowledgeAppend(
        caseId: String,
        appendId: String,
    ): BusinessCaseAppendAcknowledgement {
        return unwrap(api.acknowledgeAppend(caseId, appendId))
    }

    suspend fun updateStatus(
        caseId: String,
        status: String,
    ): BusinessCase {
        return unwrap(
            api.updateBusinessCaseStatus(
                caseId = caseId,
                payload = com.flightmonitor.mobile.api.model.BusinessCaseStatusUpdateRequest(status),
            ),
        )
    }

    suspend fun getWorkflowByCase(caseId: String): BusinessCaseWorkflowRunDetail? {
        return try {
            unwrap(api.getWorkflowByCase(caseId))
        } catch (error: HttpException) {
            if (error.code() == 404) null else throw error
        }
    }

    fun prettyJson(value: Any?): String {
        return gson.toJson(value)
    }

    private fun <T> unwrap(response: GenericApiResponse<T>): T {
        if (response.success && response.data != null) {
            return response.data
        }
        throw IllegalStateException(response.message ?: "接口返回为空")
    }
}
