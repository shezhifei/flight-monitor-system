package com.flightmonitor.mobile.ui

import android.os.Bundle
import android.view.LayoutInflater
import android.view.Menu
import android.view.View
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.NotificationItem
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.model.CollaborationUiMapper
import com.flightmonitor.mobile.ui.model.ReceiptStatusKind
import com.google.android.material.button.MaterialButton
import com.google.android.material.card.MaterialCardView
import com.google.gson.Gson
import com.google.gson.JsonElement
import com.google.gson.JsonObject
import com.google.gson.JsonParser
import com.google.gson.reflect.TypeToken
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.sse.EventSource
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import kotlin.math.max

class CollaborationActivity : AppCompatActivity() {
    private lateinit var streamStatusView: TextView
    private lateinit var streamStatusDot: View
    private lateinit var statusView: StatusMessageView
    private lateinit var progressView: ProgressBar
    private lateinit var unreadCountView: TextView
    private lateinit var notificationListContainer: LinearLayout
    private lateinit var emptyView: View
    private lateinit var swipeRefresh: SwipeRefreshLayout
    // Refresh moved to toolbar
    private lateinit var markAllNotificationsReadButton: MaterialButton

    private var notificationStream: EventSource? = null
    private var reconnectNotificationJob: Job? = null
    private var notificationFallbackRefreshJob: Job? = null

    private val gson = Gson()
    private val notificationListType = object : TypeToken<List<NotificationItem>>() {}.type

    private val notificationsState = mutableListOf<NotificationItem>()
    private var notificationUnreadCountState: Int = 0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_collaboration)

        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        // Toolbar setup
        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)

        streamStatusView = findViewById(R.id.collabStreamStatusView)
        streamStatusDot = findViewById(R.id.streamStatusDot)
        statusView = findViewById(R.id.collabStatusView)
        progressView = findViewById(R.id.collabProgress)
        unreadCountView = findViewById(R.id.notificationsUnreadCountView)
        notificationListContainer = findViewById(R.id.notificationListContainer)
        emptyView = findViewById(R.id.emptyView)
        swipeRefresh = findViewById(R.id.swipeRefresh)
        // Refresh moved to toolbar
        markAllNotificationsReadButton = findViewById(R.id.markAllNotificationsReadButton)

        // SwipeRefreshLayout setup
        swipeRefresh.setColorSchemeResources(R.color.primary)
        swipeRefresh.setOnRefreshListener { refreshAll(silent = true) }

        // Refresh moved to toolbar
        markAllNotificationsReadButton.setOnClickListener { confirmMarkAllRead() }

        refreshAll(silent = false)
    }

    override fun onStart() {
        super.onStart()
        startStreams()
    }

    override fun onStop() {
        stopStreams()
        super.onStop()
    }

    // ─── Data Loading ──────────────────────────────────────────

    private fun refreshAll(silent: Boolean) {
        val container = applicationContext.appContainer()
        if (!silent) setLoading(true)
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val notifications = container.notificationRepository.listNotifications()
                    val unreadCount = container.notificationRepository.unreadCount()
                    RefreshPayload(notifications, unreadCount)
                }
            }
            swipeRefresh.isRefreshing = false
            val payload = result.getOrNull()
            if (payload == null) {
                if (!silent) setLoading(false, result.exceptionOrNull()?.message)
                return@launch
            }
            setNotificationsState(payload.notifications, payload.unreadCount)
            if (!silent) setLoading(false)
        }
    }

    /** 二次确认弹窗：全部已读 */
    private fun confirmMarkAllRead() {
        if (notificationsState.isEmpty() || notificationUnreadCountState == 0) {
            showStatus("没有未读通知")
            return
        }
        AlertDialog.Builder(this)
            .setTitle("全部标记为已读")
            .setMessage("确定将所有 $notificationUnreadCountState 条未读通知标记为已读？")
            .setPositiveButton("确定") { _, _ -> markAllNotificationsRead() }
            .setNegativeButton("取消", null)
            .show()
    }

    private fun markAllNotificationsRead() {
        val container = applicationContext.appContainer()
        setLoading(true)
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { container.notificationRepository.markAllRead() }
            }
            if (result.isSuccess && result.getOrDefault(false)) {
                applyAllNotificationsReadState()
                setLoading(false)
                showStatus("全部通知已标记为已读")
            } else {
                setLoading(false, result.exceptionOrNull()?.message)
            }
        }
    }

    // ─── SSE Streaming ─────────────────────────────────────────

    private fun startStreams() {
        connectNotificationStreamIfNeeded()
    }

    private fun stopStreams() {
        reconnectNotificationJob?.cancel()
        reconnectNotificationJob = null
        notificationFallbackRefreshJob?.cancel()
        notificationFallbackRefreshJob = null
        notificationStream?.cancel()
        notificationStream = null
        updateStreamStatus("已断开", STATUS_DISCONNECTED)
    }

    private fun connectNotificationStreamIfNeeded() {
        if (notificationStream != null) return
        val container = applicationContext.appContainer()
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    container.notificationRepository.connectStream(
                        onOpen = {
                            runOnUiThread { updateStreamStatus("实时连接中", STATUS_CONNECTED) }
                        },
                        onEvent = { event, data ->
                            if (event != "heartbeat") {
                                onIncomingStreamEvent(event, data)
                            }
                        },
                        onClosed = {
                            runOnUiThread {
                                notificationStream = null
                                updateStreamStatus("连接断开", STATUS_DISCONNECTED)
                                scheduleNotificationReconnect()
                            }
                        },
                        onFailure = { message ->
                            runOnUiThread {
                                notificationStream = null
                                updateStreamStatus("连接失败", STATUS_ERROR)
                                scheduleNotificationReconnect()
                            }
                        },
                    )
                }
            }
            notificationStream = result.getOrNull()
            if (notificationStream == null) {
                updateStreamStatus("连接失败", STATUS_ERROR)
                scheduleNotificationReconnect()
            }
        }
    }

    private fun onIncomingStreamEvent(eventName: String?, rawData: String) {
        runOnUiThread {
            val payload = parseJsonObject(rawData)
            val handled = handleNotificationStreamEvent(eventName, payload)
            if (!handled) scheduleNotificationFallbackRefresh()
        }
    }

    private fun scheduleNotificationReconnect() {
        if (!lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) return
        if (reconnectNotificationJob?.isActive == true) return
        reconnectNotificationJob = lifecycleScope.launch {
            delay(5_000L)
            connectNotificationStreamIfNeeded()
        }
    }

    private fun scheduleNotificationFallbackRefresh() {
        if (!lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) return
        if (notificationFallbackRefreshJob?.isActive == true) return
        notificationFallbackRefreshJob = lifecycleScope.launch {
            delay(1_500L)
            refreshAll(silent = true)
        }
    }

    private fun handleNotificationStreamEvent(eventName: String?, payload: JsonObject?): Boolean {
        val normalizedEvent = eventName?.trim()?.lowercase().orEmpty()
        val payloadType = readText(payload, "type")?.lowercase().orEmpty()

        return when {
            normalizedEvent == "initial" || payloadType == "initial_data" -> {
                val notifications = parseNotifications(payload?.get("items")) ?: emptyList()
                setNotificationsState(notifications, readInt(payload, "unread_count"))
                true
            }
            payloadType == "user_notification" || normalizedEvent == "user_notification" || normalizedEvent == "message" -> {
                val notification = parseNotification(payload?.get("notification"))
                if (notification == null) {
                    false
                } else {
                    upsertNotificationState(notification)
                    notificationUnreadCountState = readInt(payload, "unread_count") ?: computeUnreadCountFromState()
                    renderNotifications()
                    true
                }
            }
            else -> false
        }
    }

    // ─── State Management ──────────────────────────────────────

    private fun setNotificationsState(notifications: List<NotificationItem>, unreadCount: Int?) {
        notificationsState.clear()
        notificationsState.addAll(normalizeNotifications(notifications))
        notificationUnreadCountState = unreadCount ?: computeUnreadCountFromState()
        renderNotifications()
    }

    private fun upsertNotificationState(notification: NotificationItem) {
        val index = notificationsState.indexOfFirst { it.notification_id == notification.notification_id }
        if (index >= 0) {
            notificationsState[index] = notification
        } else {
            notificationsState.add(0, notification)
        }
        val normalized = normalizeNotifications(notificationsState)
        notificationsState.clear()
        notificationsState.addAll(normalized)
    }

    private fun applyAllNotificationsReadState() {
        if (notificationsState.isEmpty()) {
            notificationUnreadCountState = 0
            renderNotifications()
            return
        }
        val now = nowIsoUtc()
        for (index in notificationsState.indices) {
            val current = notificationsState[index]
            notificationsState[index] = current.copy(
                is_read = true,
                read_status = "read",
                read_at = current.read_at ?: now,
            )
        }
        notificationUnreadCountState = 0
        renderNotifications()
    }

    private fun normalizeNotifications(items: List<NotificationItem>): List<NotificationItem> {
        if (items.isEmpty()) return emptyList()
        val map = linkedMapOf<String, NotificationItem>()
        items.forEach { item -> map[item.notification_id] = item }
        return map.values.sortedByDescending { it.created_at }
    }

    private fun computeUnreadCountFromState(): Int {
        return notificationsState.count { !isNotificationRead(it) }
    }

    private fun isNotificationRead(item: NotificationItem): Boolean {
        return item.is_read || item.read_status.equals("read", ignoreCase = true)
    }

    // ─── Render ────────────────────────────────────────────────

    private fun renderNotifications() {
        // Update unread badge
        if (notificationUnreadCountState > 0) {
            unreadCountView.visibility = View.VISIBLE
            unreadCountView.text = "$notificationUnreadCountState 未读"
        } else {
            unreadCountView.visibility = View.GONE
        }

        notificationListContainer.removeAllViews()

        if (notificationsState.isEmpty()) {
            emptyView.visibility = View.VISIBLE
            return
        }
        emptyView.visibility = View.GONE

        val inflater = LayoutInflater.from(this)
        notificationsState.forEach { item ->
            val cardView = inflater.inflate(R.layout.item_notification, notificationListContainer, false)
            bindNotificationCard(cardView, item)
            notificationListContainer.addView(cardView)
        }
    }

    private fun bindNotificationCard(view: View, item: NotificationItem) {
        val card = view.findViewById<MaterialCardView>(R.id.notificationCard)
        val originBadge = view.findViewById<TextView>(R.id.originBadge)
        val ackStatusBadge = view.findViewById<TextView>(R.id.ackStatusBadge)
        val timeView = view.findViewById<TextView>(R.id.timeView)
        val titleView = view.findViewById<TextView>(R.id.titleView)
        val bodyView = view.findViewById<TextView>(R.id.bodyView)
        val unreadDot = view.findViewById<View>(R.id.unreadDot)

        // Origin badge
        val origin = CollaborationUiMapper.mapOrigin(item.origin_type, item.origin_label)
        originBadge.text = origin.label

        // Ack status badge
        val ackStatus = CollaborationUiMapper.mapReceiptStatus(item.ack_status)
        ackStatusBadge.text = ackStatus.label
        when (ackStatus.kind) {
            ReceiptStatusKind.ACKNOWLEDGED -> {
                ackStatusBadge.backgroundTintList = android.content.res.ColorStateList.valueOf(0xFFE8F5E9.toInt())
                ackStatusBadge.setTextColor(0xFF2E7D32.toInt())
            }
            ReceiptStatusKind.REJECTED -> {
                ackStatusBadge.backgroundTintList = android.content.res.ColorStateList.valueOf(0xFFFFEBEE.toInt())
                ackStatusBadge.setTextColor(0xFFC62828.toInt())
            }
            ReceiptStatusKind.PENDING -> {
                ackStatusBadge.backgroundTintList = android.content.res.ColorStateList.valueOf(0xFFFFF3E0.toInt())
                ackStatusBadge.setTextColor(0xFFE65100.toInt())
            }
        }

        // Only show ack badge if receipt is required
        ackStatusBadge.visibility = if (item.receipt_required) View.VISIBLE else View.GONE

        // Time
        timeView.text = formatRelativeTime(item.created_at)

        // Title & body
        titleView.text = item.title
        bodyView.text = item.body.take(100)

        // Unread indicator
        val isRead = isNotificationRead(item)
        unreadDot.visibility = if (!isRead) View.VISIBLE else View.GONE

        // Card styling for unread
        if (!isRead) {
            card.strokeWidth = (2 * resources.displayMetrics.density).toInt()
            card.strokeColor = getColor(R.color.primary)
        } else {
            card.strokeWidth = 0
        }

        // Tap to open detail
        card.setOnClickListener {
            startActivity(NotificationDetailActivity.createIntent(this, item))
        }
    }

    private fun formatRelativeTime(isoTime: String?): String {
        if (isoTime.isNullOrBlank()) return ""
        return try {
            val formats = listOf(
                SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSSXXX", Locale.US),
                SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ssXXX", Locale.US),
                SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US).apply { timeZone = TimeZone.getTimeZone("UTC") },
                SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.US).apply { timeZone = TimeZone.getTimeZone("UTC") },
            )
            var date: Date? = null
            for (fmt in formats) {
                date = runCatching { fmt.parse(isoTime) }.getOrNull()
                if (date != null) break
            }
            if (date == null) return isoTime.take(16)
            val now = System.currentTimeMillis()
            val diff = now - date.time
            when {
                diff < 60_000 -> "刚刚"
                diff < 3600_000 -> "${diff / 60_000}分钟前"
                diff < 86400_000 -> "${diff / 3600_000}小时前"
                diff < 604800_000 -> "${diff / 86400_000}天前"
                else -> SimpleDateFormat("MM-dd HH:mm", Locale.getDefault()).format(date)
            }
        } catch (e: Exception) {
            isoTime.take(16)
        }
    }

    // ─── Stream Status UI ──────────────────────────────────────

    private fun updateStreamStatus(text: String, status: Int) {
        streamStatusView.text = text
        val dotColor = when (status) {
            STATUS_CONNECTED -> 0xFF4CAF50.toInt()  // green
            STATUS_ERROR -> 0xFFE53935.toInt()       // red
            else -> 0xFF9AA5B1.toInt()               // gray
        }
        streamStatusDot.backgroundTintList = android.content.res.ColorStateList.valueOf(dotColor)
    }

    // ─── UI Helpers ────────────────────────────────────────────

    private fun setLoading(loading: Boolean, errorMessage: String? = null) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        // Refresh moved to toolbar
        markAllNotificationsReadButton.isEnabled = !loading

        if (errorMessage != null) {
            showStatus("错误: $errorMessage")
        } else {
            statusView.renderStatus("")
        }
    }

    private fun showStatus(text: String) {
        statusView.renderStatus(text)
    }

    // ─── JSON Parsing ──────────────────────────────────────────

    private fun parseJsonObject(rawData: String?): JsonObject? {
        val text = rawData?.trim().orEmpty()
        if (text.isEmpty()) return null
        return runCatching {
            val element = JsonParser.parseString(text)
            if (element.isJsonObject) element.asJsonObject else null
        }.getOrNull()
    }

    private fun parseNotifications(element: JsonElement?): List<NotificationItem>? {
        if (element == null || element.isJsonNull) return null
        return runCatching {
            gson.fromJson<List<NotificationItem>>(element, notificationListType) ?: emptyList()
        }.getOrNull()
    }

    private fun parseNotification(element: JsonElement?): NotificationItem? {
        if (element == null || element.isJsonNull) return null
        return runCatching {
            gson.fromJson(element, NotificationItem::class.java)
        }.getOrNull()
    }

    private fun readText(payload: JsonObject?, key: String): String? {
        val element = payload?.get(key) ?: return null
        if (element.isJsonNull) return null
        return runCatching { element.asString.trim() }.getOrNull()?.ifBlank { null }
    }

    private fun readInt(payload: JsonObject?, key: String): Int? {
        val element = payload?.get(key) ?: return null
        if (element.isJsonNull) return null
        return runCatching { element.asString.toIntOrNull() }.getOrNull()
    }

    private fun nowIsoUtc(): String {
        val formatter = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US)
        formatter.timeZone = TimeZone.getTimeZone("UTC")
        return formatter.format(Date())
    }

    private data class RefreshPayload(
        val notifications: List<NotificationItem>,
        val unreadCount: Int,
    )

    
    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.menu_refresh, menu)
        return true
    }

    override fun onOptionsItemSelected(item: android.view.MenuItem): Boolean {
        return when (item.itemId) {
            R.id.action_refresh -> {
                refreshAll(silent = false)
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }
    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }

    companion object {
        private const val STATUS_CONNECTED = 1
        private const val STATUS_DISCONNECTED = 0
        private const val STATUS_ERROR = -1
    }
}
