package com.flightmonitor.mobile.api

import com.flightmonitor.mobile.api.model.AuthAckResponse
import com.flightmonitor.mobile.api.model.LoginRequest
import com.flightmonitor.mobile.api.model.OperatorContextUpdateRequest
import com.flightmonitor.mobile.api.model.RefreshTokenRequest
import com.flightmonitor.mobile.api.model.SseTokenResponse
import com.flightmonitor.mobile.api.model.TokenResponse
import com.flightmonitor.mobile.api.model.UserProfile
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.PUT

interface AuthApi {
    @POST("/api/v2/auth/login")
    suspend fun login(@Body payload: LoginRequest): TokenResponse

    @POST("/api/v2/auth/refresh")
    suspend fun refresh(@Body payload: RefreshTokenRequest): TokenResponse

    @GET("/api/v2/auth/me")
    suspend fun me(): UserProfile

    @PUT("/api/v2/auth/me/operator-context")
    suspend fun updateOperatorContext(@Body payload: OperatorContextUpdateRequest): UserProfile

    @POST("/api/v2/auth/logout")
    suspend fun logout(): AuthAckResponse

    @POST("/api/v2/auth/heartbeat")
    suspend fun heartbeat(): AuthAckResponse

    @POST("/api/v2/auth/sse-token")
    suspend fun issueSseToken(): SseTokenResponse
}
