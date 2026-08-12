package com.flightmonitor.mobile.device

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.os.Build
import android.os.BatteryManager
import android.provider.Settings
import com.flightmonitor.mobile.BuildConfig

class DeviceInfoProvider(
    private val context: Context,
) {
    fun deviceId(): String {
        val candidate = Settings.Secure.getString(context.contentResolver, Settings.Secure.ANDROID_ID)
        return if (candidate.isNullOrBlank()) {
            "android-${Build.MODEL}-${Build.VERSION.SDK_INT}"
        } else {
            candidate
        }
    }

    fun networkStatus(): String {
        val connectivityManager = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
            ?: return "unknown"
        val info = connectivityManager.activeNetworkInfo
        if (info == null || !info.isConnected) {
            return "offline"
        }
        return when (info.type) {
            ConnectivityManager.TYPE_WIFI -> "wifi"
            ConnectivityManager.TYPE_MOBILE -> "mobile"
            else -> "connected"
        }
    }

    fun batteryLevel(): Int? {
        val intent = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED)) ?: return null
        val level = intent.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
        val scale = intent.getIntExtra(BatteryManager.EXTRA_SCALE, -1)
        if (level < 0 || scale <= 0) {
            return null
        }
        val percent = (level * 100) / scale
        return if (percent in 0..100) percent else null
    }

    fun appVersionName(): String = BuildConfig.VERSION_NAME

    fun osVersionName(): String = Build.VERSION.RELEASE ?: Build.VERSION.SDK_INT.toString()

    fun manufacturer(): String = Build.MANUFACTURER ?: "unknown"

    fun model(): String = Build.MODEL ?: "unknown"
}
