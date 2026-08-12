package com.flightmonitor.mobile.ui

import android.content.Intent
import android.graphics.Typeface
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.view.Gravity
import android.view.Menu
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.DispatchActionOutcome
import com.flightmonitor.mobile.api.model.DispatchOrderItem
import com.flightmonitor.mobile.api.model.DispatchSyncOutcome
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.model.CollaborationUiMapper
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.google.android.material.button.MaterialButton
import com.google.android.material.card.MaterialCardView
import java.io.IOException
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class DispatchActionsActivity : AppCompatActivity() {
    private lateinit var noteInput: EditText
    private lateinit var issueTitleInput: EditText
    private lateinit var issueDescriptionInput: EditText
    private lateinit var attachmentUrlsInput: EditText
    private lateinit var ordersView: TextView
    private lateinit var queueSizeView: StatusMessageView
    private lateinit var statusView: StatusMessageView
    private lateinit var dispatchOriginBadgeView: TextView
    private lateinit var dispatchReceiptSummaryView: TextView
    private lateinit var dispatchReceiptProgressBar: ProgressBar
    private lateinit var dispatchReceiptCountsRow: LinearLayout
    private lateinit var dispatchAckCountLabel: TextView
    private lateinit var dispatchRejCountLabel: TextView
    private lateinit var dispatchPendCountLabel: TextView
    private lateinit var currentOrderSummary: TextView
    private lateinit var currentStatusView: TextView
    private lateinit var orderSelectorLabel: TextView
    private lateinit var orderSelectorContainer: LinearLayout
    private lateinit var progressView: ProgressBar
    // Refresh moved to toolbar
    private lateinit var syncQueueButton: MaterialButton
    private lateinit var acceptButton: MaterialButton
    private lateinit var checkInButton: MaterialButton
    private lateinit var startButton: MaterialButton
    private lateinit var etaReportButton: MaterialButton
    private lateinit var completeButton: MaterialButton
    private lateinit var checkOutButton: MaterialButton
    private lateinit var reportIssueButton: MaterialButton
    private lateinit var uploadAttachmentButton: MaterialButton
    private lateinit var clearAttachmentButton: MaterialButton
    private lateinit var safetyChecklistButton: MaterialButton
    private lateinit var toggleIssueSection: MaterialButton
    private lateinit var issueSectionContent: MaterialCardView

    private val uploadedAttachmentUrls: MutableList<String> = mutableListOf()
    private var cachedOrders: List<DispatchOrderItem> = emptyList()
    private var selectedOrderId: String? = null
    private var isIssueSectionExpanded = false
    private val pickAttachmentLauncher = registerForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) {
            uploadAttachment(uri)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_dispatch_actions)

        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(systemBars.left, systemBars.top, systemBars.right, systemBars.bottom)
            insets
        }

        noteInput = findViewById(R.id.dispatchNoteInput)
        issueTitleInput = findViewById(R.id.dispatchIssueTitleInput)
        issueDescriptionInput = findViewById(R.id.dispatchIssueDescriptionInput)
        attachmentUrlsInput = findViewById(R.id.dispatchAttachmentUrlsInput)
        ordersView = findViewById(R.id.dispatchOrdersView)
        dispatchOriginBadgeView = findViewById(R.id.dispatchOriginBadgeView)
        dispatchReceiptSummaryView = findViewById(R.id.dispatchReceiptSummaryView)
        dispatchReceiptProgressBar = findViewById(R.id.dispatchReceiptProgressBar)
        dispatchReceiptCountsRow = findViewById(R.id.dispatchReceiptCountsRow)
        dispatchAckCountLabel = findViewById(R.id.dispatchAckCountLabel)
        dispatchRejCountLabel = findViewById(R.id.dispatchRejCountLabel)
        dispatchPendCountLabel = findViewById(R.id.dispatchPendCountLabel)
        currentOrderSummary = findViewById(R.id.dispatchCurrentOrderSummary)
        currentStatusView = findViewById(R.id.dispatchCurrentStatus)
        orderSelectorLabel = findViewById(R.id.orderSelectorLabel)
        orderSelectorContainer = findViewById(R.id.orderSelectorContainer)
        queueSizeView = findViewById(R.id.dispatchQueueSizeView)
        statusView = findViewById(R.id.dispatchStatusView)
        progressView = findViewById(R.id.dispatchProgress)
        // Refresh moved to toolbar
        syncQueueButton = findViewById(R.id.syncQueueButton)
        acceptButton = findViewById(R.id.acceptOrderButton)
        checkInButton = findViewById(R.id.checkInOrderButton)
        startButton = findViewById(R.id.startOrderButton)
        etaReportButton = findViewById(R.id.etaReportButton)
        completeButton = findViewById(R.id.completeOrderButton)
        reportIssueButton = findViewById(R.id.reportIssueButton)
        uploadAttachmentButton = findViewById(R.id.uploadAttachmentButton)
        clearAttachmentButton = findViewById(R.id.clearAttachmentButton)
        safetyChecklistButton = findViewById(R.id.dispatchSafetyChecklistButton)
        toggleIssueSection = findViewById(R.id.toggleIssueSection)
        issueSectionContent = findViewById(R.id.issueSectionContent)

        // Toolbar setup
        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)

        // Refresh moved to toolbar
        syncQueueButton.setOnClickListener { syncQueue() }
        acceptButton.setOnClickListener { acceptOrder() }
        checkInButton.setOnClickListener { checkInOrder() }
        startButton.setOnClickListener { startOrder() }
        etaReportButton.setOnClickListener { reportEstimatedCompletion() }
        completeButton.setOnClickListener { completeOrder() }
        checkOutButton = findViewById(R.id.checkOutOrderButton)
        checkOutButton.setOnClickListener { checkOutOrder() }
        reportIssueButton.setOnClickListener { reportIssue() }
        uploadAttachmentButton.setOnClickListener { pickAttachmentLauncher.launch("*/*") }
        clearAttachmentButton.setOnClickListener { clearAttachments() }
        safetyChecklistButton.setOnClickListener { openSafetyChecklist() }
        toggleIssueSection.setOnClickListener { toggleIssueSectionVisibility() }

        // 从 Intent 获取 order_id（从工作台点击跳转传入）
        selectedOrderId = intent?.getStringExtra(EXTRA_ORDER_ID)

        refreshOrders(syncBeforeLoad = true)
    }

    private fun toggleIssueSectionVisibility() {
        isIssueSectionExpanded = !isIssueSectionExpanded
        if (isIssueSectionExpanded) {
            issueSectionContent.visibility = View.VISIBLE
            toggleIssueSection.text = "收起异常上报"
            toggleIssueSection.setCompoundDrawablesRelativeWithIntrinsicBounds(R.drawable.ic_expand_less, 0, 0, 0)
        } else {
            issueSectionContent.visibility = View.GONE
            toggleIssueSection.text = "展开异常上报"
            toggleIssueSection.setCompoundDrawablesRelativeWithIntrinsicBounds(R.drawable.ic_expand_more, 0, 0, 0)
        }
    }

    private fun refreshOrders(syncBeforeLoad: Boolean, statusPrefix: String? = null) {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.loading))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val syncOutcome = if (syncBeforeLoad) {
                        runCatching { container.dispatchRepository.syncOfflineActions() }.getOrNull()
                    } else null
                    val orders = container.dispatchRepository.listMyOrders()
                    val queueSize = container.dispatchRepository.pendingQueueSize()
                    RefreshPayload(
                        orders = orders,
                        queueSize = queueSize,
                        syncOutcome = syncOutcome,
                    )
                }
            }

            val payload = result.getOrNull()
            if (payload == null) {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.dispatch_orders_load_failed)
                val currentQueueSize = container.dispatchRepository.pendingQueueSize()
                if (currentQueueSize > 0) {
                    queueSizeView.renderStatus(getString(R.string.dispatch_queue_size, currentQueueSize))
                } else {
                    queueSizeView.renderStatus("")
                }
                setLoading(false, getString(R.string.error_prefix, message))
                return@launch
            }

            cachedOrders = payload.orders
            bindOrders(payload.orders)

            // 离线队列提示
            if (payload.queueSize > 0) {
                queueSizeView.renderStatus(getString(R.string.dispatch_queue_size, payload.queueSize))
            } else {
                queueSizeView.renderStatus("")
            }

            val defaultStatus = payload.syncOutcome?.let {
                getString(
                    R.string.dispatch_sync_result,
                    it.total,
                    it.applied,
                    it.duplicates,
                    it.failed,
                    it.remainingQueueSize,
                )
            } ?: getString(R.string.dispatch_orders_loaded, payload.orders.size)
            val finalStatus = if (statusPrefix.isNullOrBlank()) {
                defaultStatus
            } else {
                "$statusPrefix；$defaultStatus"
            }
            setLoading(false, finalStatus)
        }
    }

    private fun syncQueue() {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.dispatch_syncing))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { container.dispatchRepository.syncOfflineActions() }
            }
            if (result.isSuccess) {
                val outcome = result.getOrThrow()
                val syncMessage = getString(
                    R.string.dispatch_sync_result,
                    outcome.total,
                    outcome.applied,
                    outcome.duplicates,
                    outcome.failed,
                    outcome.remainingQueueSize,
                )
                refreshOrders(syncBeforeLoad = false, statusPrefix = syncMessage)
            } else {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.dispatch_sync_failed)
                queueSizeView.renderStatus(getString(
                    R.string.dispatch_queue_size,
                    container.dispatchRepository.pendingQueueSize(),
                ))
                setLoading(false, getString(R.string.error_prefix, message))
            }
        }
    }

    private fun acceptOrder() {
        executeOrderAction(getString(R.string.dispatch_accepting)) { orderId, note ->
            applicationContext.appContainer().dispatchRepository.acceptOrder(
                orderId = orderId,
                note = note,
            )
        }
    }

    private fun checkInOrder() {
        executeOrderAction(getString(R.string.dispatch_checking_in)) { orderId, note ->
            applicationContext.appContainer().dispatchRepository.checkInOrder(
                orderId = orderId,
                note = note,
            )
        }
    }

    private fun checkOutOrder() {
        executeOrderAction("签退中…") { orderId, note ->
            applicationContext.appContainer().dispatchRepository.checkOutOrder(
                orderId = orderId,
                note = note,
            )
        }
    }

    private fun startOrder() {
        executeOrderAction(getString(R.string.dispatch_starting)) { orderId, note ->
            applicationContext.appContainer().dispatchRepository.startOrder(
                orderId = orderId,
                notes = note,
            )
        }
    }

    private fun completeOrder() {
        val orderId = readOrderId() ?: return
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.dispatch_validating_safety_gate))
        lifecycleScope.launch {
            val gateEvaluation = withContext(Dispatchers.IO) {
                evaluateSafetyGateBeforeComplete(orderId = orderId)
            }
            if (gateEvaluation.blocked) {
                setLoading(false, gateEvaluation.statusMessage ?: getString(R.string.dispatch_complete_blocked_unknown))
                return@launch
            }
            setLoading(false, gateEvaluation.statusMessage ?: getString(R.string.dispatch_complete_order))
            showTimeInputDialog(
                title = getString(R.string.dispatch_actual_end_time_title),
                initialValue = formatLocalDateTime(Date()),
            ) { actualEndTime ->
                executeOrderAction(
                    loadingText = getString(R.string.dispatch_completing),
                    successPrefix = gateEvaluation.statusMessage,
                ) { completeOrderId, note ->
                    container.dispatchRepository.completeOrder(
                        orderId = completeOrderId,
                        completionNotes = note,
                        actualEndTime = actualEndTime,
                    )
                }
            }
        }
    }

    private fun reportEstimatedCompletion() {
        val orderId = readOrderId() ?: return
        val note = noteInput.text?.toString()?.trim().orEmpty().ifBlank { null }
        showTimeInputDialog(
            title = getString(R.string.dispatch_eta_report_title),
            initialValue = formatLocalDateTime(Date(System.currentTimeMillis() + 30 * 60 * 1000L)),
        ) { estimatedCompletionTime ->
            setLoading(true, getString(R.string.dispatch_reporting_eta))
            lifecycleScope.launch {
                val result = withContext(Dispatchers.IO) {
                    runCatching {
                        applicationContext.appContainer().dispatchRepository.reportEstimatedCompletion(
                            orderId = orderId,
                            estimatedCompletionTime = estimatedCompletionTime,
                            note = note,
                        )
                    }
                }
                if (result.isSuccess) {
                    val outcome = result.getOrThrow()
                    refreshOrders(syncBeforeLoad = false, statusPrefix = outcome.message)
                } else {
                    val message = result.exceptionOrNull()?.message ?: getString(R.string.dispatch_action_failed)
                    setLoading(false, getString(R.string.error_prefix, message))
                }
            }
        }
    }

    private fun reportIssue() {
        val orderId = readOrderId() ?: return
        val title = issueTitleInput.text?.toString()?.trim().orEmpty()
            .ifEmpty { getString(R.string.dispatch_default_issue_title) }
        val description = issueDescriptionInput.text?.toString()?.trim().orEmpty().ifBlank { null }
        val attachments = readAttachmentUrls()

        setLoading(true, getString(R.string.dispatch_reporting_issue))
        val container = applicationContext.appContainer()
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    container.dispatchRepository.reportIssue(
                        orderId = orderId,
                        title = title,
                        description = description,
                        attachments = attachments,
                    )
                }
            }
            if (result.isSuccess) {
                val outcome = result.getOrThrow()
                refreshOrders(syncBeforeLoad = false, statusPrefix = outcome.message)
            } else {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.dispatch_action_failed)
                setLoading(false, getString(R.string.error_prefix, message))
            }
        }
    }

    private fun executeOrderAction(
        loadingText: String,
        successPrefix: String? = null,
        block: suspend (orderId: String, note: String?) -> DispatchActionOutcome,
    ) {
        val orderId = readOrderId() ?: return
        val note = noteInput.text?.toString()?.trim().orEmpty().ifBlank { null }
        val container = applicationContext.appContainer()
        setLoading(true, loadingText)
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    block(orderId, note)
                }
            }
            if (result.isSuccess) {
                val outcomeMessage = result.getOrThrow().message
                val finalPrefix = if (successPrefix.isNullOrBlank()) {
                    outcomeMessage
                } else {
                    "$successPrefix；$outcomeMessage"
                }
                refreshOrders(syncBeforeLoad = false, statusPrefix = finalPrefix)
            } else {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.dispatch_action_failed)
                setLoading(false, getString(R.string.error_prefix, message))
            }
        }
    }

    private fun bindOrders(orders: List<DispatchOrderItem>) {
        if (orders.isEmpty()) {
            currentOrderSummary.text = getString(R.string.dispatch_no_orders)
            currentStatusView.visibility = View.GONE
            dispatchOriginBadgeView.text = ""
            dispatchReceiptSummaryView.text = ""
            orderSelectorLabel.visibility = View.GONE
            orderSelectorContainer.visibility = View.GONE
            hideAllActionButtons()
            return
        }

        // 自动选中：优先用 Intent 传入的 ID，或第一条
        if (selectedOrderId.isNullOrBlank() || orders.none { it.id == selectedOrderId }) {
            selectedOrderId = orders.first().id
        }

        val selectedOrder = orders.firstOrNull { it.id == selectedOrderId } ?: orders.first()

        // 更新顶部固定摘要 — SpannableStringBuilder 排版层次
        val ssb = android.text.SpannableStringBuilder()
        val flightLine = "${selectedOrder.flight_id}"
        ssb.append(flightLine)
        ssb.setSpan(
            android.text.style.StyleSpan(Typeface.BOLD),
            0, flightLine.length,
            android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        ssb.setSpan(
            android.text.style.RelativeSizeSpan(1.2f),
            0, flightLine.length,
            android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        val detailLine = buildString {
            append("\n${selectedOrder.step_code}")
            selectedOrder.stand_id?.takeIf { it.isNotBlank() }?.let { append("  ·  机位:$it") }
            selectedOrder.terminal?.takeIf { it.isNotBlank() }?.let { append("  ·  $it") }
        }
        val detailStart = ssb.length
        ssb.append(detailLine)
        ssb.setSpan(
            android.text.style.ForegroundColorSpan(resources.getColor(R.color.text_secondary)),
            detailStart, ssb.length,
            android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        ssb.setSpan(
            android.text.style.RelativeSizeSpan(0.85f),
            detailStart, ssb.length,
            android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        currentOrderSummary.text = ssb

        val statusLabel = mapStatusLabel(selectedOrder.status)
        currentStatusView.text = statusLabel
        currentStatusView.visibility = View.VISIBLE
        val statusColors = mapStatusColors(selectedOrder.status)
        currentStatusView.setTextColor(statusColors.first)
        currentStatusView.setBackgroundColor(statusColors.second)
        val dp6 = (6 * resources.displayMetrics.density).toInt()
        val dp3 = (3 * resources.displayMetrics.density).toInt()
        currentStatusView.setPadding(dp6, dp3, dp6, dp3)

        // 根据 status 控制操作按钮可见性
        updateActionButtonsForStatus(selectedOrder.status)

        // 工单选择器（多工单时才显示）
        if (orders.size > 1) {
            orderSelectorLabel.visibility = View.VISIBLE
            orderSelectorContainer.visibility = View.VISIBLE
            orderSelectorContainer.removeAllViews()
            for (order in orders) {
                orderSelectorContainer.addView(buildOrderSelectorItem(order))
            }
        } else {
            orderSelectorLabel.visibility = View.GONE
            orderSelectorContainer.visibility = View.GONE
        }

        bindSelectedOrderDetails(selectedOrder)
    }

    private fun buildOrderSelectorItem(order: DispatchOrderItem): View {
        val isSelected = order.id == selectedOrderId
        val item = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            if (isSelected) {
                setBackgroundResource(R.drawable.bg_status_info)
            } else {
                setBackgroundResource(R.drawable.bg_card_surface)
            }
            val dp10 = (10 * resources.displayMetrics.density).toInt()
            val dp8 = (8 * resources.displayMetrics.density).toInt()
            setPadding(dp10, dp8, dp10, dp8)
            val params = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            )
            params.bottomMargin = (4 * resources.displayMetrics.density).toInt()
            layoutParams = params
            isClickable = true
            isFocusable = true
        }

        val label = TextView(this).apply {
            text = "${order.flight_id}  ·  ${mapStatusLabel(order.status)}  ·  ${order.step_code}"
            textSize = 14f
            setCompoundDrawablesRelativeWithIntrinsicBounds(R.drawable.baseline_flight_24, 0, 0, 0)
            compoundDrawablePadding = (4 * resources.displayMetrics.density).toInt()
            setTextColor(
                if (isSelected) resources.getColor(R.color.status_info_text)
                else resources.getColor(R.color.text_primary),
            )
            if (isSelected) setTypeface(null, Typeface.BOLD)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }

        item.addView(label)
        item.setOnClickListener {
            selectedOrderId = order.id
            bindOrders(cachedOrders)
        }

        return item
    }

    private fun updateActionButtonsForStatus(status: String) {
        hideAllActionButtons()
        when (status.lowercase()) {
            "assigned" -> {
                acceptButton.visibility = View.VISIBLE
            }
            "accepted" -> {
                checkInButton.visibility = View.VISIBLE
            }
            "checked_in" -> {
                startButton.visibility = View.VISIBLE
            }
            "in_progress" -> {
                etaReportButton.visibility = View.VISIBLE
                completeButton.visibility = View.VISIBLE
                checkOutButton.visibility = View.VISIBLE
                safetyChecklistButton.visibility = View.VISIBLE
            }
        }
    }

    private fun hideAllActionButtons() {
        acceptButton.visibility = View.GONE
        checkInButton.visibility = View.GONE
        startButton.visibility = View.GONE
        etaReportButton.visibility = View.GONE
        completeButton.visibility = View.GONE
        checkOutButton.visibility = View.GONE
        safetyChecklistButton.visibility = View.GONE
    }

    private fun mapStatusLabel(status: String): String {
        return when (status.lowercase()) {
            "pending" -> "待分配"
            "assigned" -> "待接单"
            "accepted" -> "已接单"
            "checked_in" -> "已签到"
            "in_progress" -> "作业中"
            "completed" -> "已完工"
            "cancelled" -> "已取消"
            else -> status
        }
    }

    private fun mapStatusColors(status: String): Pair<Int, Int> {
        return when (status.lowercase()) {
            "pending", "assigned" -> Pair(
                resources.getColor(R.color.status_warning_text),
                resources.getColor(R.color.status_warning_bg),
            )
            "accepted", "checked_in", "in_progress" -> Pair(
                resources.getColor(R.color.status_info_text),
                resources.getColor(R.color.status_info_bg),
            )
            "completed" -> Pair(
                resources.getColor(R.color.status_success_text),
                resources.getColor(R.color.status_success_bg),
            )
            "cancelled" -> Pair(
                resources.getColor(R.color.text_secondary),
                resources.getColor(R.color.surface_muted),
            )
            else -> Pair(
                resources.getColor(R.color.text_primary),
                resources.getColor(R.color.surface_soft),
            )
        }
    }

    private fun bindSelectedOrderDetails(order: DispatchOrderItem) {
        val origin = CollaborationUiMapper.mapOrigin(order.origin_type, order.origin_label)
        dispatchOriginBadgeView.text = badgeLine(
            context = this,
            badges = listOf(buildOriginBadge(origin)),
            suffix = getString(R.string.notification_detail_origin_label, origin.label),
        )
        val summary = CollaborationUiMapper.mapReceiptSummary(order.notification_receipt_summary)
        dispatchReceiptSummaryView.text = getString(
            R.string.dispatch_receipt_summary_label,
            summary.totalCount,
            summary.pendingCount,
            summary.acknowledgedCount,
            summary.rejectedCount,
        )

        // 可视化进度条和分色计数
        if (summary.totalCount > 0) {
            val total = summary.totalCount.coerceAtLeast(1)
            val ackPercent = (summary.acknowledgedCount * 100) / total
            dispatchReceiptProgressBar.visibility = View.VISIBLE
            dispatchReceiptProgressBar.max = 100
            dispatchReceiptProgressBar.progress = ackPercent

            dispatchReceiptCountsRow.visibility = View.VISIBLE
            dispatchAckCountLabel.text = "✓ ${summary.acknowledgedCount}"
            dispatchRejCountLabel.text = "✗ ${summary.rejectedCount}"
            dispatchPendCountLabel.text = "○ ${summary.pendingCount}"
        } else {
            dispatchReceiptProgressBar.visibility = View.GONE
            dispatchReceiptCountsRow.visibility = View.GONE
        }
    }

    private fun readOrderId(): String? {
        if (selectedOrderId.isNullOrBlank()) {
            statusView.renderStatus(getString(R.string.dispatch_order_required))
            return null
        }
        return selectedOrderId
    }

    private fun setLoading(loading: Boolean, statusText: String) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        // Refresh moved to toolbar
        syncQueueButton.isEnabled = !loading
        acceptButton.isEnabled = !loading
        checkInButton.isEnabled = !loading
        startButton.isEnabled = !loading
        etaReportButton.isEnabled = !loading
        completeButton.isEnabled = !loading
        checkOutButton.isEnabled = !loading
        reportIssueButton.isEnabled = !loading
        uploadAttachmentButton.isEnabled = !loading
        clearAttachmentButton.isEnabled = !loading
        safetyChecklistButton.isEnabled = !loading
        statusView.renderStatus(statusText)
    }

    private fun openSafetyChecklist() {
        val intent = Intent(this, SafetyChecklistActivity::class.java)
        selectedOrderId?.takeIf { it.isNotBlank() }?.let {
            intent.putExtra(SafetyChecklistActivity.EXTRA_ORDER_ID, it)
        }
        startActivity(intent)
    }

    private fun uploadAttachment(uri: Uri) {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.dispatch_uploading_attachment))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val resolver = contentResolver
                    val fileName = resolveFileName(uri)
                        ?: "dispatch_attachment_${System.currentTimeMillis()}"
                    val contentType = resolver.getType(uri)
                    val bytes = resolver.openInputStream(uri)?.use { stream ->
                        stream.readBytes()
                    } ?: throw IOException("无法读取附件内容")
                    if (bytes.isEmpty()) {
                        throw IOException("附件内容为空")
                    }
                    val uploadedUrl = container.mobileRepository.uploadDispatchIssueAttachment(
                        fileName = fileName,
                        contentType = contentType,
                        bytes = bytes,
                    ) ?: throw IllegalStateException("附件上传返回为空")
                    uploadedUrl
                }
            }
            val uploadedUrl = result.getOrNull()
            if (uploadedUrl == null) {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.dispatch_upload_attachment_failed)
                setLoading(false, getString(R.string.error_prefix, message))
                return@launch
            }
            if (uploadedUrl !in uploadedAttachmentUrls) {
                uploadedAttachmentUrls += uploadedUrl
            }
            updateAttachmentInput()
            setLoading(
                false,
                getString(R.string.dispatch_upload_attachment_success, uploadedAttachmentUrls.size),
            )
        }
    }

    private fun clearAttachments() {
        uploadedAttachmentUrls.clear()
        attachmentUrlsInput.setText("")
        statusView.renderStatus(getString(R.string.dispatch_attachment_cleared))
    }

    private fun updateAttachmentInput() {
        val merged = linkedSetOf<String>()
        merged += readAttachmentUrls()
        merged += uploadedAttachmentUrls
        uploadedAttachmentUrls.clear()
        uploadedAttachmentUrls.addAll(merged)
        attachmentUrlsInput.setText(uploadedAttachmentUrls.joinToString(separator = "\n"))
    }

    private fun readAttachmentUrls(): List<String> {
        val rawText = attachmentUrlsInput.text?.toString().orEmpty()
        if (rawText.isBlank()) {
            return emptyList()
        }
        return rawText
            .split(Regex("[,;\\n\\s]+"))
            .map { it.trim() }
            .filter { it.isNotBlank() }
            .distinct()
    }

    private fun resolveFileName(uri: Uri): String? {
        return runCatching {
            contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (index >= 0) cursor.getString(index) else null
                } else {
                    null
                }
            }
        }.getOrNull()
    }

    private suspend fun evaluateSafetyGateBeforeComplete(orderId: String): SafetyGateEvaluation {
        return runCatching {
            applicationContext.appContainer().dispatchSafetyRepository.getSafetyStatus(orderId)
        }.fold(
            onSuccess = { status ->
                if (!status.enforced) {
                    return@fold SafetyGateEvaluation(
                        blocked = false,
                        statusMessage = getString(R.string.dispatch_safety_gate_not_enforced),
                    )
                }
                if (status.ready) {
                    return@fold SafetyGateEvaluation(
                        blocked = false,
                        statusMessage = getString(
                            R.string.dispatch_safety_gate_ready,
                            status.completed_required,
                            status.required_total,
                        ),
                    )
                }
                val pendingItems = status.pending_required_items
                    .takeIf { it.isNotEmpty() }
                    ?.joinToString(separator = ",")
                    ?: getString(R.string.safety_none)
                SafetyGateEvaluation(
                    blocked = true,
                    statusMessage = getString(
                        R.string.dispatch_complete_blocked_by_safety,
                        status.completed_required,
                        status.required_total,
                        pendingItems,
                    ),
                )
            },
            onFailure = { error ->
                SafetyGateEvaluation(
                    blocked = false,
                    statusMessage = getString(
                        R.string.dispatch_safety_gate_check_skipped,
                        error.message ?: getString(R.string.dispatch_safety_gate_check_unknown),
                    ),
                )
            },
        )
    }

    private data class RefreshPayload(
        val orders: List<DispatchOrderItem>,
        val queueSize: Int,
        val syncOutcome: DispatchSyncOutcome?,
    )

    private data class SafetyGateEvaluation(
        val blocked: Boolean,
        val statusMessage: String? = null,
    )

    companion object {
        const val EXTRA_ORDER_ID = "dispatch_order_id"
    }

    
    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.menu_refresh, menu)
        return true
    }

    override fun onOptionsItemSelected(item: android.view.MenuItem): Boolean {
        return when (item.itemId) {
            R.id.action_refresh -> {
                refreshOrders(syncBeforeLoad = false)
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }
    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }

    private fun showTimeInputDialog(title: String, initialValue: String, onConfirm: (String) -> Unit) {
        val input = EditText(this).apply {
            setText(initialValue)
            hint = "yyyy-MM-dd HH:mm"
            setSelection(text?.length ?: 0)
        }
        AlertDialog.Builder(this)
            .setTitle(title)
            .setMessage(getString(R.string.dispatch_time_dialog_hint))
            .setView(input)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val isoValue = parseLocalDateTimeToIso(input.text?.toString().orEmpty())
                if (isoValue == null) {
                    statusView.renderStatus(getString(R.string.dispatch_invalid_time_input))
                    return@setPositiveButton
                }
                onConfirm(isoValue)
            }
            .show()
    }

    private fun formatLocalDateTime(date: Date): String {
        val formatter = SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.getDefault())
        return formatter.format(date)
    }

    private fun parseLocalDateTimeToIso(text: String): String? {
        val normalized = text.trim()
        if (normalized.isBlank()) {
            return null
        }
        return runCatching {
            val parser = SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.getDefault())
            parser.isLenient = false
            val parsed = parser.parse(normalized) ?: return null
            val formatter = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US)
            formatter.timeZone = TimeZone.getTimeZone("UTC")
            formatter.format(parsed)
        }.getOrNull()
    }
}
