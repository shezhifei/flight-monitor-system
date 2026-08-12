package com.flightmonitor.mobile.network

import com.flightmonitor.mobile.session.TokenStorage
import okhttp3.Interceptor
import okhttp3.RequestBody
import okhttp3.Response
import okio.Buffer
import java.security.MessageDigest
import java.util.Locale
import java.util.UUID
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

class AntiReplayHeaderInterceptor(
    private val tokenStorage: TokenStorage,
) : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        val tokenBundle = tokenStorage.getTokenBundle()
        if (tokenBundle?.accessToken.isNullOrBlank() || tokenBundle?.sessionSecret.isNullOrBlank()) {
            return chain.proceed(request)
        }

        val timestamp = (System.currentTimeMillis() / 1000L).toString()
        val nonce = UUID.randomUUID().toString().replace("-", "")
        val bodyBytes = request.body.readBytesSafely()
        val bodyHash = sha256Hex(bodyBytes)
        val uri = request.url.encodedQuery?.takeIf { it.isNotBlank() }
            ?.let { "${request.url.encodedPath}?$it" }
            ?: request.url.encodedPath
        val signaturePayload = listOf(
            request.method.uppercase(Locale.ROOT),
            uri,
            timestamp,
            nonce,
            bodyHash,
        ).joinToString(":")
        val signature = hmacSha256Hex(
            secret = tokenBundle!!.sessionSecret!!,
            payload = signaturePayload,
        )

        val signedRequest = request.newBuilder()
            .header("X-Request-Timestamp", timestamp)
            .header("X-Request-Nonce", nonce)
            .header("X-Request-Body-SHA256", bodyHash)
            .header("X-Request-Signature", signature)
            .build()
        return chain.proceed(signedRequest)
    }

    private fun RequestBody?.readBytesSafely(): ByteArray {
        if (this == null) {
            return ByteArray(0)
        }
        val buffer = Buffer()
        writeTo(buffer)
        return buffer.readByteArray()
    }

    private fun sha256Hex(bytes: ByteArray): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(bytes)
        return digest.toHex()
    }

    private fun hmacSha256Hex(secret: String, payload: String): String {
        val mac = Mac.getInstance("HmacSHA256")
        val key = SecretKeySpec(secret.toByteArray(Charsets.UTF_8), "HmacSHA256")
        mac.init(key)
        return mac.doFinal(payload.toByteArray(Charsets.UTF_8)).toHex()
    }

    private fun ByteArray.toHex(): String {
        val builder = StringBuilder(size * 2)
        for (byte in this) {
            builder.append(((byte.toInt() ushr 4) and 0x0f).toString(16))
            builder.append((byte.toInt() and 0x0f).toString(16))
        }
        return builder.toString()
    }
}
