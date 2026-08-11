package com.flightmonitor.mobile.ui

import android.content.Intent
import android.os.Bundle
import android.view.MenuItem
import android.view.View
import android.widget.ArrayAdapter
import android.widget.AutoCompleteTextView
import android.widget.ProgressBar
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.BusinessCaseCreateRequest
import com.flightmonitor.mobile.api.model.BusinessCaseType
import com.flightmonitor.mobile.api.model.BusinessCaseWorkflowStartRequest
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.google.android.material.button.MaterialButton
import com.google.android.material.textfield.TextInputEditText
import com.google.gson.Gson
import com.google.gson.reflect.TypeToken
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class BusinessCaseEditorActivity : AppCompatActivity() {

    companion object {
        private val VISIBILITY_OPTIONS = arrayOf("DEPARTMENT", "COMMON")
        private val STATUS_OPTIONS = arrayOf("", "INITIAL", "PENDING", "PROCESSING", "SUCCESS", "FAILED")
    }

    private lateinit var statusView: StatusMessageView
    private lateinit var progressView: ProgressBar
    private lateinit var caseTypeInput: AutoCompleteTextView
    private lateinit var flightIdInput: TextInputEditText
    private lateinit var visibilityInput: AutoCompleteTextView
    private lateinit var statusInput: AutoCompleteTextView
    private lateinit var descriptionInput: TextInputEditText
    private lateinit var contextJsonInput: TextInputEditText
    private lateinit var createOnlyButton: MaterialButton
    private lateinit var createWithWorkflowButton: MaterialButton

    private val gson = Gson()
    private var loadedCaseTypes: List<BusinessCaseType> = emptyList()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_business_case_editor)

        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
        supportActionBar?.subtitle = getString(R.string.business_case_editor_subtitle)

        statusView = findViewById(R.id.businessCaseEditorStatusView)
        progressView = findViewById(R.id.businessCaseEditorProgress)
        caseTypeInput = findViewById(R.id.businessCaseEditorCaseTypeInput)
        flightIdInput = findViewById(R.id.businessCaseEditorFlightIdInput)
        visibilityInput = findViewById(R.id.businessCaseEditorVisibilityInput)
        statusInput = findViewById(R.id.businessCaseEditorStatusInput)
        descriptionInput = findViewById(R.id.businessCaseEditorDescriptionInput)
        contextJsonInput = findViewById(R.id.businessCaseEditorContextInput)
        createOnlyButton = findViewById(R.id.businessCaseCreateOnlyButton)
        createWithWorkflowButton = findViewById(R.id.businessCaseCreateWithWorkflowButton)

        visibilityInput.setAdapter(
            ArrayAdapter(this, android.R.layout.simple_list_item_1, VISIBILITY_OPTIONS),
        )
        visibilityInput.setText(VISIBILITY_OPTIONS.first(), false)

        statusInput.setAdapter(
            ArrayAdapter(this, android.R.layout.simple_list_item_1, STATUS_OPTIONS),
        )
        statusInput.setText("PENDING", false)

        createOnlyButton.setOnClickListener { submit(createWithWorkflow = false) }
        createWithWorkflowButton.setOnClickListener { submit(createWithWorkflow = true) }

        loadCaseTypes()
    }

    private fun loadCaseTypes() {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.business_case_type_loading))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    if (!container.authRepository.ensureSession()) {
                        throw IllegalStateException("会话失效，请重新登录")
                    }
                    container.businessCaseRepository.listCaseTypes()
                }
            }
            loadedCaseTypes = result.getOrDefault(emptyList())
            caseTypeInput.setAdapter(
                ArrayAdapter(
                    this@BusinessCaseEditorActivity,
                    android.R.layout.simple_list_item_1,
                    loadedCaseTypes.map { it.code },
                ),
            )
            val statusText = if (loadedCaseTypes.isNotEmpty()) {
                getString(R.string.business_case_type_load_success, loadedCaseTypes.size)
            } else {
                result.exceptionOrNull()?.message
                    ?: getString(R.string.business_case_type_load_fallback)
            }
            setLoading(false, statusText)
        }
    }

    private fun submit(createWithWorkflow: Boolean) {
        val caseType = caseTypeInput.text?.toString().orEmpty().trim()
        val flightId = flightIdInput.text?.toString().orEmpty().trim()
        val description = descriptionInput.text?.toString().orEmpty().trim()
        val visibilityScope = visibilityInput.text?.toString().orEmpty().trim().ifBlank { "DEPARTMENT" }
        val status = statusInput.text?.toString().orEmpty().trim().ifBlank { null }
        val rawContext = contextJsonInput.text?.toString().orEmpty().trim()

        if (caseType.isBlank()) {
            statusView.renderStatus(getString(R.string.business_case_case_type_required))
            return
        }
        if (flightId.isBlank()) {
            statusView.renderStatus(getString(R.string.business_case_flight_id_required))
            return
        }
        if (description.isBlank()) {
            statusView.renderStatus(getString(R.string.business_case_description_required))
            return
        }

        val parsedContext = runCatching { parseJsonMap(rawContext) }.getOrElse { error ->
            statusView.renderStatus(
                getString(
                    R.string.error_prefix,
                    error.message ?: getString(R.string.business_case_context_invalid),
                ),
            )
            return
        }

        val container = applicationContext.appContainer()
        val loadingText = if (createWithWorkflow) {
            getString(R.string.business_case_workflow_creating)
        } else {
            getString(R.string.business_case_creating)
        }
        setLoading(true, loadingText)
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    if (!container.authRepository.ensureSession()) {
                        throw IllegalStateException("会话失效，请重新登录")
                    }
                    if (createWithWorkflow) {
                        container.businessCaseRepository.createAndStartWorkflow(
                            templateCode = caseType,
                            payload = BusinessCaseWorkflowStartRequest(
                                flight_id = flightId,
                                description = description,
                                extra_info = parsedContext,
                            ),
                        ).business_case
                    } else {
                        container.businessCaseRepository.createBusinessCase(
                            BusinessCaseCreateRequest(
                                case_type = caseType,
                                flight_id = flightId,
                                description = description,
                                visibility_scope = visibilityScope,
                                status = status,
                                context = parsedContext,
                            ),
                        )
                    }
                }
            }
            val createdCase = result.getOrNull()
            if (createdCase == null) {
                setLoading(
                    false,
                    getString(
                        R.string.error_prefix,
                        result.exceptionOrNull()?.message ?: getString(R.string.business_case_create_failed),
                    ),
                )
                return@launch
            }
            startActivity(
                Intent(this@BusinessCaseEditorActivity, BusinessCaseDetailActivity::class.java)
                    .putExtra(BusinessCaseDetailActivity.EXTRA_CASE_ID, createdCase.case_id),
            )
            finish()
        }
    }

    private fun parseJsonMap(raw: String): Map<String, Any?> {
        if (raw.isBlank()) {
            return emptyMap()
        }
        val type = object : TypeToken<Map<String, Any?>>() {}.type
        return gson.fromJson(raw, type)
            ?: throw IllegalArgumentException(getString(R.string.business_case_context_invalid))
    }

    private fun setLoading(loading: Boolean, statusText: String) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        createOnlyButton.isEnabled = !loading
        createWithWorkflowButton.isEnabled = !loading
        statusView.renderStatus(statusText)
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean {
        return when (item.itemId) {
            android.R.id.home -> {
                finish()
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }
}
