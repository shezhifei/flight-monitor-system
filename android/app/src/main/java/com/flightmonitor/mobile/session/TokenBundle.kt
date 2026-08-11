package com.flightmonitor.mobile.session

data class TokenBundle(
    val accessToken: String,
    val refreshToken: String?,
    val tokenType: String,
    val accessExpireAtEpochSeconds: Long,
    val sseToken: String?,
    val sseExpireAtEpochSeconds: Long?,
    val sessionSecret: String?,
) {
    fun isAccessExpired(nowEpochSeconds: Long): Boolean {
        return nowEpochSeconds >= accessExpireAtEpochSeconds
    }

    fun hasSessionSecret(): Boolean = !sessionSecret.isNullOrBlank()
}
