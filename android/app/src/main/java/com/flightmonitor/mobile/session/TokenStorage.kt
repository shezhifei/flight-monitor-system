package com.flightmonitor.mobile.session

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import android.util.Log
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import kotlin.math.max

/**
 * Persists access/refresh/SSE tokens and session_secret exclusively via
 * EncryptedSharedPreferences (AES-256 + Android Keystore).
 *
 * Fail-closed: if encrypted storage cannot be opened, construction throws —
 * never fall back to plaintext SharedPreferences.
 */
class TokenStorage(context: Context) {
    private val prefs: SharedPreferences = openEncryptedPreferences(context.applicationContext)

    fun getTokenBundle(): TokenBundle? {
        val accessToken = prefs.getString(KEY_ACCESS_TOKEN, null)?.trim().orEmpty()
        if (accessToken.isEmpty()) {
            return null
        }
        val tokenType = prefs.getString(KEY_TOKEN_TYPE, "bearer").orEmpty()
        val accessExpireAt = prefs.getLong(KEY_ACCESS_EXPIRE_AT, 0L)
        val refreshToken = prefs.getString(KEY_REFRESH_TOKEN, null)
        val sseToken = prefs.getString(KEY_SSE_TOKEN, null)
        val sseExpireAt = prefs.getLong(KEY_SSE_EXPIRE_AT, 0L).takeIf { it > 0L }
        val sessionSecret = prefs.getString(KEY_SESSION_SECRET, null)?.trim().orEmpty()
        return TokenBundle(
            accessToken = accessToken,
            refreshToken = refreshToken,
            tokenType = tokenType,
            accessExpireAtEpochSeconds = accessExpireAt,
            sseToken = sseToken,
            sseExpireAtEpochSeconds = sseExpireAt,
            sessionSecret = sessionSecret.ifBlank { null },
        )
    }

    fun updateFromLogin(
        accessToken: String,
        refreshToken: String?,
        tokenType: String,
        expiresInSeconds: Long,
        sseToken: String?,
        sseExpiresInSeconds: Long?,
        sessionSecret: String?,
    ) {
        val nowSeconds = System.currentTimeMillis() / 1000L
        val expireAt = nowSeconds + max(1L, expiresInSeconds)
        prefs.edit()
            .putString(KEY_ACCESS_TOKEN, accessToken)
            .putString(KEY_REFRESH_TOKEN, refreshToken)
            .putString(KEY_TOKEN_TYPE, tokenType)
            .putLong(KEY_ACCESS_EXPIRE_AT, expireAt)
            .putString(KEY_SSE_TOKEN, sseToken)
            .putLong(KEY_SSE_EXPIRE_AT, if (sseExpiresInSeconds != null) nowSeconds + max(1L, sseExpiresInSeconds) else 0L)
            .putString(KEY_SESSION_SECRET, sessionSecret)
            .apply()
    }

    fun updateAccessToken(
        accessToken: String,
        tokenType: String,
        expiresInSeconds: Long,
        sseToken: String?,
        sseExpiresInSeconds: Long?,
        sessionSecret: String?,
    ) {
        val nowSeconds = System.currentTimeMillis() / 1000L
        val expireAt = nowSeconds + max(1L, expiresInSeconds)
        prefs.edit()
            .putString(KEY_ACCESS_TOKEN, accessToken)
            .putString(KEY_TOKEN_TYPE, tokenType)
            .putLong(KEY_ACCESS_EXPIRE_AT, expireAt)
            .putString(KEY_SSE_TOKEN, sseToken)
            .putLong(KEY_SSE_EXPIRE_AT, if (sseExpiresInSeconds != null) nowSeconds + max(1L, sseExpiresInSeconds) else 0L)
            .putString(KEY_SESSION_SECRET, sessionSecret)
            .apply()
    }

    fun updateSseToken(
        sseToken: String,
        sseExpiresInSeconds: Long,
    ) {
        val nowSeconds = System.currentTimeMillis() / 1000L
        prefs.edit()
            .putString(KEY_SSE_TOKEN, sseToken)
            .putLong(KEY_SSE_EXPIRE_AT, nowSeconds + max(1L, sseExpiresInSeconds))
            .apply()
    }

    fun clear() {
        prefs.edit().clear().apply()
    }

    companion object {
        private const val TAG = "TokenStorage"
        const val PREFS_NAME_ENCRYPTED = "mobile_auth_tokens_encrypted"
        /** Legacy plaintext prefs name used by pre-security builds; wiped on open. */
        const val PREFS_NAME_LEGACY_PLAINTEXT = "mobile_auth_tokens"
        private const val KEY_ACCESS_TOKEN = "access_token"
        private const val KEY_REFRESH_TOKEN = "refresh_token"
        private const val KEY_TOKEN_TYPE = "token_type"
        private const val KEY_ACCESS_EXPIRE_AT = "access_expire_at"
        private const val KEY_SSE_TOKEN = "sse_token"
        private const val KEY_SSE_EXPIRE_AT = "sse_expire_at"
        private const val KEY_SESSION_SECRET = "session_secret"

        /**
         * Securely wipe any historical plaintext token store so secrets cannot
         * remain on disk after upgrade. Does not migrate tokens into encrypted
         * storage (force re-login).
         *
         * Fail-closed: if the legacy store contains data and clear/commit fails,
         * throws [SecureTokenStorageException] rather than continuing with secrets
         * still on disk.
         */
        fun wipeLegacyPlaintextTokens(context: Context) {
            val appContext = context.applicationContext
            val legacy = try {
                appContext.getSharedPreferences(PREFS_NAME_LEGACY_PLAINTEXT, Context.MODE_PRIVATE)
            } catch (error: Exception) {
                throw SecureTokenStorageException(
                    "Unable to open legacy plaintext token store for wipe",
                    error,
                )
            }
            val hadSecrets = legacy.all.isNotEmpty()
            val cleared = try {
                legacy.edit().clear().commit()
            } catch (error: Exception) {
                throw SecureTokenStorageException(
                    "Failed to clear legacy plaintext token store",
                    error,
                )
            }
            if (hadSecrets && !cleared) {
                throw SecureTokenStorageException(
                    "Legacy plaintext token store still contains data after clear",
                )
            }
            // File deletion is best-effort once content is empty; secrets must not remain.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
                try {
                    appContext.deleteSharedPreferences(PREFS_NAME_LEGACY_PLAINTEXT)
                } catch (error: Exception) {
                    Log.w(TAG, "Legacy plaintext prefs file delete failed after successful clear", error)
                }
            }
        }

        private fun openEncryptedPreferences(context: Context): SharedPreferences {
            wipeLegacyPlaintextTokens(context)
            try {
                val masterKey = MasterKey.Builder(context)
                    .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
                    .build()
                return EncryptedSharedPreferences.create(
                    context,
                    PREFS_NAME_ENCRYPTED,
                    masterKey,
                    EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                    EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
                )
            } catch (error: Exception) {
                Log.e(TAG, "EncryptedSharedPreferences unavailable; refusing plaintext fallback", error)
                throw SecureTokenStorageException(
                    "Encrypted token storage is required and could not be opened",
                    error,
                )
            }
        }
    }
}

/** Thrown when Keystore/EncryptedSharedPreferences cannot be used or legacy wipe fails. */
class SecureTokenStorageException(
    message: String,
    cause: Throwable? = null,
) : RuntimeException(message, cause)
