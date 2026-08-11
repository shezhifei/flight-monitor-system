package com.flightmonitor.mobile.data

import com.flightmonitor.mobile.api.DispatchApi
import com.flightmonitor.mobile.api.model.DispatchSafetyChecklistRecord
import com.flightmonitor.mobile.api.model.DispatchSafetyChecklistItemResultRequest
import com.flightmonitor.mobile.api.model.DispatchSafetyChecklistStatus

class DispatchSafetyRepository(
    private val dispatchApi: DispatchApi,
) {
    suspend fun getSafetyStatus(orderId: String): DispatchSafetyChecklistStatus {
        return dispatchApi.getSafetyChecklistStatus(orderId = orderId).data
            ?: throw IllegalStateException("安全门禁状态返回为空")
    }

    suspend fun submitItemResult(
        orderId: String,
        itemCode: String,
        result: String,
        note: String? = null,
    ): DispatchSafetyChecklistRecord {
        return dispatchApi.submitSafetyChecklistItem(
            orderId = orderId,
            itemCode = itemCode,
            payload = DispatchSafetyChecklistItemResultRequest(
                result = result,
                note = note,
            ),
        ).data ?: throw IllegalStateException("安全门禁提交结果返回为空")
    }
}
