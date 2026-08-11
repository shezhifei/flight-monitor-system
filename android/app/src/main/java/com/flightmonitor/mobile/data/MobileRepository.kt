package com.flightmonitor.mobile.data

import com.flightmonitor.mobile.api.MobileApi
import com.flightmonitor.mobile.api.model.MobileDeviceHeartbeatRequest
import com.flightmonitor.mobile.api.model.MobileDeviceRegisterRequest
import com.flightmonitor.mobile.api.model.MobileOperationsEventsResponse
import com.flightmonitor.mobile.api.model.MobileWorkbenchResponse
import com.flightmonitor.mobile.device.DeviceInfoProvider
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.MultipartBody
import okhttp3.RequestBody.Companion.toRequestBody

class MobileRepository(
    private val mobileApi: MobileApi,
    private val deviceInfoProvider: DeviceInfoProvider,
) {
    private val deviceId: String by lazy { deviceInfoProvider.deviceId() }

    suspend fun loadWorkbench(pendingSyncActionCount: Int = 0): MobileWorkbenchResponse? {
        val response = mobileApi.getWorkbench(
            pendingSyncActionCount = pendingSyncActionCount,
            maxOrders = 50,
        )
        return response.data
    }

    suspend fun loadOperationEvents(limit: Int = 120): MobileOperationsEventsResponse? {
        val response = mobileApi.getOperationsEvents(limit = limit)
        return response.data
    }

    suspend fun uploadDispatchIssueAttachment(
        fileName: String,
        contentType: String?,
        bytes: ByteArray,
    ): String? {
        val fileBody = bytes.toRequestBody(contentType?.toMediaTypeOrNull())
        val filePart = MultipartBody.Part.createFormData("file", fileName, fileBody)
        val categoryBody = "dispatch_issue".toRequestBody("text/plain".toMediaTypeOrNull())
        val response = mobileApi.uploadAsset(
            file = filePart,
            category = categoryBody,
        )
        return response.data?.attachment_url
    }

    suspend fun registerCurrentDevice(pushToken: String? = null, pushChannel: String = "none"): Boolean {
        val request = MobileDeviceRegisterRequest(
            device_id = deviceId,
            platform = "android",
            push_channel = pushChannel,
            push_token = pushToken,
            app_version = deviceInfoProvider.appVersionName(),
            os_version = deviceInfoProvider.osVersionName(),
            device_model = deviceInfoProvider.model(),
            manufacturer = deviceInfoProvider.manufacturer(),
            metadata = mapOf(
                "api_level" to android.os.Build.VERSION.SDK_INT,
            ),
        )
        return runCatching { mobileApi.registerDevice(request) }.getOrNull()?.success == true
    }

    suspend fun sendHeartbeat(sseReconnectCount: Int = 0): Boolean {
        val request = MobileDeviceHeartbeatRequest(
            network_status = deviceInfoProvider.networkStatus(),
            battery_level = deviceInfoProvider.batteryLevel(),
            metadata = mapOf(
                "sse_reconnect_count" to sseReconnectCount,
            ),
        )
        return runCatching {
            mobileApi.heartbeat(deviceId = deviceId, payload = request)
        }.getOrNull()?.success == true
    }

    suspend fun unregisterCurrentDevice(): Boolean {
        return runCatching { mobileApi.unregisterDevice(deviceId = deviceId) }.getOrNull()?.success == true
    }
}
