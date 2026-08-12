package com.flightmonitor.mobile.ui

import android.content.Intent
import android.os.Bundle
import android.view.View
import android.widget.EditText
import android.widget.ProgressBar
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.google.android.material.button.MaterialButton
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class LoginActivity : AppCompatActivity() {
    private lateinit var usernameInput: EditText
    private lateinit var passwordInput: EditText
    private lateinit var loginButton: MaterialButton
    private lateinit var loginProgress: ProgressBar
    private lateinit var loginStatus: StatusMessageView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_login)

        usernameInput = findViewById(R.id.usernameInput)
        passwordInput = findViewById(R.id.passwordInput)
        loginButton = findViewById(R.id.loginButton)
        loginProgress = findViewById(R.id.loginProgress)
        loginStatus = findViewById(R.id.loginStatus)

        loginButton.setOnClickListener { onLoginClicked() }
    }

    private fun onLoginClicked() {
        val username = usernameInput.text?.toString()?.trim().orEmpty()
        val password = passwordInput.text?.toString().orEmpty()
        if (username.isEmpty() || password.isEmpty()) {
            loginStatus.renderStatus(getString(R.string.error_prefix, "请输入账号和密码"))
            return
        }

        setLoading(true, getString(R.string.loading))
        val container = applicationContext.appContainer()
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    container.authRepository.login(username, password)
                    container.mobileRepository.registerCurrentDevice()
                    container.mobileRepository.sendHeartbeat()
                    runCatching { container.dispatchRepository.syncOfflineActions() }
                }
            }
            if (result.isSuccess) {
                startActivity(Intent(this@LoginActivity, WorkbenchActivity::class.java))
                finish()
            } else {
                val message = result.exceptionOrNull()?.message ?: "登录失败"
                setLoading(false, getString(R.string.error_prefix, message))
            }
        }
    }

    private fun setLoading(loading: Boolean, statusText: String) {
        loginButton.isEnabled = !loading
        loginProgress.visibility = if (loading) View.VISIBLE else View.GONE
        loginStatus.renderStatus(statusText)
    }
}
