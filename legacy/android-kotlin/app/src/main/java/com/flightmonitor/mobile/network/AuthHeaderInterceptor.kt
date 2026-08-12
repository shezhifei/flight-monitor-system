package com.flightmonitor.mobile.network

import com.flightmonitor.mobile.session.TokenStorage
import okhttp3.Interceptor
import okhttp3.Response

class AuthHeaderInterceptor(
    private val tokenStorage: TokenStorage,
) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        val token = tokenStorage.getTokenBundle()?.accessToken
        if (token.isNullOrBlank()) {
            return chain.proceed(request)
        }
        val authorized = request.newBuilder()
            .header("Authorization", "Bearer $token")
            .build()
        return chain.proceed(authorized)
    }
}
