package com.flightmonitor.mobile.ui

import android.os.Bundle
import android.view.Menu
import android.view.View
import android.widget.ProgressBar
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.MobileOperationsEventsResponse
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.google.android.material.button.MaterialButton
import com.google.android.material.chip.Chip
import com.google.android.material.chip.ChipGroup
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class OperationsCenterActivity : AppCompatActivity() {
    private lateinit var generatedAtView: TextView
    private lateinit var totalEventsView: TextView
    private lateinit var statusView: StatusMessageView
    private lateinit var progressView: ProgressBar
    // Refresh moved to toolbar

    private lateinit var eventsRecyclerView: RecyclerView
    private lateinit var noEventsHint: TextView
    private lateinit var severityFilterGroup: ChipGroup

    private var refreshJob: Job? = null
    private var cachedPayload: MobileOperationsEventsResponse? = null
    private var selectedSeverity: String? = null
    private lateinit var eventAdapter: OperationsEventAdapter

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_operations_center)

        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        generatedAtView = findViewById(R.id.operationsGeneratedAtView)
        totalEventsView = findViewById(R.id.operationsTotalEventsView)
        statusView = findViewById(R.id.operationsStatusView)
        progressView = findViewById(R.id.operationsProgress)
        // Refresh moved to toolbar
        // Back button removed - using toolbar navigation

        // Toolbar setup
        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)

        eventsRecyclerView = findViewById(R.id.eventsRecyclerView)
        noEventsHint = findViewById(R.id.noEventsHint)
        severityFilterGroup = findViewById(R.id.severityFilterGroup)

        eventsRecyclerView.layoutManager = LinearLayoutManager(this)
        eventAdapter = OperationsEventAdapter()
        eventsRecyclerView.adapter = eventAdapter

        // Refresh moved to toolbar


        loadEvents(showLoading = true)
    }

    override fun onStart() {
        super.onStart()
        if (refreshJob?.isActive == true) {
            return
        }
        refreshJob = lifecycleScope.launch {
            while (isActive) {
                delay(45_000L)
                loadEvents(showLoading = false)
            }
        }
    }

    override fun onStop() {
        refreshJob?.cancel()
        refreshJob = null
        super.onStop()
    }

    private fun loadEvents(showLoading: Boolean) {
        val container = applicationContext.appContainer()
        val limit = DEFAULT_LIMIT
        if (showLoading) {
            setLoading(true, getString(R.string.ops_loading_with_limit, limit))
        }
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    if (!container.authRepository.ensureSession()) {
                        throw IllegalStateException("会话失效，请重新登录")
                    }
                    container.mobileRepository.loadOperationEvents(limit = limit)
                        ?: throw IllegalStateException("战情聚合返回为空")
                }
            }
            val payload = result.getOrNull()
            if (payload == null) {
                if (showLoading) {
                    val message = result.exceptionOrNull()?.message ?: getString(R.string.ops_load_failed)
                    setLoading(false, getString(R.string.error_prefix, message))
                }
                return@launch
            }
            cachedPayload = payload
            bindEvents(payload)
            if (showLoading) {
                setLoading(false, getString(R.string.ops_load_success, payload.total))
            } else {
                statusView.renderStatus(getString(R.string.ops_updated_at, payload.generated_at))
            }
        }
    }

    private fun bindEvents(payload: MobileOperationsEventsResponse) {
        generatedAtView.text = payload.generated_at

        // 渲染过滤器 chips
        buildSeverityFilterChips(payload)

        // 应用过滤
        val filteredEvents = if (selectedSeverity == null) {
            payload.events
        } else {
            payload.events.filter { it.severity.equals(selectedSeverity, ignoreCase = true) }
        }

        totalEventsView.text = "事件 ${filteredEvents.size}/${payload.total}"

        if (filteredEvents.isEmpty()) {
            noEventsHint.visibility = View.VISIBLE
            eventsRecyclerView.visibility = View.GONE
        } else {
            noEventsHint.visibility = View.GONE
            eventsRecyclerView.visibility = View.VISIBLE
            eventAdapter.submitList(filteredEvents)
        }
    }

    private fun buildSeverityFilterChips(payload: MobileOperationsEventsResponse) {
        severityFilterGroup.removeAllViews()

        val allChip = Chip(this).apply {
            text = "全部"
            isCheckable = true
            isChecked = selectedSeverity == null
            setOnCheckedChangeListener { _, isChecked ->
                if (isChecked && selectedSeverity != null) {
                    selectedSeverity = null
                    cachedPayload?.let { bindEvents(it) }
                }
            }
        }
        severityFilterGroup.addView(allChip)

        val sortedSeverities = payload.severity_counts.entries
            .sortedByDescending { it.value }

        for ((severity, count) in sortedSeverities) {
            val chip = Chip(this).apply {
                text = "$severity($count)"
                isCheckable = true
                isChecked = selectedSeverity == severity
                setOnCheckedChangeListener { _, isChecked ->
                    if (isChecked && selectedSeverity != severity) {
                        selectedSeverity = severity
                        cachedPayload?.let { bindEvents(it) }
                    }
                }
            }
            severityFilterGroup.addView(chip)
        }
    }

    private fun setLoading(loading: Boolean, statusText: String) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        // Refresh moved to toolbar

        statusView.renderStatus(statusText)
    }

    private companion object {
        const val DEFAULT_LIMIT = 120
    }

    
    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.menu_refresh, menu)
        return true
    }

    override fun onOptionsItemSelected(item: android.view.MenuItem): Boolean {
        return when (item.itemId) {
            R.id.action_refresh -> {
                loadEvents(showLoading = true)
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }
    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }
}
