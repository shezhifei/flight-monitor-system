package com.flightmonitor.mobile.ui

import android.os.Bundle
import android.view.Menu
import android.view.MenuItem
import android.view.View
import android.widget.EditText
import android.widget.ProgressBar
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.BusinessCase
import com.flightmonitor.mobile.api.model.BusinessCaseAppendEntry
import com.flightmonitor.mobile.api.model.BusinessCaseWorkflowRunDetail
import com.flightmonitor.mobile.api.model.acknowledgmentMap
import com.flightmonitor.mobile.api.model.businessCaseStatusLabel
import com.flightmonitor.mobile.api.model.businessCaseVisibilityLabel
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.google.android.material.button.MaterialButton
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class BusinessCaseDetailActivity : AppCompatActivity() {

    companion object {
        const val EXTRA_CASE_ID = "extra_case_id"

        private val STATUS_OPTIONS = arrayOf(
            "INITIAL",
            "PENDING",
            "PROCESSING",
            "SUCCESS",
            "FAILED",
        )
    }

    private lateinit var statusView: StatusMessageView
    private lateinit var progressView: ProgressBar
    private lateinit var summaryView: TextView
    private lateinit var workflowView: TextView
    private lateinit var contextView: TextView
    private lateinit var terminalMetadataView: TextView
    private lateinit var appendEmptyHint: TextView
    private lateinit var appendRecyclerView: RecyclerView
    private lateinit var appendButton: MaterialButton
    private lateinit var statusButton: MaterialButton
    private lateinit var appendAdapter: BusinessCaseAppendAdapter

    private var caseId: String = ""
    private var currentUserId: String? = null
    private var currentCase: BusinessCase? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_business_case_detail)

        caseId = intent.getStringExtra(EXTRA_CASE_ID).orEmpty()
        if (caseId.isBlank()) {
            finish()
            return
        }

        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)

        statusView = findViewById(R.id.businessCaseDetailStatusView)
        progressView = findViewById(R.id.businessCaseDetailProgress)
        summaryView = findViewById(R.id.businessCaseSummaryView)
        workflowView = findViewById(R.id.businessCaseWorkflowView)
        contextView = findViewById(R.id.businessCaseContextView)
        terminalMetadataView = findViewById(R.id.businessCaseTerminalMetadataView)
        appendEmptyHint = findViewById(R.id.businessCaseAppendEmptyHint)
        appendRecyclerView = findViewById(R.id.businessCaseAppendRecyclerView)
        appendButton = findViewById(R.id.businessCaseAppendButton)
        statusButton = findViewById(R.id.businessCaseStatusButton)

        appendAdapter = BusinessCaseAppendAdapter(
            currentUserId = { currentUserId },
            onAcknowledge = { entry -> acknowledgeAppend(entry) },
        )
        appendRecyclerView.layoutManager = LinearLayoutManager(this)
        appendRecyclerView.adapter = appendAdapter

        appendButton.setOnClickListener { openAppendDialog() }
        statusButton.setOnClickListener { openStatusDialog() }

        loadDetail()
    }

    private fun loadDetail() {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.business_case_loading_detail))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    if (!container.authRepository.ensureSession()) {
                        throw IllegalStateException("会话失效，请重新登录")
                    }
                    val profile = container.authRepository.me()
                    val businessCase = container.businessCaseRepository.getBusinessCase(caseId)
                    val workflow = container.businessCaseRepository.getWorkflowByCase(caseId)
                    Triple(profile.id, businessCase, workflow)
                }
            }
            val payload = result.getOrNull()
            if (payload == null) {
                setLoading(
                    false,
                    getString(
                        R.string.error_prefix,
                        result.exceptionOrNull()?.message ?: getString(R.string.business_case_detail_load_failed),
                    ),
                )
                return@launch
            }
            currentUserId = payload.first
            currentCase = payload.second
            bindDetail(payload.second, payload.third)
            setLoading(false, getString(R.string.business_case_detail_load_success))
        }
    }

    private fun bindDetail(
        businessCase: BusinessCase,
        workflow: BusinessCaseWorkflowRunDetail?,
    ) {
        supportActionBar?.title = businessCase.flight_no.ifBlank { businessCase.flight_id }
        supportActionBar?.subtitle = businessCase.case_type

        summaryView.text = buildString {
            appendLine("事项ID：${businessCase.case_id}")
            appendLine("航班ID：${businessCase.flight_id}")
            appendLine("状态：${businessCaseStatusLabel(businessCase.status)}")
            appendLine("范围：${businessCaseVisibilityLabel(businessCase.visibility_scope, businessCase.department_name_snapshot)}")
            appendLine("创建人：${businessCase.created_by}")
            appendLine("创建时间：${businessCase.created_at}")
            businessCase.stand?.takeIf { it.isNotBlank() }?.let { appendLine("机位：$it") }
            businessCase.gate?.takeIf { it.isNotBlank() }?.let { appendLine("登机口：$it") }
            appendLine()
            append(businessCase.description.ifBlank { "暂无描述" })
        }.trim()

        workflowView.text = if (workflow == null) {
            getString(R.string.business_case_workflow_none)
        } else {
            buildString {
                appendLine("模板：${workflow.run.template_code}")
                appendLine("流程状态：${workflow.run.status}")
                appendLine("实例ID：${workflow.run.process_instance_id}")
                workflow.run.receipt_group_id?.let { appendLine("回执批次：$it") }
                appendLine("激活任务：${workflow.active_tasks.size}")
                append("历史任务：${workflow.historic_tasks.size}")
            }
        }

        contextView.text = containerPrettyJson(businessCase.context)
        terminalMetadataView.text = businessCase.terminal_metadata?.let { containerPrettyJson(it) }
            ?: getString(R.string.business_case_terminal_metadata_empty)

        appendAdapter.submitList(businessCase.append_entries)
        val appendEmpty = businessCase.append_entries.isEmpty()
        appendEmptyHint.visibility = if (appendEmpty) View.VISIBLE else View.GONE
        appendRecyclerView.visibility = if (appendEmpty) View.GONE else View.VISIBLE
    }

    private fun openAppendDialog() {
        if (currentCase == null) {
            statusView.renderStatus(getString(R.string.business_case_detail_load_failed))
            return
        }
        val contentInput = EditText(this).apply {
            hint = getString(R.string.business_case_append_content_hint)
            minLines = 4
        }
        val mentionInput = EditText(this).apply {
            hint = getString(R.string.business_case_append_mentions_hint)
        }
        val containerView = android.widget.LinearLayout(this).apply {
            orientation = android.widget.LinearLayout.VERTICAL
            setPadding(40, 16, 40, 0)
            addView(contentInput)
            addView(mentionInput)
        }
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.business_case_append_dialog_title)
            .setView(containerView)
            .setNegativeButton(R.string.handover_operator_dialog_cancel, null)
            .setPositiveButton(R.string.business_case_append_submit) { _, _ ->
                val content = contentInput.text?.toString().orEmpty()
                val mentionUserIds = mentionInput.text?.toString()
                    .orEmpty()
                    .split(",", "，", "\n", " ")
                    .mapNotNull { item -> item.trim().takeIf { it.isNotBlank() } }
                    .distinct()
                submitAppend(content, mentionUserIds)
            }
            .show()
    }

    private fun submitAppend(content: String, mentionUserIds: List<String>) {
        if (content.isBlank()) {
            statusView.renderStatus(getString(R.string.business_case_append_content_required))
            return
        }
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.business_case_appending))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    container.businessCaseRepository.appendBusinessCase(
                        caseId = caseId,
                        content = content,
                        mentionUserIds = mentionUserIds,
                    )
                }
            }
            if (result.isSuccess) {
                currentCase = result.getOrNull()
                bindDetail(currentCase!!, withContext(Dispatchers.IO) {
                    runCatching { container.businessCaseRepository.getWorkflowByCase(caseId) }.getOrNull()
                })
                setLoading(false, getString(R.string.business_case_append_success))
            } else {
                setLoading(
                    false,
                    getString(
                        R.string.error_prefix,
                        result.exceptionOrNull()?.message ?: getString(R.string.business_case_append_failed),
                    ),
                )
            }
        }
    }

    private fun acknowledgeAppend(entry: BusinessCaseAppendEntry) {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.business_case_append_ack_loading))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    container.businessCaseRepository.acknowledgeAppend(caseId, entry.append_id)
                }
            }
            if (result.isSuccess) {
                loadDetail()
            } else {
                setLoading(
                    false,
                    getString(
                        R.string.error_prefix,
                        result.exceptionOrNull()?.message ?: getString(R.string.business_case_append_ack_failed),
                    ),
                )
            }
        }
    }

    private fun openStatusDialog() {
        val selectedIndex = STATUS_OPTIONS.indexOf(currentCase?.status?.uppercase()).coerceAtLeast(0)
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.business_case_status_dialog_title)
            .setSingleChoiceItems(STATUS_OPTIONS, selectedIndex) { dialog, which ->
                dialog.dismiss()
                submitStatusUpdate(STATUS_OPTIONS[which])
            }
            .setNegativeButton(R.string.handover_operator_dialog_cancel, null)
            .show()
    }

    private fun submitStatusUpdate(status: String) {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.business_case_status_updating))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { container.businessCaseRepository.updateStatus(caseId, status) }
            }
            if (result.isSuccess) {
                currentCase = result.getOrNull()
                bindDetail(currentCase!!, withContext(Dispatchers.IO) {
                    runCatching { container.businessCaseRepository.getWorkflowByCase(caseId) }.getOrNull()
                })
                setLoading(false, getString(R.string.business_case_status_update_success))
            } else {
                setLoading(
                    false,
                    getString(
                        R.string.error_prefix,
                        result.exceptionOrNull()?.message ?: getString(R.string.business_case_status_update_failed),
                    ),
                )
            }
        }
    }

    private fun containerPrettyJson(value: Any?): String {
        return applicationContext.appContainer().businessCaseRepository.prettyJson(value)
    }

    private fun setLoading(loading: Boolean, statusText: String) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        appendButton.isEnabled = !loading
        statusButton.isEnabled = !loading
        statusView.renderStatus(statusText)
    }

    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.menu_refresh, menu)
        return true
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean {
        return when (item.itemId) {
            android.R.id.home -> {
                finish()
                true
            }
            R.id.action_refresh -> {
                loadDetail()
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }
}
