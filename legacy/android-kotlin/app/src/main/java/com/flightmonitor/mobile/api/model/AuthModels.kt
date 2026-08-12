package com.flightmonitor.mobile.api.model

data class LoginRequest(
    val username: String,
    val password: String,
)

data class RefreshTokenRequest(
    val refresh_token: String,
)

data class TokenResponse(
    val access_token: String,
    val token_type: String,
    val expires_in: Long,
    val refresh_token: String? = null,
    val sse_token: String? = null,
    val sse_expires_in: Long? = null,
    val session_secret: String? = null,
)

data class SseTokenResponse(
    val sse_token: String,
    val sse_expires_in: Long,
)

data class AuthAckResponse(
    val success: Boolean,
    val message: String? = null,
)

data class UserProfile(
    val id: String,
    val username: String,
    val is_admin: Boolean,
    val roles: List<String> = emptyList(),
    val permissions: List<String> = emptyList(),
    val display_name: String? = null,
    val job_title: String? = null,
    val effective_operator_name: String? = null,
    val effective_operator_label: String? = null,
    val operator_context_type: String? = null,
    val operator_context_id: String? = null,
)

data class OperatorContextUpdateRequest(
    val operator_name: String? = null,
)
