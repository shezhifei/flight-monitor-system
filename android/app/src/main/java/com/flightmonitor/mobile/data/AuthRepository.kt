package com.flightmonitor.mobile.data

import com.flightmonitor.mobile.api.AuthApi
import com.flightmonitor.mobile.api.model.LoginRequest
import com.flightmonitor.mobile.api.model.OperatorContextUpdateRequest
import com.flightmonitor.mobile.api.model.RefreshTokenRequest
import com.flightmonitor.mobile.api.model.UserProfile
import com.flightmonitor.mobile.session.TokenStorage
import retrofit2.HttpException

class AuthRepository(
    private val publicAuthApi: AuthApi,
    private val authorizedAuthApi: AuthApi,
    private val tokenStorage: TokenStorage,
) {
    suspend fun login(username: String, password: String): UserProfile {
        val tokenPayload = publicAuthApi.login(
            payload = LoginRequest(
                username = username.trim(),
                password = password,
            ),
        )
        tokenStorage.updateFromLogin(
            accessToken = tokenPayload.access_token,
            refreshToken = tokenPayload.refresh_token,
            tokenType = tokenPayload.token_type,
            expiresInSeconds = tokenPayload.expires_in,
            sseToken = tokenPayload.sse_token,
            sseExpiresInSeconds = tokenPayload.sse_expires_in,
            sessionSecret = tokenPayload.session_secret,
        )
        val profile = authorizedAuthApi.me()
        issueSseTokenIfNeeded()
        return profile
    }

    suspend fun ensureSession(): Boolean {
        val current = tokenStorage.getTokenBundle() ?: return false
        val now = System.currentTimeMillis() / 1000L
        if (current.isAccessExpired(now) || !current.hasSessionSecret()) {
            val refreshed = refreshAccessToken()
            if (!refreshed) {
                return false
            }
        }
        return runCatching {
            authorizedAuthApi.me()
            issueSseTokenIfNeeded()
            true
        }.recoverCatching { error ->
            if (error is HttpException && error.code() == 401) {
                if (!refreshAccessToken()) {
                    return@recoverCatching false
                }
                authorizedAuthApi.me()
                issueSseTokenIfNeeded()
                true
            } else {
                false
            }
        }.getOrDefault(false)
    }

    suspend fun refreshAccessToken(): Boolean {
        val refreshToken = tokenStorage.getTokenBundle()?.refreshToken?.trim().orEmpty()
        if (refreshToken.isEmpty()) {
            tokenStorage.clear()
            return false
        }
        val payload = runCatching {
            publicAuthApi.refresh(RefreshTokenRequest(refresh_token = refreshToken))
        }.getOrNull() ?: return false

        tokenStorage.updateAccessToken(
            accessToken = payload.access_token,
            tokenType = payload.token_type,
            expiresInSeconds = payload.expires_in,
            sseToken = payload.sse_token,
            sseExpiresInSeconds = payload.sse_expires_in,
            sessionSecret = payload.session_secret,
        )
        return true
    }

    suspend fun me(): UserProfile {
        return authorizedAuthApi.me()
    }

    suspend fun updateOperatorContext(operatorName: String?): UserProfile {
        return authorizedAuthApi.updateOperatorContext(
            OperatorContextUpdateRequest(operator_name = operatorName?.trim()),
        )
    }

    suspend fun issueSseTokenIfNeeded() {
        val current = tokenStorage.getTokenBundle() ?: return
        val now = System.currentTimeMillis() / 1000L
        val sseExpireAt = current.sseExpireAtEpochSeconds ?: 0L
        if (current.sseToken.isNullOrBlank() || now >= sseExpireAt) {
            val payload = runCatching { authorizedAuthApi.issueSseToken() }.getOrNull() ?: return
            tokenStorage.updateSseToken(
                sseToken = payload.sse_token,
                sseExpiresInSeconds = payload.sse_expires_in,
            )
        }
    }

    suspend fun heartbeat() {
        authorizedAuthApi.heartbeat()
    }

    suspend fun logout() {
        runCatching { authorizedAuthApi.logout() }
        tokenStorage.clear()
    }

    fun hasLocalSession(): Boolean = tokenStorage.getTokenBundle() != null
}
