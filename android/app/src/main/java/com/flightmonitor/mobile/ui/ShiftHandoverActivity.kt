package com.flightmonitor.mobile.ui

import android.widget.EditText
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
import com.flightmonitor.mobile.api.model.ShiftHandover
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.google.android.material.button.MaterialButton
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class ShiftHandoverActivity : AppCompatActivity() {
    companion object {
        private const val OPERATOR_PREFS = "operator_identity_prefs"
        private const val OPERATOR_INIT_PREFIX = "operator_initialized::"
    }

    private lateinit var statusView: StatusMessageView
    private lateinit var progressView: ProgressBar
    private lateinit var summaryView: TextView
    private lateinit var noDataHint: TextView
    private lateinit var ackHandoverButton: MaterialButton
    // Refresh moved to toolbar
    private lateinit var detailSection: View
    private lateinit var detailLabel: TextView

    private lateinit var listRecyclerView: RecyclerView
    private lateinit var itemsRecyclerView: RecyclerView
    private lateinit var listAdapter: ShiftHandoverAdapter
    private lateinit var itemAdapter: ShiftHandoverItemAdapter

    private var selectedHandoverId: String? = null
    private var cachedHandovers: List<ShiftHandover> = emptyList()
    private var currentOperatorLabel: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_shift_handover)

        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        statusView = findViewById(R.id.handoverStatusView)
        progressView = findViewById(R.id.handoverProgress)
        summaryView = findViewById(R.id.handoverSummaryView)
        noDataHint = findViewById(R.id.handoverNoDataHint)
        ackHandoverButton = findViewById(R.id.handoverAckButton)
        // Refresh moved to toolbar
        detailSection = findViewById(R.id.handoverDetailSection)
        detailLabel = findViewById(R.id.handoverDetailLabel)
        
        listRecyclerView = findViewById(R.id.handoverListRecyclerView)
        itemsRecyclerView = findViewById(R.id.handoverItemsRecyclerView)
        
        // Back button removed - using toolbar navigation

        // Toolbar setup
        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)

        // Refresh moved to toolbar
        ackHandoverButton.setOnClickListener { acknowledgeHandover() }

        setupRecyclerViews()

        ensureOperatorIdentityAndRefresh()
    }

    private fun setupRecyclerViews() {
        listRecyclerView.layoutManager = LinearLayoutManager(this)
        listAdapter = ShiftHandoverAdapter { handover ->
            selectedHandoverId = handover.handover_id
            listAdapter.setSelectedHandoverId(selectedHandoverId)
            bindHandoverDetail(handover)
        }
        listRecyclerView.adapter = listAdapter

        itemsRecyclerView.layoutManager = LinearLayoutManager(this)
        itemAdapter = ShiftHandoverItemAdapter { item ->
            selectedHandoverId?.let { handoverId ->
                acknowledgeItem(handoverId, item.item_id)
            }
        }
        itemsRecyclerView.adapter = itemAdapter
    }

    private fun ensureOperatorIdentityAndRefresh(statusPrefix: String? = null) {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.loading))
        lifecycleScope.launch {
            val profile = withContext(Dispatchers.IO) {
                runCatching {
                    if (!container.authRepository.ensureSession()) {
                        throw IllegalStateException("会话失效，请重新登录")
                    }
                    container.authRepository.me()
                }
            }.getOrElse { error ->
                setLoading(false, getString(R.string.error_prefix, error.message ?: getString(R.string.handover_load_failed)))
                return@launch
            }

            currentOperatorLabel = profile.effective_operator_label
            supportActionBar?.subtitle = profile.effective_operator_label

            if (shouldPromptOperatorIdentity(profile)) {
                promptForOperatorIdentity(
                    initialValue = profile.display_name ?: profile.effective_operator_name ?: "",
                    onConfirmed = { operatorName ->
                        lifecycleScope.launch {
                            val updateResult = withContext(Dispatchers.IO) {
                                runCatching { container.authRepository.updateOperatorContext(operatorName) }
                            }
                            val updatedProfile = updateResult.getOrElse { error ->
                                setLoading(false, getString(R.string.error_prefix, error.message ?: getString(R.string.handover_operator_update_failed)))
                                return@launch
                            }
                            markOperatorIdentityInitialized(updatedProfile.id, container.deviceId())
                            currentOperatorLabel = updatedProfile.effective_operator_label
                            supportActionBar?.subtitle = updatedProfile.effective_operator_label
                            refreshHandovers(statusPrefix)
                        }
                    },
                )
                return@launch
            }

            refreshHandovers(statusPrefix)
        }
    }

    private fun refreshHandovers(statusPrefix: String? = null) {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.loading))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    if (!container.authRepository.ensureSession()) {
                        throw IllegalStateException("会话失效，请重新登录")
                    }
                    val profile = container.authRepository.me()
                    container.shiftHandoverRepository.listHandovers(
                        toUserId = profile.id,
                        limit = 80,
                    )
                }
            }
            val handovers = result.getOrNull()
            if (handovers == null) {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.handover_load_failed)
                setLoading(false, getString(R.string.error_prefix, message))
                return@launch
            }
            cachedHandovers = handovers
            bindHandoverList(handovers)
            val count = handovers.size
            val summary = getString(R.string.handover_load_success, count)
            val finalStatus = if (statusPrefix.isNullOrBlank()) summary else "$statusPrefix；$summary"
            setLoading(false, finalStatus)
        }
    }

    private fun acknowledgeItem(handoverId: String, itemId: String) {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.handover_ack_item_loading))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    container.shiftHandoverRepository.acknowledgeItem(
                        handoverId = handoverId,
                        itemId = itemId,
                        acknowledged = true,
                    )
                }
            }
            if (result.isSuccess) {
                refreshHandovers(statusPrefix = getString(R.string.handover_ack_item_success))
            } else {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.handover_ack_item_failed)
                setLoading(false, getString(R.string.error_prefix, message))
            }
        }
    }

    private fun acknowledgeHandover() {
        val handoverId = selectedHandoverId
        if (handoverId.isNullOrBlank()) {
            statusView.renderStatus(getString(R.string.handover_id_required))
            return
        }
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.handover_ack_loading))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { container.shiftHandoverRepository.acknowledgeHandover(handoverId) }
            }
            if (result.isSuccess) {
                refreshHandovers(statusPrefix = getString(R.string.handover_ack_success))
            } else {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.handover_ack_failed)
                setLoading(false, getString(R.string.error_prefix, message))
            }
        }
    }

    private fun bindHandoverList(handovers: List<ShiftHandover>) {
        if (handovers.isEmpty()) {
            noDataHint.visibility = View.VISIBLE
            listRecyclerView.visibility = View.GONE
            summaryView.text = getString(R.string.handover_no_data)
            detailSection.visibility = View.GONE
            ackHandoverButton.visibility = View.GONE
            listAdapter.submitList(emptyList())
            return
        }

        noDataHint.visibility = View.GONE
        listRecyclerView.visibility = View.VISIBLE

        // 自动选中第一个或保持之前的选中
        if (selectedHandoverId == null || handovers.none { it.handover_id == selectedHandoverId }) {
            selectedHandoverId = handovers.first().handover_id
        }

            val pendingCount = handovers.count { it.status != "completed" }
            val ssb = android.text.SpannableStringBuilder()

            currentOperatorLabel?.takeIf { it.isNotBlank() }?.let { label ->
                val operatorLine = "当前值班：$label\n"
                ssb.append(operatorLine)
            }

            // 总数（大字、粗体）
            val headline = "共 ${handovers.size} 条交接"
            val headlineStart = ssb.length
            ssb.append(headline)
            ssb.setSpan(
                android.text.style.StyleSpan(android.graphics.Typeface.BOLD),
                headlineStart, headlineStart + headline.length,
                android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
            )
            ssb.setSpan(
                android.text.style.RelativeSizeSpan(1.2f),
                headlineStart, headlineStart + headline.length,
                android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
            )

        // 待签收（警告色突出）
        val pendingLine = "\n待签收 $pendingCount 条"
        val pendingStart = ssb.length
        ssb.append(pendingLine)
        if (pendingCount > 0) {
            ssb.setSpan(
                android.text.style.ForegroundColorSpan(resources.getColor(R.color.status_warning_text)),
                pendingStart, ssb.length,
                android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
            )
        }
        summaryView.text = ssb

        listAdapter.submitList(handovers)
        listAdapter.setSelectedHandoverId(selectedHandoverId)

        // 渲染选中的交接班详情
        val selected = handovers.firstOrNull { it.handover_id == selectedHandoverId }
        if (selected != null) {
            bindHandoverDetail(selected)
        } else {
            detailSection.visibility = View.GONE
        }
    }

    private fun bindHandoverDetail(handover: ShiftHandover) {
        detailSection.visibility = View.VISIBLE
        val pendingItems = handover.items.count { !it.acknowledged }
        ackHandoverButton.visibility = if (handover.status != "completed" && pendingItems == 0) View.VISIBLE else View.GONE

        val actorRoute = listOfNotNull(handover.from_operator_label, handover.to_operator_label)
            .takeIf { it.size == 2 }
            ?.joinToString(" → ")
        detailLabel.text = listOfNotNull(
            "📝 交接条目详情 (${handover.items.size})",
            actorRoute,
            handover.summary?.takeIf { it.isNotBlank() },
        ).joinToString("  ·  ")
        itemAdapter.submitList(handover.items)
    }

    private fun setLoading(loading: Boolean, statusText: String) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        // Refresh moved to toolbar
        ackHandoverButton.isEnabled = !loading

        statusView.renderStatus(statusText)
    }

    
    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.menu_refresh, menu)
        return true
    }

    override fun onOptionsItemSelected(item: android.view.MenuItem): Boolean {
        return when (item.itemId) {
            R.id.action_refresh -> {
                refreshHandovers()
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }
    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }

    private fun shouldPromptOperatorIdentity(profile: com.flightmonitor.mobile.api.model.UserProfile): Boolean {
        val deviceId = applicationContext.appContainer().deviceId()
        val initialized = getSharedPreferences(OPERATOR_PREFS, MODE_PRIVATE)
            .getBoolean("${OPERATOR_INIT_PREFIX}${profile.id}::$deviceId", false)
        if (initialized) {
            return false
        }

        val hasBoundMobileContext =
            profile.operator_context_type == "mobile_device" &&
                profile.operator_context_id == deviceId &&
                !profile.effective_operator_name.isNullOrBlank()
        if (hasBoundMobileContext) {
            markOperatorIdentityInitialized(profile.id, deviceId)
            return false
        }

        return true
    }

    private fun markOperatorIdentityInitialized(userId: String, deviceId: String) {
        getSharedPreferences(OPERATOR_PREFS, MODE_PRIVATE)
            .edit()
            .putBoolean("${OPERATOR_INIT_PREFIX}$userId::$deviceId", true)
            .apply()
    }

    private fun promptForOperatorIdentity(
        initialValue: String,
        onConfirmed: (String) -> Unit,
    ) {
        val inputView = EditText(this).apply {
            setText(initialValue)
            setSelection(text.length)
            hint = getString(R.string.handover_operator_name_hint)
            setSingleLine(true)
        }

        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.handover_operator_dialog_title)
            .setMessage(R.string.handover_operator_dialog_message)
            .setView(inputView)
            .setCancelable(false)
            .setNegativeButton(R.string.handover_operator_dialog_cancel) { _, _ ->
                finish()
            }
            .setPositiveButton(R.string.handover_operator_dialog_confirm) { _, _ ->
                val operatorName = inputView.text?.toString()?.trim().orEmpty()
                if (operatorName.isBlank()) {
                    statusView.renderStatus(getString(R.string.handover_operator_name_required))
                    promptForOperatorIdentity(initialValue, onConfirmed)
                    return@setPositiveButton
                }
                onConfirmed(operatorName)
            }
            .show()
    }
}
