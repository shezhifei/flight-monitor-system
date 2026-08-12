package com.flightmonitor.mobile.di

import android.content.Context
import com.flightmonitor.mobile.data.AuthRepository
import com.flightmonitor.mobile.data.BusinessCaseRepository
import com.flightmonitor.mobile.data.DispatchOfflineQueue
import com.flightmonitor.mobile.data.DispatchChatRepository
import com.flightmonitor.mobile.data.DispatchRepository
import com.flightmonitor.mobile.data.DispatchSafetyRepository
import com.flightmonitor.mobile.data.MobileRepository
import com.flightmonitor.mobile.data.NotificationRepository
import com.flightmonitor.mobile.data.ShiftHandoverRepository
import com.flightmonitor.mobile.device.DeviceInfoProvider
import com.flightmonitor.mobile.network.ApiFactory
import com.flightmonitor.mobile.network.SseStreamClient
import com.flightmonitor.mobile.session.TokenStorage

class AppContainer(
    context: Context,
) {
    private val appContext = context.applicationContext
    private val tokenStorage = TokenStorage(appContext)
    private val deviceInfoProvider = DeviceInfoProvider(appContext)
    private val apiFactory = ApiFactory(tokenStorage, deviceInfoProvider)
    private val dispatchOfflineQueue = DispatchOfflineQueue(appContext)

    val authRepository: AuthRepository = AuthRepository(
        publicAuthApi = apiFactory.publicAuthApi(),
        authorizedAuthApi = apiFactory.authorizedAuthApi(),
        tokenStorage = tokenStorage,
    )

    val dispatchRepository: DispatchRepository = DispatchRepository(
        dispatchApi = apiFactory.dispatchApi(),
        offlineQueue = dispatchOfflineQueue,
    )

    val dispatchSafetyRepository: DispatchSafetyRepository = DispatchSafetyRepository(
        dispatchApi = apiFactory.dispatchApi(),
    )

    private val sseStreamClient: SseStreamClient = SseStreamClient(
        authRepository = authRepository,
        tokenStorage = tokenStorage,
    )

    val dispatchChatRepository: DispatchChatRepository = DispatchChatRepository(
        dispatchChatApi = apiFactory.dispatchChatApi(),
        sseStreamClient = sseStreamClient,
    )

    val notificationRepository: NotificationRepository = NotificationRepository(
        notificationApi = apiFactory.notificationApi(),
        sseStreamClient = sseStreamClient,
    )

    val businessCaseRepository: BusinessCaseRepository = BusinessCaseRepository(
        api = apiFactory.businessCaseApi(),
    )

    val shiftHandoverRepository: ShiftHandoverRepository = ShiftHandoverRepository(
        api = apiFactory.shiftHandoverApi(),
    )

    val mobileRepository: MobileRepository = MobileRepository(
        mobileApi = apiFactory.mobileApi(),
        deviceInfoProvider = deviceInfoProvider,
    )

    fun deviceId(): String = deviceInfoProvider.deviceId()
}
