package com.flightmonitor.mobile.ui

import android.content.Intent
import android.os.Bundle
import android.text.format.DateUtils
import android.view.Gravity
import android.view.MenuItem
import android.view.Menu
import android.view.View
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import androidx.appcompat.app.ActionBarDrawerToggle
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.widget.Toolbar
import androidx.drawerlayout.widget.DrawerLayout
import androidx.lifecycle.lifecycleScope
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.MobileWorkbenchOrderItem
import com.flightmonitor.mobile.api.model.MobileWorkbenchResponse
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.google.android.material.button.MaterialButton
import com.google.android.material.chip.Chip
import com.google.android.material.chip.ChipGroup
import com.google.android.material.navigation.NavigationView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class WorkbenchActivity : AppCompatActivity(), NavigationView.OnNavigationItemSelectedListener {

    private lateinit var drawerLayout: DrawerLayout
    private lateinit var navigationView: NavigationView
    private lateinit var toolbar: Toolbar

    private lateinit var countPendingNumber: TextView
    private lateinit var countAssignedNumber: TextView
    private lateinit var countInProgressNumber: TextView
    private lateinit var countCompletedNumber: TextView
    private lateinit var myOrdersContainer: LinearLayout
    private lateinit var noOrdersHint: TextView
    private lateinit var badgeChipGroup: ChipGroup
    private lateinit var syncTimeCaption: TextView
    private lateinit var offlineQueueWarning: StatusMessageView
    private lateinit var progressView: ProgressBar
    // Refresh moved to toolbar menu

    private var heartbeatJob: Job? = null
    private var preferredSafetyOrderId: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_workbench)

        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        // Drawer + Toolbar setup
        drawerLayout = findViewById(R.id.drawerLayout)
        navigationView = findViewById(R.id.navigationView)
        toolbar = findViewById(R.id.toolbar)

        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
        supportActionBar?.setHomeAsUpIndicator(R.drawable.ic_menu)

        val toggle = ActionBarDrawerToggle(
            this, drawerLayout, toolbar,
            R.string.action_back, R.string.action_back
        )
        drawerLayout.addDrawerListener(toggle)
        toggle.syncState()

        navigationView.setNavigationItemSelectedListener(this)

        // Update nav header with current user info
        updateNavHeader()

        // Main content bindings
        countPendingNumber = findViewById(R.id.countPendingNumber)
        countAssignedNumber = findViewById(R.id.countAssignedNumber)
        countInProgressNumber = findViewById(R.id.countInProgressNumber)
        countCompletedNumber = findViewById(R.id.countCompletedNumber)
        myOrdersContainer = findViewById(R.id.myOrdersContainer)
        noOrdersHint = findViewById(R.id.noOrdersHint)
        badgeChipGroup = findViewById(R.id.badgeChipGroup)
        syncTimeCaption = findViewById(R.id.syncTimeCaption)
        offlineQueueWarning = findViewById(R.id.offlineQueueWarning)
        progressView = findViewById(R.id.workbenchProgress)
        // Refresh moved to toolbar menu

        // Refresh moved to toolbar menu

        loadWorkbench()
    }

    override fun onStart() {
        super.onStart()
        startHeartbeatLoop()
    }

    override fun onResume() {
        super.onResume()
        loadWorkbench()
    }

    override fun onStop() {
        heartbeatJob?.cancel()
        heartbeatJob = null
        super.onStop()
    }

    override fun onNavigationItemSelected(item: MenuItem): Boolean {
        drawerLayout.closeDrawer(Gravity.START)
        when (item.itemId) {
            R.id.nav_dispatch -> openDispatchActions()
            R.id.nav_chat -> openChatGroups()
            R.id.nav_notification -> openNotifications()
            R.id.nav_operations -> openOperationsCenter()
            R.id.nav_business_case -> openBusinessCases()
            R.id.nav_handover -> openShiftHandovers()
            R.id.nav_safety -> openSafetyChecklist()
            R.id.nav_logout -> logout()
            else -> return false
        }
        return true
    }

    private fun updateNavHeader() {
        val headerView = navigationView.getHeaderView(0) ?: return
        val usernameView = headerView.findViewById<TextView>(R.id.navHeaderUsername)
        val roleView = headerView.findViewById<TextView>(R.id.navHeaderRole)
        usernameView?.text = "加载中..."
        roleView?.text = "地勤保障"
        val container = applicationContext.appContainer()
        lifecycleScope.launch {
            val profile = withContext(Dispatchers.IO) {
                runCatching { container.authRepository.me() }.getOrNull()
            }
            usernameView?.text = profile?.effective_operator_label ?: profile?.username ?: "未知用户"
            roleView?.text = profile?.username?.let { "岗位账号：$it" } ?: "地勤保障"
        }
    }

    private fun startHeartbeatLoop() {
        if (heartbeatJob?.isActive == true) return
        val container = applicationContext.appContainer()
        heartbeatJob = lifecycleScope.launch {
            while (isActive) {
                withContext(Dispatchers.IO) {
                    runCatching {
                        container.authRepository.heartbeat()
                        container.mobileRepository.sendHeartbeat()
                        container.dispatchRepository.syncOfflineActions()
                    }
                }
                delay(60_000L)
            }
        }
    }

    private fun loadWorkbench() {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.loading))
        lifecycleScope.launch {
            val payload = withContext(Dispatchers.IO) {
                runCatching {
                    if (!container.authRepository.ensureSession()) {
                        throw IllegalStateException("会话失效，请重新登录")
                    }
                    val syncOutcome = runCatching {
                        container.dispatchRepository.syncOfflineActions()
                    }.getOrNull()
                    val pendingQueueSize = container.dispatchRepository.pendingQueueSize()
                    val workbench = container.mobileRepository.loadWorkbench(
                        pendingSyncActionCount = pendingQueueSize,
                    ) ?: throw IllegalStateException("工作台返回为空")
                    WorkbenchLoadPayload(
                        data = workbench,
                        pendingQueueSize = pendingQueueSize,
                        syncOutcome = syncOutcome?.let {
                            "补传 applied=${it.applied}, duplicate=${it.duplicates}, failed=${it.failed}"
                        },
                    )
                }
            }
            val loadPayload = payload.getOrNull()
            if (loadPayload == null) {
                val message = payload.exceptionOrNull()?.message ?: "工作台加载失败"
                setLoading(false, getString(R.string.error_prefix, message))
                return@launch
            }
            bindWorkbench(loadPayload.data)

            // Format sync time
            val generatedAt = loadPayload.data.generated_at
            val relativeTime = formatRelativeTime(generatedAt)
            val pendingQueue = loadPayload.pendingQueueSize

            // Offline queue warning
            if (pendingQueue > 0) {
                offlineQueueWarning.renderStatus("离线队列中有 $pendingQueue 个操作等待同步")
            } else {
                offlineQueueWarning.renderStatus("")
            }

            val statusParts = mutableListOf<String>()
            statusParts += "同步于 $relativeTime"
            loadPayload.syncOutcome?.let { statusParts += it }
            setLoading(false, statusParts.joinToString("·"))
        }
    }

    private fun bindWorkbench(data: MobileWorkbenchResponse) {
        preferredSafetyOrderId = data.my_orders.firstOrNull()?.order_id

        // Stats grid
        countPendingNumber.text = data.order_counts.pending.toString()
        countAssignedNumber.text = data.order_counts.assigned.toString()
        countInProgressNumber.text = data.order_counts.in_progress.toString()
        countCompletedNumber.text = data.order_counts.completed.toString()

        // Order cards
        myOrdersContainer.removeAllViews()
        if (data.my_orders.isEmpty()) {
            noOrdersHint.visibility = View.VISIBLE
        } else {
            noOrdersHint.visibility = View.GONE
            for (order in data.my_orders) {
                myOrdersContainer.addView(buildOrderCard(order))
            }
        }

        // Badge chips — clickable navigation
        badgeChipGroup.removeAllViews()
        val hasAnyBadge = data.notification_unread_count > 0
            || data.chat_unread_total > 0
            || data.pending_shift_handover_count > 0
        if (hasAnyBadge) {
            badgeChipGroup.visibility = View.VISIBLE
            if (data.notification_unread_count > 0) {
                badgeChipGroup.addView(buildBadgeChip(
                    "未读通知 ${data.notification_unread_count}",
                    R.drawable.ic_notifications
                ) { openNotifications() })
            }
            if (data.chat_unread_total > 0) {
                badgeChipGroup.addView(buildBadgeChip(
                    "未读消息 ${data.chat_unread_total}",
                    R.drawable.ic_chat
                ) { openChatGroups() })
            }
            if (data.pending_shift_handover_count > 0) {
                badgeChipGroup.addView(buildBadgeChip(
                    "待签交接 ${data.pending_shift_handover_count}",
                    R.drawable.ic_swap_horiz
                ) { openShiftHandovers() })
            }
        } else {
            badgeChipGroup.visibility = View.GONE
        }
    }

    private fun buildBadgeChip(label: String, iconRes: Int, onClick: () -> Unit): Chip {
        return Chip(this).apply {
            text = label
            setChipIconResource(iconRes)
            isChipIconVisible = true
            isClickable = true
            isCheckable = false
            setOnClickListener { onClick() }
        }
    }

    private fun buildOrderCard(order: MobileWorkbenchOrderItem): View {
        val density = resources.displayMetrics.density
        fun dp(v: Int) = (v * density).toInt()

        val card = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundResource(R.drawable.bg_card_surface)
            setPadding(dp(14), dp(12), dp(14), dp(12))
            layoutParams = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            ).also { it.bottomMargin = dp(8) }
        }

        // Row 1: Flight ID + Status badge
        val row1 = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            layoutParams = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            )
        }
        val flightLabel = TextView(this).apply {
            text = order.flight_id
            textSize = 18f
            setTextColor(resources.getColor(R.color.text_primary))
            setTypeface(typeface, android.graphics.Typeface.BOLD)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        val statusLabel = TextView(this).apply {
            text = mapStatusLabel(order.status)
            textSize = 13f
            val statusColors = mapStatusColors(order.status)
            setTextColor(statusColors.first)
            setBackgroundColor(statusColors.second)
            setPadding(dp(6), dp(3), dp(6), dp(3))
        }
        row1.addView(flightLabel)
        row1.addView(statusLabel)
        card.addView(row1)

        // Row 2: Step + Order ID
        val row2 = TextView(this).apply {
            text = "${order.step_code}  ·  ${order.order_id.take(8)}..."
            textSize = 13f
            setTextColor(resources.getColor(R.color.text_secondary))
            setPadding(0, dp(4), 0, 0)
        }
        card.addView(row2)

        // Row 3: Location (terminal + stand) — only if available
        val locationParts = listOfNotNull(
            order.terminal?.let { "航站楼 $it" },
            order.stand_id?.let { "机位 $it" },
            order.gate?.let { "登机口 $it" },
        )
        if (locationParts.isNotEmpty()) {
            val row3 = TextView(this).apply {
                text = locationParts.joinToString("  ·  ")
                textSize = 12f
                setTextColor(resources.getColor(R.color.text_secondary))
                setPadding(0, dp(4), 0, 0)
            }
            card.addView(row3)
        }

        // Row 4: Planned time — only if available
        val timeText = formatTimeRange(order.planned_start_time, order.planned_end_time)
        if (timeText != null) {
            val row4 = TextView(this).apply {
                text = timeText
                textSize = 12f
                setTextColor(resources.getColor(R.color.text_secondary))
                setPadding(0, dp(4), 0, 0)
            }
            card.addView(row4)
        }

        card.isClickable = true
        card.isFocusable = true
        card.setOnClickListener { showOrderDetailSheet(order) }

        return card
    }

    private fun showOrderDetailSheet(order: MobileWorkbenchOrderItem) {
        val sheet = com.google.android.material.bottomsheet.BottomSheetDialog(this)
        val view = layoutInflater.inflate(R.layout.layout_order_detail_sheet, null)

        // Flight + status
        view.findViewById<TextView>(R.id.sheetFlightId).text = order.flight_id
        val badge = view.findViewById<TextView>(R.id.sheetStatusBadge)
        badge.text = mapStatusLabel(order.status)
        val colors = mapStatusColors(order.status)
        badge.setTextColor(colors.first)
        badge.setBackgroundColor(colors.second)

        // Step + order
        view.findViewById<TextView>(R.id.sheetStepAndOrder).text =
            "步骤: ${order.step_code}  ·  工单: ${order.order_id.take(12)}..."

        // Location
        val locParts = listOfNotNull(
            order.terminal?.let { "航站楼 $it" },
            order.stand_id?.let { "停机位 $it" },
            order.gate?.let { "登机口 $it" },
        )
        if (locParts.isNotEmpty()) {
            val locView = view.findViewById<TextView>(R.id.sheetLocationRow)
            locView.visibility = View.VISIBLE
            locView.text = locParts.joinToString("  ·  ")
        }

        // Planned time
        val plannedRange = formatTimeRange(order.planned_start_time, order.planned_end_time)
        if (plannedRange != null) {
            val pv = view.findViewById<TextView>(R.id.sheetPlannedTimeRow)
            pv.visibility = View.VISIBLE
            pv.text = "计划: $plannedRange"
        }

        // Actual start
        val actualTime = formatSingleTime(order.actual_start_time)
        if (actualTime != null) {
            val av = view.findViewById<TextView>(R.id.sheetActualTimeRow)
            av.visibility = View.VISIBLE
            av.text = "实际开始: $actualTime"
        }

        // Deadline
        val deadlineTime = formatSingleTime(order.assignment_deadline)
        if (deadlineTime != null) {
            val dv = view.findViewById<TextView>(R.id.sheetDeadlineRow)
            dv.visibility = View.VISIBLE
            dv.text = "截止: $deadlineTime"
        }

        // Supervisor
        if (order.supervisor_notified) {
            val sv = view.findViewById<TextView>(R.id.sheetSupervisorRow)
            sv.visibility = View.VISIBLE
            sv.text = "主管已通知"
        }

        // Open order button
        view.findViewById<View>(R.id.sheetOpenOrderButton).setOnClickListener {
            sheet.dismiss()
            val intent = Intent(this, DispatchActionsActivity::class.java)
            intent.putExtra(DispatchActionsActivity.EXTRA_ORDER_ID, order.order_id)
            startActivity(intent)
        }

        sheet.setContentView(view)
        sheet.show()
    }

    private fun formatTimeRange(start: String?, end: String?): String? {
        val s = formatSingleTime(start)
        val e = formatSingleTime(end)
        return when {
            s != null && e != null -> "$s → $e"
            s != null -> "$s → ?"
            e != null -> "? → $e"
            else -> null
        }
    }

    private fun formatSingleTime(iso: String?): String? {
        if (iso.isNullOrBlank()) return null
        return try {
            val sdf = java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss", java.util.Locale.getDefault())
            sdf.timeZone = java.util.TimeZone.getTimeZone("UTC")
            val date = sdf.parse(iso) ?: return iso.take(16)
            val localFmt = java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault())
            localFmt.format(date)
        } catch (_: Exception) {
            iso.take(16)
        }
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

    private fun logout() {
        val container = applicationContext.appContainer()
        setLoading(true, getString(R.string.loading))
        lifecycleScope.launch {
            withContext(Dispatchers.IO) {
                runCatching {
                    container.mobileRepository.unregisterCurrentDevice()
                    container.authRepository.logout()
                }
            }
            startActivity(
                Intent(this@WorkbenchActivity, LoginActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK),
            )
            finish()
        }
    }

    private fun setLoading(loading: Boolean, statusText: String) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        // Refresh moved to toolbar menu
        syncTimeCaption.text = statusText
    }

    private fun formatRelativeTime(isoTimestamp: String): String {
        return try {
            val sdf = java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss", java.util.Locale.getDefault())
            // Try parsing with timezone offset
            val dateWithTz = try {
                java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ssXXX", java.util.Locale.getDefault()).parse(isoTimestamp)
            } catch (_: Exception) { null }
            val date = dateWithTz ?: sdf.parse(isoTimestamp)
            if (date != null) {
                DateUtils.getRelativeTimeSpanString(
                    date.time,
                    System.currentTimeMillis(),
                    DateUtils.MINUTE_IN_MILLIS,
                    DateUtils.FORMAT_ABBREV_RELATIVE
                ).toString()
            } else {
                isoTimestamp
            }
        } catch (_: Exception) {
            isoTimestamp
        }
    }

    private fun openDispatchActions() {
        startActivity(Intent(this, DispatchActionsActivity::class.java))
    }

    private fun openChatGroups() {
        startActivity(Intent(this, ChatGroupListActivity::class.java))
    }

    private fun openNotifications() {
        startActivity(Intent(this, CollaborationActivity::class.java))
    }

    private fun openOperationsCenter() {
        startActivity(Intent(this, OperationsCenterActivity::class.java))
    }

    private fun openBusinessCases() {
        startActivity(Intent(this, BusinessCaseListActivity::class.java))
    }

    private fun openShiftHandovers() {
        startActivity(Intent(this, ShiftHandoverActivity::class.java))
    }

    private fun openSafetyChecklist() {
        val intent = Intent(this, SafetyChecklistActivity::class.java)
        preferredSafetyOrderId?.takeIf { it.isNotBlank() }?.let {
            intent.putExtra(SafetyChecklistActivity.EXTRA_ORDER_ID, it)
        }
        startActivity(intent)
    }

    private data class WorkbenchLoadPayload(
        val data: MobileWorkbenchResponse,
        val pendingQueueSize: Int,
        val syncOutcome: String?,
    )

    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.menu_refresh, menu)
        return true
    }

    override fun onOptionsItemSelected(item: android.view.MenuItem): Boolean {
        return when (item.itemId) {
            R.id.action_refresh -> {
                loadWorkbench()
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }
}
