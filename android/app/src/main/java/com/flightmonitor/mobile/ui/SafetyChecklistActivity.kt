package com.flightmonitor.mobile.ui

import android.os.Bundle
import android.view.View
import android.widget.ProgressBar
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.flightmonitor.mobile.api.model.DispatchSafetyChecklistItemStatus
import com.flightmonitor.mobile.api.model.DispatchSafetyChecklistStatus
import com.flightmonitor.mobile.di.appContainer
import com.google.android.material.button.MaterialButton
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class SafetyChecklistActivity : AppCompatActivity() {
    private lateinit var summaryView: TextView
    private lateinit var statusView: StatusMessageView
    private lateinit var progressView: ProgressBar
    private lateinit var itemsRecyclerView: RecyclerView
    private lateinit var noItemsHint: TextView

    private lateinit var listAdapter: SafetyChecklistAdapter

    private var cachedStatus: DispatchSafetyChecklistStatus? = null
    private var selectedOrderId: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_safety_checklist)

        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        summaryView = findViewById(R.id.safetySummaryView)
        statusView = findViewById(R.id.safetyStatusView)
        progressView = findViewById(R.id.safetyProgress)
        itemsRecyclerView = findViewById(R.id.safetyItemsRecyclerView)
        noItemsHint = findViewById(R.id.safetyNoItemsHint)

        // Back button removed - using toolbar navigation

        // Toolbar setup
        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)

        // Setup RecyclerView
        itemsRecyclerView.layoutManager = LinearLayoutManager(this)
        listAdapter = SafetyChecklistAdapter { itemCode, result ->
            submitItemResult(itemCode, result)
        }
        itemsRecyclerView.adapter = listAdapter

        val initialOrderId = intent?.getStringExtra(EXTRA_ORDER_ID).orEmpty()
        if (initialOrderId.isNotBlank()) {
            selectedOrderId = initialOrderId
            loadSafetyStatus()
        } else {
            statusView.renderStatus(getString(R.string.safety_order_required))
        }
    }

    private fun loadSafetyStatus(statusPrefix: String? = null) {
        val orderId = readOrderId() ?: return
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.safety_loading))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    if (!container.authRepository.ensureSession()) {
                        throw IllegalStateException("会话失效，请重新登录")
                    }
                    container.dispatchSafetyRepository.getSafetyStatus(orderId)
                }
            }
            val payload = result.getOrNull()
            if (payload == null) {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.safety_load_failed)
                setLoading(false, getString(R.string.error_prefix, message))
                return@launch
            }
            cachedStatus = payload
            bindSafetyStatus(payload)
            val summary = getString(
                R.string.safety_load_success,
                payload.completed_required,
                payload.required_total,
                if (payload.ready) getString(R.string.safety_ready_yes) else getString(R.string.safety_ready_no),
            )
            val finalStatus = if (statusPrefix.isNullOrBlank()) summary else "$statusPrefix；$summary"
            setLoading(false, finalStatus)
        }
    }

    private fun submitItemResult(itemCode: String, resultCode: String) {
        val orderId = readOrderId() ?: return
        if (resultCode == "na" && !canSubmitNa(itemCode)) {
            statusView.renderStatus(getString(R.string.safety_item_na_not_allowed, itemCode))
            return
        }

        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.safety_submitting, resultCode))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    container.dispatchSafetyRepository.submitItemResult(
                        orderId = orderId,
                        itemCode = itemCode,
                        result = resultCode,
                        note = null,
                    )
                }
            }
            val record = result.getOrNull()
            if (record == null) {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.safety_submit_failed)
                setLoading(false, getString(R.string.error_prefix, message))
                return@launch
            }
            loadSafetyStatus(
                statusPrefix = getString(
                    R.string.safety_submit_success,
                    record.item_code,
                    record.result,
                ),
            )
        }
    }

    private fun bindSafetyStatus(status: DispatchSafetyChecklistStatus) {
        // 顶部摘要 — 使用 Spannable 实现阅读层次
        val readyLabel = if (status.ready) "✅ 可完工" else "⏳ 未就绪"
        val ssb = android.text.SpannableStringBuilder()

        // 第一行：就绪状态（大字、粗体）
        val headline = readyLabel
        ssb.append(headline)
        ssb.setSpan(
            android.text.style.StyleSpan(android.graphics.Typeface.BOLD),
            0, headline.length,
            android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        ssb.setSpan(
            android.text.style.RelativeSizeSpan(1.3f),
            0, headline.length,
            android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )

        // 第二行：进度
        val progressLine = "\n必填进度  ${status.completed_required} / ${status.required_total}"
        ssb.append(progressLine)

        // 第三行：工单信息（较小）
        val orderLine = "\n工单 ${status.dispatch_order_id.take(8)}…  ·  ${status.step_code}"
        val orderStart = ssb.length
        ssb.append(orderLine)
        ssb.setSpan(
            android.text.style.RelativeSizeSpan(0.85f),
            orderStart, ssb.length,
            android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        ssb.setSpan(
            android.text.style.ForegroundColorSpan(resources.getColor(R.color.text_secondary)),
            orderStart, ssb.length,
            android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )

        // 待补项（警告色）
        if (status.pending_required_items.isNotEmpty()) {
            val pendingLine = "\n⚠ 待补项: ${status.pending_required_items.joinToString(", ")}"
            val pendingStart = ssb.length
            ssb.append(pendingLine)
            ssb.setSpan(
                android.text.style.ForegroundColorSpan(resources.getColor(R.color.status_warning_text)),
                pendingStart, ssb.length,
                android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
            )
        }

        // 失败项（错误色）
        if (status.failed_required_items.isNotEmpty()) {
            val failLine = "\n❌ 失败项: ${status.failed_required_items.joinToString(", ")}"
            val failStart = ssb.length
            ssb.append(failLine)
            ssb.setSpan(
                android.text.style.ForegroundColorSpan(resources.getColor(R.color.status_error_text)),
                failStart, ssb.length,
                android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
            )
        }

        summaryView.text = ssb

        // 渲染检查项列表
        val items = status.items.sortedWith(
            compareBy<DispatchSafetyChecklistItemStatus> { if (it.status == "pending") 0 else 1 }
                .thenBy { it.order }
                .thenBy { it.item_code },
        )

        if (items.isEmpty()) {
            noItemsHint.visibility = View.VISIBLE
            itemsRecyclerView.visibility = View.GONE
        } else {
            noItemsHint.visibility = View.GONE
            itemsRecyclerView.visibility = View.VISIBLE
            listAdapter.submitList(items)
        }
    }

    private fun canSubmitNa(itemCode: String): Boolean {
        val item = cachedStatus?.items?.firstOrNull { it.item_code.equals(itemCode, ignoreCase = true) }
        return item?.allow_na ?: true
    }

    private fun readOrderId(): String? {
        if (selectedOrderId.isNullOrBlank()) {
            statusView.renderStatus(getString(R.string.safety_order_required))
            return null
        }
        return selectedOrderId
    }

    private fun setLoading(loading: Boolean, statusText: String) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        statusView.renderStatus(statusText)
    }

    companion object {
        const val EXTRA_ORDER_ID = "dispatch_order_id"
    }

    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }
}
