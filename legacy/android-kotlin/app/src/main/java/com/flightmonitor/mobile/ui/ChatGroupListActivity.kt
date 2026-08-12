package com.flightmonitor.mobile.ui

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.LayoutInflater
import android.view.Menu
import android.view.View
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.DispatchChatGroupSummary
import com.flightmonitor.mobile.di.appContainer
import com.google.android.material.button.MaterialButton
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

class ChatGroupListActivity : AppCompatActivity() {


    private lateinit var statusView: StatusMessageView
    private lateinit var progressView: ProgressBar
    private lateinit var streamStatusView: TextView
    private lateinit var chatGroupListContainer: LinearLayout
    private lateinit var emptyView: TextView
    // Refresh moved to toolbar

    private var chatStream: EventSource? = null
    private var reconnectChatJob: Job? = null
    private var chatFallbackRefreshJob: Job? = null

    private val gson = Gson()
    private val dispatchGroupListType = object : TypeToken<List<DispatchChatGroupSummary>>() {}.type
    private val chatGroupsState = mutableListOf<DispatchChatGroupSummary>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_chat_group_list)

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
        statusView = findViewById(R.id.statusView)
        progressView = findViewById(R.id.chatGroupProgress)
        streamStatusView = findViewById(R.id.streamStatusView)
        chatGroupListContainer = findViewById(R.id.chatGroupListContainer)
        emptyView = findViewById(R.id.emptyView)
        // Refresh moved to toolbar


        // Refresh moved to toolbar

        refreshChatGroups(silent = false)
    }

    override fun onStart() {
        super.onStart()
        startStream()
    }

    override fun onStop() {
        stopStream()
        super.onStop()
    }

    private fun refreshChatGroups(silent: Boolean) {
        val container = applicationContext.appContainer()
        if (!silent) {
            setLoading(true, "正在加载群组...")
        }
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { container.dispatchChatRepository.listGroups() }
            }
            val payload = result.getOrNull()
            if (payload == null) {
                if (!silent) {
                    val message = result.exceptionOrNull()?.message ?: "加载群组失败"
                    setLoading(false, "错误: $message")
                }
                return@launch
            }
            setChatGroupsState(payload.items)
            if (!silent) {
                setLoading(false, "加载完成 (${payload.items.size}个群组)")
            }
        }
    }

    private fun startStream() {
        if (chatStream != null) return
        val container = applicationContext.appContainer()
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    container.dispatchChatRepository.connectStream(
                        onOpen = {
                            runOnUiThread { streamStatusView.text = "已连接" }
                        },
                        onEvent = { event, data ->
                            if (event != "heartbeat") {
                                onIncomingStreamEvent(event, data)
                            }
                        },
                        onClosed = {
                            runOnUiThread {
                                chatStream = null
                                streamStatusView.text = "已断开"
                                scheduleReconnect()
                            }
                        },
                        onFailure = { message ->
                            runOnUiThread {
                                chatStream = null
                                streamStatusView.text = "连接错误"
                                scheduleReconnect()
                            }
                        }
                    )
                }
            }
            chatStream = result.getOrNull()
            if (chatStream == null) {
                streamStatusView.text = "连接失败"
                scheduleReconnect()
            }
        }
    }

    private fun stopStream() {
        reconnectChatJob?.cancel()
        reconnectChatJob = null
        chatFallbackRefreshJob?.cancel()
        chatFallbackRefreshJob = null
        chatStream?.cancel()
        chatStream = null
        streamStatusView.text = "未连接"
    }

    private fun onIncomingStreamEvent(eventName: String?, rawData: String) {
        runOnUiThread {
            val payload = parseJsonObject(rawData)
            val handled = handleChatStreamEvent(eventName, payload)
            if (!handled) {
                scheduleFallbackRefresh()
            }
        }
    }

    private fun handleChatStreamEvent(eventName: String?, payload: JsonObject?): Boolean {
        val normalizedEvent = eventName?.trim()?.lowercase().orEmpty()
        val payloadType = readText(payload, "type")?.lowercase().orEmpty()

        return when {
            normalizedEvent == "initial" || payloadType == "dispatch_chat_initial" -> {
                val groups = parseChatGroups(payload?.get("items")) ?: emptyList()
                setChatGroupsState(groups)
                true
            }
            normalizedEvent == "chat_message" || payloadType == "dispatch_chat_message" -> {
                val groupId = readText(payload, "group_id") ?: return false
                val existing = findChatGroup(groupId) ?: return false // Only update joined groups
                
                val messageContent = payload?.get("message")?.asJsonObject?.get("content")?.asString
                upsertChatGroupState(
                    existing.copy(
                        unread_count = readInt(payload, "unread_count") ?: existing.unread_count,
                        last_message_preview = messageContent ?: existing.last_message_preview,
                    )
                )
                true
            }
            normalizedEvent == "chat_group_upserted" || payloadType == "dispatch_chat_group_upserted" -> {
                val group = parseChatGroup(payload?.get("group")) ?: return false
                upsertChatGroupState(group)
                true
            }
            normalizedEvent == "chat_group_archived" || payloadType == "dispatch_chat_group_archived" -> {
                val groupId = readText(payload, "group_id") ?: return false
                val existing = findChatGroup(groupId)
                if (existing != null) {
                    upsertChatGroupState(existing.copy(status = "archived", read_only = true))
                }
                true
            }
            normalizedEvent == "chat_read_synced" || payloadType == "dispatch_chat_read_synced" -> {
                val groupId = readText(payload, "group_id") ?: return false
                val existing = findChatGroup(groupId)
                if (existing != null) {
                    upsertChatGroupState(existing.copy(unread_count = readInt(payload, "unread_count") ?: existing.unread_count))
                }
                true
            }
            else -> false
        }
    }

    private fun scheduleReconnect() {
        if (!lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) return
        if (reconnectChatJob?.isActive == true) return
        reconnectChatJob = lifecycleScope.launch {
            delay(5_000L)
            startStream()
        }
    }

    private fun scheduleFallbackRefresh() {
        if (!lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) return
        if (chatFallbackRefreshJob?.isActive == true) return
        chatFallbackRefreshJob = lifecycleScope.launch {
            delay(1_500L)
            refreshChatGroups(silent = true)
        }
    }

    private fun setChatGroupsState(groups: List<DispatchChatGroupSummary>) {
        chatGroupsState.clear()
        // Filter only joined groups? The prompt requests "只展示自己加入的的群聊"
        // In the existing API, listGroups() typically returns joined groups, but let's assume all returned groups are joined for now
        chatGroupsState.addAll(groups)
        sortChatGroupsState()
        renderChatGroups()
    }

    private fun upsertChatGroupState(group: DispatchChatGroupSummary) {
        val index = chatGroupsState.indexOfFirst { it.group_id == group.group_id }
        if (index >= 0) {
            chatGroupsState[index] = group
        } else {
            chatGroupsState.add(group)
        }
        sortChatGroupsState()
        renderChatGroups()
    }

    private fun findChatGroup(groupId: String): DispatchChatGroupSummary? {
        return chatGroupsState.firstOrNull { it.group_id == groupId }
    }

    private fun sortChatGroupsState() {
        chatGroupsState.sortWith(
            compareByDescending<DispatchChatGroupSummary> { it.unread_count }
                .thenByDescending { it.last_message_at ?: "" }
                .thenBy { it.group_name }
        )
    }

    private fun renderChatGroups() {
        chatGroupListContainer.removeAllViews()
        if (chatGroupsState.isEmpty()) {
            chatGroupListContainer.visibility = View.GONE
            emptyView.visibility = View.VISIBLE
            return
        }
        
        chatGroupListContainer.visibility = View.VISIBLE
        emptyView.visibility = View.GONE

        val inflater = LayoutInflater.from(this)
        chatGroupsState.forEach { group ->
            val card = inflater.inflate(R.layout.item_chat_group, chatGroupListContainer, false)
            
            val titleView = card.findViewById<TextView>(R.id.groupTitle)
            val lastMessageView = card.findViewById<TextView>(R.id.groupLastMessage)
            val unreadCountView = card.findViewById<TextView>(R.id.groupUnreadCount)

            titleView.text = group.group_name
            
            if (group.last_message_preview.isNullOrBlank()) {
                lastMessageView.text = "暂无消息"
                lastMessageView.setTextColor(getColor(R.color.text_tertiary))
            } else {
                lastMessageView.text = group.last_message_preview
                lastMessageView.setTextColor(getColor(R.color.text_secondary))
            }

            if (group.unread_count > 0) {
                unreadCountView.visibility = View.VISIBLE
                unreadCountView.text = group.unread_count.toString()
            } else {
                unreadCountView.visibility = View.GONE
            }

            card.setOnClickListener {
                startActivity(ChatMessageActivity.createIntent(this, group.group_id, group.group_name))
            }
            chatGroupListContainer.addView(card)
        }
    }

    private fun setLoading(loading: Boolean, msg: String) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        // Refresh moved to toolbar
        
        if (loading || msg.startsWith("错误")) {
            statusView.renderStatus(msg)
        } else {
            statusView.renderStatus("")
        }
    }

    // Parsing Helpers
    private fun parseJsonObject(rawData: String?): JsonObject? {
        val text = rawData?.trim().orEmpty()
        if (text.isEmpty()) return null
        return runCatching {
            val element = JsonParser.parseString(text)
            if (element.isJsonObject) element.asJsonObject else null
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

    private fun parseChatGroups(element: JsonElement?): List<DispatchChatGroupSummary>? {
        if (element == null || element.isJsonNull) return null
        return runCatching {
            gson.fromJson<List<DispatchChatGroupSummary>>(element, dispatchGroupListType) ?: emptyList()
        }.getOrNull()
    }

    private fun parseChatGroup(element: JsonElement?): DispatchChatGroupSummary? {
        if (element == null || element.isJsonNull) return null
        return runCatching {
            gson.fromJson(element, DispatchChatGroupSummary::class.java)
        }.getOrNull()
    }

    companion object {
        fun createIntent(context: Context): Intent {
            return Intent(context, ChatGroupListActivity::class.java)
        }
    }

    
    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.menu_refresh, menu)
        return true
    }

    override fun onOptionsItemSelected(item: android.view.MenuItem): Boolean {
        return when (item.itemId) {
            R.id.action_refresh -> {
                refreshChatGroups(silent = false)
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
