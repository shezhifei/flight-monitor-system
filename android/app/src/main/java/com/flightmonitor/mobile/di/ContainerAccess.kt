package com.flightmonitor.mobile.di

import android.content.Context
import com.flightmonitor.mobile.FlightMonitorApp

fun Context.appContainer(): AppContainer {
    return (applicationContext as FlightMonitorApp).appContainer
}
