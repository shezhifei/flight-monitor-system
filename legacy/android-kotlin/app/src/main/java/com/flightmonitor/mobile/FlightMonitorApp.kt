package com.flightmonitor.mobile

import android.app.Application
import com.flightmonitor.mobile.di.AppContainer

class FlightMonitorApp : Application() {
    lateinit var appContainer: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        appContainer = AppContainer(this)
    }
}
