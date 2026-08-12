package com.flightmonitor.mobile.ui

import android.app.Activity
import android.graphics.Color
import android.os.Build
import android.view.View
import android.view.Window
import android.view.WindowManager
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsControllerCompat

object UIWindowInsets {

    /**
     * Set up edge-to-edge display and modify status/navigation bar colors depending on the API level.
     * Ensure the root layout of the Activity has fitsSystemWindows="false" or handles padding itself if utilizing edge-to-edge.
     * By default, we use a translucent approach for pre-API 23, and light/dark mode configuration for newer APIs.
     *
     * @param activity The target activity.
     * @param isLightStatusIcon True if you want status bar text/icons to be dark (for light backgrounds).
     */
    fun applySystemBarStyle(activity: Activity, isLightStatusIcon: Boolean = true) {
        val window = activity.window

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // API 30+ (Android 11+)
            WindowCompat.setDecorFitsSystemWindows(window, false)
            window.statusBarColor = Color.TRANSPARENT
            window.navigationBarColor = Color.TRANSPARENT
            
            val controller = WindowCompat.getInsetsController(window, window.decorView)
            controller.isAppearanceLightStatusBars = isLightStatusIcon
            controller.isAppearanceLightNavigationBars = true
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            // API 23+ (Android 6.0+)
            window.decorView.systemUiVisibility = (
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
            )
            
            var flags = window.decorView.systemUiVisibility
            if (isLightStatusIcon) {
                flags = flags or View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
            } else {
                flags = flags and View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR.inv()
            }

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                flags = flags or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
            }
            
            window.decorView.systemUiVisibility = flags
            
            window.clearFlags(WindowManager.LayoutParams.FLAG_TRANSLUCENT_STATUS)
            window.addFlags(WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS)
            window.statusBarColor = Color.TRANSPARENT
            window.navigationBarColor = Color.TRANSPARENT
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
            // API 21+ (Android 5.0+)
            window.decorView.systemUiVisibility = (
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
            )
            window.clearFlags(WindowManager.LayoutParams.FLAG_TRANSLUCENT_STATUS)
            window.addFlags(WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS)
            
            // For API 21-22, we can't change the icon color, so pure transparent on a light background is unreadable.
            // Using a translucent semi-black color is the accepted standard.
            window.statusBarColor = Color.parseColor("#40000000")
            window.navigationBarColor = Color.parseColor("#40000000")
        }
    }
}
