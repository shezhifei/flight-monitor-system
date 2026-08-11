package com.flightmonitor.mobile.network

import com.flightmonitor.mobile.BuildConfig
import com.flightmonitor.mobile.api.AuthApi
import com.flightmonitor.mobile.api.BusinessCaseApi
import com.flightmonitor.mobile.api.DispatchChatApi
import com.flightmonitor.mobile.api.DispatchApi
import com.flightmonitor.mobile.api.MobileApi
import com.flightmonitor.mobile.api.NotificationApi
import com.flightmonitor.mobile.api.ShiftHandoverApi
import com.flightmonitor.mobile.device.DeviceInfoProvider
import com.flightmonitor.mobile.session.TokenStorage
import okhttp3.OkHttpClient
import okhttp3.logging.HttpLoggingInterceptor
import retrofit2.Retrofit
import retrofit2.converter.gson.GsonConverterFactory
import java.util.concurrent.TimeUnit

class ApiFactory(
    private val tokenStorage: TokenStorage,
    private val deviceInfoProvider: DeviceInfoProvider,
) {
    private fun buildPublicClient(): OkHttpClient {
        return OkHttpClient.Builder()
            .connectTimeout(15, TimeUnit.SECONDS)
            .readTimeout(20, TimeUnit.SECONDS)
            .writeTimeout(20, TimeUnit.SECONDS)
            .addInterceptor(ClientSurfaceInterceptor())
            .addInterceptor(loggingInterceptor())
            .build()
    }

    private fun buildAuthClient(): OkHttpClient {
        return OkHttpClient.Builder()
            .connectTimeout(15, TimeUnit.SECONDS)
            .readTimeout(20, TimeUnit.SECONDS)
            .writeTimeout(20, TimeUnit.SECONDS)
            .addInterceptor(ClientSurfaceInterceptor())
            .addInterceptor(AuthHeaderInterceptor(tokenStorage))
            .addInterceptor(OperatorContextHeaderInterceptor(deviceInfoProvider))
            .addInterceptor(AntiReplayHeaderInterceptor(tokenStorage))
            .addInterceptor(loggingInterceptor())
            .build()
    }

    private fun loggingInterceptor(): HttpLoggingInterceptor {
        val level = if (BuildConfig.DEBUG) HttpLoggingInterceptor.Level.BODY else HttpLoggingInterceptor.Level.BASIC
        return HttpLoggingInterceptor().apply { this.level = level }
    }

    private fun retrofit(client: OkHttpClient): Retrofit {
        val baseUrl = BuildConfig.API_BASE_URL
        if (!BuildConfig.DEBUG && !baseUrl.startsWith("https://")) {
            throw IllegalStateException("Release builds must use HTTPS API_BASE_URL")
        }
        return Retrofit.Builder()
            .baseUrl(baseUrl)
            .addConverterFactory(GsonConverterFactory.create())
            .client(client)
            .build()
    }

    fun publicAuthApi(): AuthApi {
        return retrofit(buildPublicClient()).create(AuthApi::class.java)
    }

    fun authorizedAuthApi(): AuthApi {
        return retrofit(buildAuthClient()).create(AuthApi::class.java)
    }

    fun mobileApi(): MobileApi {
        return retrofit(buildAuthClient()).create(MobileApi::class.java)
    }

    fun dispatchApi(): DispatchApi {
        return retrofit(buildAuthClient()).create(DispatchApi::class.java)
    }

    fun dispatchChatApi(): DispatchChatApi {
        return retrofit(buildAuthClient()).create(DispatchChatApi::class.java)
    }

    fun notificationApi(): NotificationApi {
        return retrofit(buildAuthClient()).create(NotificationApi::class.java)
    }

    fun businessCaseApi(): BusinessCaseApi {
        return retrofit(buildAuthClient()).create(BusinessCaseApi::class.java)
    }

    fun shiftHandoverApi(): ShiftHandoverApi {
        return retrofit(buildAuthClient()).create(ShiftHandoverApi::class.java)
    }
}
