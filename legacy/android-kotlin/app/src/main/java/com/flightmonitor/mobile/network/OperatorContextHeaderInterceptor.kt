package com.flightmonitor.mobile.network

import com.flightmonitor.mobile.device.DeviceInfoProvider
import okhttp3.Interceptor
import okhttp3.Response

class OperatorContextHeaderInterceptor(
    private val deviceInfoProvider: DeviceInfoProvider,
) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request().newBuilder()
            .header("X-Operator-Context-Type", "mobile_device")
            .header("X-Operator-Context-Id", deviceInfoProvider.deviceId())
            .build()
        return chain.proceed(request)
    }
}
