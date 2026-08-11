package com.flightmonitor.mobile.ui

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.EditText
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.DispatchChatMessage
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

class ChatMessageActivity : AppCompatActivity() {


    private lateinit var streamStatusView: TextView
    private lateinit var statusView: StatusMessageView
    private lateinit var chatScrollView: ScrollView
    private lateinit var chatMessagesContainer: LinearLayout
    private lateinit var chatMessageInput: EditText
    private lateinit var sendChatMessageButton: MaterialButton

    private var groupId: String = ""
    private var groupName: String = ""
    private var chatStream: EventSource? = null
    private var reconnectChatJob: Job? = null
    private var chatFallbackRefreshJob: Job? = null

    private val gson = Gson()
    private val chatMessagesState = mutableListOf<DispatchChatMessage>()
    // Current user's logic normally checks against local ID to differentiate sent vs received. 
    // We'll approximate this by looking for 'sender_username' vs 'unknown' or assuming API gives hints.
    // For now we'll display them all generally, but styling them slightly differently if possible.

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_chat_message)

        val rootView = findViewById<View>(android.R.id.content)
        // 让系统不自动处理 insets，由我们手动分配 padding
        androidx.core.view.WindowCompat.setDecorFitsSystemWindows(window, false)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            val ime = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.ime())
            // 底部取 systemBars 和 IME 的最大值，确保键盘弹出时输入栏上移
            val bottomPadding = maxOf(systemBars.bottom, ime.bottom)
            view.setPadding(systemBars.left, systemBars.top, systemBars.right, bottomPadding)
            insets
        }

        groupId = intent.getStringExtra(EXTRA_GROUP_ID) ?: ""
        groupName = intent.getStringExtra(EXTRA_GROUP_NAME) ?: "群聊详情"

        if (groupId.isBlank()) {
            finish()
            return
        }


        // Toolbar setup
        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
        streamStatusView = findViewById(R.id.streamStatusView)
        statusView = findViewById(R.id.statusView)
        chatScrollView = findViewById(R.id.chatScrollView)
        chatMessagesContainer = findViewById(R.id.chatMessagesContainer)
        chatMessageInput = findViewById(R.id.chatMessageInput)
        sendChatMessageButton = findViewById(R.id.sendChatMessageButton)

        supportActionBar?.title = groupName
        sendChatMessageButton.setOnClickListener { sendChatMessage() }

        refreshChatMessages(silent = false)
    }

    override fun onStart() {
        super.onStart()
        startStream()
    }

    override fun onStop() {
        stopStream()
        super.onStop()
    }

    private fun refreshChatMessages(silent: Boolean) {
        val container = applicationContext.appContainer()
        if (!silent) setLoading(true, "正在加载消息...")
        
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { container.dispatchChatRepository.listMessages(groupId = groupId) }
            }
            val payload = result.getOrNull()
            if (payload == null) {
                if (!silent) {
                    val message = result.exceptionOrNull()?.message ?: "加载消息失败"
                    setLoading(false, "错误: $message")
                }
                return@launch
            }
            setChatMessagesState(payload.items)
            if (!silent) setLoading(false, "")
            
            // Mark as read after load
            val maxSeq = payload.items.maxOfOrNull { it.seq_no }
            if (maxSeq != null) {
               markChatRead(maxSeq)
            }
        }
    }

    private fun sendChatMessage() {
        val content = chatMessageInput.text?.toString()?.trim().orEmpty()
        if (content.isBlank()) return

        val container = applicationContext.appContainer()
        chatMessageInput.isEnabled = false
        sendChatMessageButton.isEnabled = false
        setLoading(true, "正在发送...")

        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    container.dispatchChatRepository.sendMessage(
                        groupId = groupId,
                        content = content,
                        atAll = false
                    )
                }
            }
            chatMessageInput.isEnabled = true
            sendChatMessageButton.isEnabled = true
            if (result.isSuccess) {
                val message = result.getOrThrow()
                chatMessageInput.setText("")
                upsertChatMessageState(message)
                setLoading(false, "")
            } else {
                val err = result.exceptionOrNull()?.message ?: "发送失败"
                setLoading(false, "错误: $err")
            }
        }
    }

    private fun markChatRead(readSeq: Int) {
        val container = applicationContext.appContainer()
        lifecycleScope.launch {
            withContext(Dispatchers.IO) {
                runCatching {
                    container.dispatchChatRepository.markRead(groupId = groupId, readSeq = readSeq)
                }
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
                                streamStatusView.text = "连接出错"
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
            normalizedEvent == "chat_message" || payloadType == "dispatch_chat_message" -> {
                val message = parseChatMessage(payload?.get("message")) ?: return false
                val incomingGroupId = readText(payload, "group_id") ?: message.group_id
                
                if (incomingGroupId == this.groupId) {
                    upsertChatMessageState(message)
                    markChatRead(message.seq_no)
                    true
                } else {
                    false
                }
            }
            // Other events might be irrelevant to the specific chat list view except read_synced which we don't display
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
            refreshChatMessages(silent = true)
        }
    }

    private fun setChatMessagesState(messages: List<DispatchChatMessage>) {
        chatMessagesState.clear()
        chatMessagesState.addAll(normalizeMessages(messages))
        renderChatMessages()
    }

    private fun upsertChatMessageState(message: DispatchChatMessage) {
        val key = normalizeMessageKey(message)
        val index = chatMessagesState.indexOfFirst { normalizeMessageKey(it) == key }
        if (index >= 0) {
            chatMessagesState[index] = message
        } else {
            chatMessagesState.add(message)
        }
        val normalized = normalizeMessages(chatMessagesState)
        chatMessagesState.clear()
        chatMessagesState.addAll(normalized)
        renderChatMessages()
    }

    private fun normalizeMessages(items: List<DispatchChatMessage>): List<DispatchChatMessage> {
        if (items.isEmpty()) return emptyList()
        val map = linkedMapOf<String, DispatchChatMessage>()
        items.sortedBy { it.seq_no }.forEach { message ->
            map[normalizeMessageKey(message)] = message
        }
        return map.values.sortedBy { it.seq_no }
    }

    private fun normalizeMessageKey(message: DispatchChatMessage): String {
        return message.message_id.ifBlank { "${message.group_id}_${message.seq_no}" }
    }

    private fun renderChatMessages() {
        chatMessagesContainer.removeAllViews()

        for (message in chatMessagesState) {
            val messageView = formatMessageView(message)
            chatMessagesContainer.addView(messageView)
        }
        
        // Scroll to bottom
        chatScrollView.post {
            chatScrollView.fullScroll(View.FOCUS_DOWN)
        }
    }

    private fun formatMessageView(message: DispatchChatMessage): View {
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, 
                LinearLayout.LayoutParams.WRAP_CONTENT
            ).apply {
                bottomMargin = dpToPx(12)
            }
        }

        val sender = message.sender_username ?: message.sender_user_id ?: "unknown"
        val isSystem = message.message_type.equals("system", ignoreCase = true)
        
        // Sender Name Header
        if (!isSystem) {
            val nameView = TextView(this).apply {
                text = sender
                textSize = 12f
                setTextColor(getColor(R.color.text_tertiary))
                layoutParams = LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                    LinearLayout.LayoutParams.WRAP_CONTENT
                ).apply {
                    bottomMargin = dpToPx(4)
                }
            }
            container.addView(nameView)
        }

        // Message Bubble
        val bubble = TextView(this).apply {
            text = message.content
            textSize = 15f
            setTextColor(getColor(R.color.text_primary))
            setPadding(dpToPx(12), dpToPx(8), dpToPx(12), dpToPx(8))
            
            layoutParams = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.WRAP_CONTENT,
                LinearLayout.LayoutParams.WRAP_CONTENT
            ).apply {
                if (isSystem) {
                    gravity = Gravity.CENTER_HORIZONTAL
                    background = getDrawable(R.drawable.badge_bg)
                    backgroundTintList = getColorStateList(R.color.badge_gray_bg)
                    setTextColor(getColor(R.color.text_secondary))
                    textSize = 12f
                } else {
                    // Default to received styling
                    background = getDrawable(R.drawable.bg_chat_bubble_received)
                }
            }
        }
        
        container.addView(bubble)
        return container
    }

    private fun dpToPx(dp: Int): Int {
        return (dp * resources.displayMetrics.density).toInt()
    }

    private fun setLoading(loading: Boolean, msg: String) {
        if (msg.isNotEmpty() || loading) {
            statusView.renderStatus(msg)
            if(loading && msg.isEmpty()) {
               statusView.renderStatus("加载中...")
            }
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

    private fun parseChatMessage(element: JsonElement?): DispatchChatMessage? {
        if (element == null || element.isJsonNull) return null
        return runCatching {
            gson.fromJson(element, DispatchChatMessage::class.java)
        }.getOrNull()
    }

    companion object {
        const val EXTRA_GROUP_ID = "extra_group_id"
        const val EXTRA_GROUP_NAME = "extra_group_name"

        fun createIntent(context: Context, groupId: String, groupName: String): Intent {
            return Intent(context, ChatMessageActivity::class.java).apply {
                putExtra(EXTRA_GROUP_ID, groupId)
                putExtra(EXTRA_GROUP_NAME, groupName)
            }
        }
    }

    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }
}
