package com.flightmonitor.mobile.network

import okhttp3.Interceptor
import okhttp3.Response

/**
 * Declares the native token-delivery surface so the API may include
 * refresh/session secrets in JSON for encrypted local storage.
 *
 * Transport/protocol selection only — not a trusted identity boundary.
 */
class ClientSurfaceInterceptor : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
            .newBuilder()
            .header(HEADER_NAME, HEADER_VALUE)
            .build()
        return chain.proceed(request)
    }

    companion object {
        const val HEADER_NAME = "X-Client-Surface"
        const val HEADER_VALUE = "native"
    }
}
