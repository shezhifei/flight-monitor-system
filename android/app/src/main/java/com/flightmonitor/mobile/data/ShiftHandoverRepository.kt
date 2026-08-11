package com.flightmonitor.mobile.data

import com.flightmonitor.mobile.api.ShiftHandoverApi
import com.flightmonitor.mobile.api.model.ShiftHandover
import com.flightmonitor.mobile.api.model.ShiftHandoverItem
import com.flightmonitor.mobile.api.model.ShiftHandoverItemAcknowledgeRequest

class ShiftHandoverRepository(
    private val api: ShiftHandoverApi,
) {
    suspend fun listHandovers(
        toUserId: String? = null,
        status: String? = null,
        limit: Int = 50,
        offset: Int = 0,
    ): List<ShiftHandover> {
        return api.listHandovers(
            toUserId = toUserId,
            status = status,
            limit = limit,
            offset = offset,
        )
    }

    suspend fun getHandover(handoverId: String): ShiftHandover {
        return api.getHandover(handoverId)
    }

    suspend fun acknowledgeItem(
        handoverId: String,
        itemId: String,
        acknowledged: Boolean = true,
    ): ShiftHandoverItem {
        return api.acknowledgeItem(
            handoverId = handoverId,
            itemId = itemId,
            payload = ShiftHandoverItemAcknowledgeRequest(acknowledged = acknowledged),
        )
    }

    suspend fun acknowledgeHandover(handoverId: String): ShiftHandover? {
        val response = api.acknowledgeHandover(handoverId = handoverId)
        return if (response.success) response.data else null
    }
}
