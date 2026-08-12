package com.flightmonitor.mobile.network

import android.net.Uri
import com.flightmonitor.mobile.BuildConfig
import com.flightmonitor.mobile.data.AuthRepository
import com.flightmonitor.mobile.session.TokenStorage
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.logging.HttpLoggingInterceptor
import okhttp3.sse.EventSource
import okhttp3.sse.EventSourceListener
import okhttp3.sse.EventSources
import java.util.UUID
import java.util.concurrent.TimeUnit

class SseStreamClient(
    private val authRepository: AuthRepository,
    private val tokenStorage: TokenStorage,
) {
    private val eventSourceFactory by lazy {
        EventSources.createFactory(
            OkHttpClient.Builder()
                .connectTimeout(15, TimeUnit.SECONDS)
                .readTimeout(0, TimeUnit.SECONDS)
                .writeTimeout(20, TimeUnit.SECONDS)
                .addInterceptor(loggingInterceptor())
                .build(),
        )
    }

    suspend fun connect(
        path: String,
        clientScope: String,
        onOpen: () -> Unit,
        onEvent: (event: String?, data: String) -> Unit,
        onClosed: () -> Unit,
        onFailure: (message: String) -> Unit,
    ): EventSource {
        authRepository.issueSseTokenIfNeeded()
        val tokenBundle = tokenStorage.getTokenBundle()
        val streamUrl = buildUrl(
            path = path,
            clientScope = clientScope,
            sseToken = tokenBundle?.sseToken,
            accessToken = tokenBundle?.accessToken,
        )
        val request = Request.Builder()
            .url(streamUrl)
            .get()
            .build()

        return eventSourceFactory.newEventSource(
            request,
            object : EventSourceListener() {
                override fun onOpen(eventSource: EventSource, response: okhttp3.Response) {
                    onOpen()
                }

                override fun onEvent(
                    eventSource: EventSource,
                    id: String?,
                    type: String?,
                    data: String,
                ) {
                    onEvent(type, data)
                }

                override fun onClosed(eventSource: EventSource) {
                    onClosed()
                }

                override fun onFailure(
                    eventSource: EventSource,
                    t: Throwable?,
                    response: okhttp3.Response?,
                ) {
                    val message = t?.message
                        ?: response?.message
                        ?: "SSE connection failure"
                    onFailure(message)
                }
            },
        )
    }

    private fun buildUrl(
        path: String,
        clientScope: String,
        sseToken: String?,
        accessToken: String?,
    ): String {
        val endpoint = "${BuildConfig.API_BASE_URL.trimEnd('/')}${path.trim()}"
        val builder = Uri.parse(endpoint).buildUpon()
        val clientInstanceId = "android_${clientScope}_${randomSuffix()}"
        builder.appendQueryParameter("client_instance_id", clientInstanceId)
        if (!sseToken.isNullOrBlank()) {
            builder.appendQueryParameter("sse_token", sseToken)
        } else if (!accessToken.isNullOrBlank()) {
            builder.appendQueryParameter("access_token", accessToken)
        }
        return builder.build().toString()
    }

    private fun randomSuffix(): String {
        return UUID.randomUUID().toString().replace("-", "").take(12)
    }

    private fun loggingInterceptor(): HttpLoggingInterceptor {
        val level = if (BuildConfig.DEBUG) {
            HttpLoggingInterceptor.Level.BASIC
        } else {
            HttpLoggingInterceptor.Level.NONE
        }
        return HttpLoggingInterceptor().apply { this.level = level }
    }
}
