package com.flightmonitor.mobile.ui

import android.content.Intent
import android.os.Bundle
import android.view.Menu
import android.view.MenuItem
import android.view.View
import android.widget.ProgressBar
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.BusinessCase
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.google.android.material.button.MaterialButton
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class BusinessCaseListActivity : AppCompatActivity() {

    private lateinit var statusView: StatusMessageView
    private lateinit var progressView: ProgressBar
    private lateinit var emptyHintView: TextView
    private lateinit var recyclerView: RecyclerView
    private lateinit var createButton: MaterialButton
    private lateinit var adapter: BusinessCaseAdapter

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_business_case_list)

        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
        supportActionBar?.subtitle = getString(R.string.business_case_list_subtitle)

        statusView = findViewById(R.id.businessCaseListStatusView)
        progressView = findViewById(R.id.businessCaseListProgress)
        emptyHintView = findViewById(R.id.businessCaseListEmptyHint)
        recyclerView = findViewById(R.id.businessCaseRecyclerView)
        createButton = findViewById(R.id.businessCaseCreateButton)

        adapter = BusinessCaseAdapter { businessCase ->
            startActivity(
                Intent(this, BusinessCaseDetailActivity::class.java)
                    .putExtra(BusinessCaseDetailActivity.EXTRA_CASE_ID, businessCase.case_id),
            )
        }
        recyclerView.layoutManager = LinearLayoutManager(this)
        recyclerView.adapter = adapter

        createButton.setOnClickListener {
            startActivity(Intent(this, BusinessCaseEditorActivity::class.java))
        }

        loadBusinessCases()
    }

    override fun onResume() {
        super.onResume()
        loadBusinessCases()
    }

    private fun loadBusinessCases() {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.business_case_loading))
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    if (!container.authRepository.ensureSession()) {
                        throw IllegalStateException("会话失效，请重新登录")
                    }
                    container.businessCaseRepository.listBusinessCases()
                }
            }
            val items = result.getOrNull()
            if (items == null) {
                setLoading(
                    false,
                    getString(
                        R.string.error_prefix,
                        result.exceptionOrNull()?.message ?: getString(R.string.business_case_list_load_failed),
                    ),
                )
                return@launch
            }
            bindList(items)
            setLoading(
                false,
                getString(R.string.business_case_list_load_success, items.size),
            )
        }
    }

    private fun bindList(items: List<BusinessCase>) {
        adapter.submitList(items)
        val isEmpty = items.isEmpty()
        emptyHintView.visibility = if (isEmpty) View.VISIBLE else View.GONE
        recyclerView.visibility = if (isEmpty) View.GONE else View.VISIBLE
    }

    private fun setLoading(loading: Boolean, statusText: String) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        createButton.isEnabled = !loading
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
                loadBusinessCases()
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }
}
