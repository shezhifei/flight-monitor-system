package com.flightmonitor.mobile.ui

import android.content.Intent
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.di.appContainer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class LauncherActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_launcher)

        lifecycleScope.launch {
            val container = applicationContext.appContainer()
            val hasSession = withContext(Dispatchers.IO) {
                container.authRepository.ensureSession()
            }
            if (hasSession) {
                withContext(Dispatchers.IO) {
                    container.mobileRepository.registerCurrentDevice()
                    container.mobileRepository.sendHeartbeat()
                    runCatching { container.dispatchRepository.syncOfflineActions() }
                }
                startActivity(Intent(this@LauncherActivity, WorkbenchActivity::class.java))
            } else {
                startActivity(Intent(this@LauncherActivity, LoginActivity::class.java))
            }
            finish()
        }
    }
}
