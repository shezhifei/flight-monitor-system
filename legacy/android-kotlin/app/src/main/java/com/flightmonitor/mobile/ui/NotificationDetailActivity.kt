package com.flightmonitor.mobile.ui

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.View
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.NotificationItem
import com.flightmonitor.mobile.api.model.NotificationReceipt
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.model.CollaborationUiMapper
import com.flightmonitor.mobile.ui.model.ReceiptStatusKind
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import com.google.android.material.button.MaterialButton
import com.google.android.material.textfield.TextInputEditText
import com.google.android.material.textfield.TextInputLayout
import com.google.gson.Gson
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class NotificationDetailActivity : AppCompatActivity() {
    private lateinit var headlineView: TextView
    private lateinit var metaView: TextView
    private lateinit var badgeContainer: LinearLayout
    private lateinit var bodyView: TextView
    private lateinit var noteInputLayout: TextInputLayout
    private lateinit var noteInput: TextInputEditText
    private lateinit var acknowledgeButton: MaterialButton
    private lateinit var rejectButton: MaterialButton
    private lateinit var receiptGroupButton: MaterialButton

    private lateinit var progressView: ProgressBar
    private lateinit var statusView: StatusMessageView

    private val gson = Gson()
    private var currentNotification: NotificationItem? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_notification_detail)

        // WindowInsets edge-to-edge
        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        headlineView = findViewById(R.id.notificationDetailHeadlineView)
        metaView = findViewById(R.id.notificationDetailMetaView)
        badgeContainer = findViewById(R.id.badgeContainer)
        bodyView = findViewById(R.id.notificationDetailBodyView)
        noteInputLayout = findViewById(R.id.noteInputLayout)
        noteInput = findViewById(R.id.notificationDetailNoteInput)
        acknowledgeButton = findViewById(R.id.notificationDetailAcknowledgeButton)
        rejectButton = findViewById(R.id.notificationDetailRejectButton)
        receiptGroupButton = findViewById(R.id.notificationDetailReceiptGroupButton)

        // Toolbar setup
        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
        progressView = findViewById(R.id.notificationDetailProgress)
        statusView = findViewById(R.id.notificationDetailStatusView)

        acknowledgeButton.setOnClickListener { confirmAction("acknowledged") }
        rejectButton.setOnClickListener { confirmAction("rejected") }
        receiptGroupButton.setOnClickListener { openReceiptGroup() }


        currentNotification = readNotificationExtra()
        val notification = currentNotification
        if (notification == null) {
            statusView.renderStatus(getString(R.string.notification_detail_not_found))
            setLoading(false)
            return
        }
        bindNotification(notification)
        markRead(notification.notification_id)
    }

    private fun readNotificationExtra(): NotificationItem? {
        val raw = intent.getStringExtra(EXTRA_NOTIFICATION_JSON).orEmpty()
        if (raw.isBlank()) {
            return null
        }
        return runCatching { gson.fromJson(raw, NotificationItem::class.java) }.getOrNull()
    }

    private fun bindNotification(notification: NotificationItem) {
        currentNotification = notification
        headlineView.text = notification.title
        metaView.text = buildString {
            append(getString(R.string.notification_detail_origin_label, notification.origin_label))
            append("\n")
            append(getString(R.string.notification_detail_time_label, notification.created_at))
            notification.receipt_group_id?.takeIf { it.isNotBlank() }?.let {
                append("\n")
                append(getString(R.string.notification_detail_receipt_group_label, it))
            }
        }

        // 独立 badge 渲染
        val origin = CollaborationUiMapper.mapOrigin(notification.origin_type, notification.origin_label)
        val receiptStatus = CollaborationUiMapper.mapReceiptStatus(notification.ack_status)

        badgeContainer.removeAllViews()
        addBadgeView(buildOriginBadge(origin))
        buildReceiptRequiredBadge(notification.receipt_required)?.let { addBadgeView(it) }
        addBadgeView(buildReceiptStatusBadge(receiptStatus))

        bodyView.text = notification.body

        val actionState = CollaborationUiMapper.mapNotificationActionState(notification.ack_status, notification.receipt_required)
        acknowledgeButton.visibility = if (actionState.canSubmit) View.VISIBLE else View.GONE
        rejectButton.visibility = if (actionState.canSubmit) View.VISIBLE else View.GONE
        noteInputLayout.visibility = if (actionState.canSubmit) View.VISIBLE else View.GONE
        receiptGroupButton.visibility = if (notification.receipt_group_id.isNullOrBlank()) View.GONE else View.VISIBLE
        if (!actionState.canSubmit) {
            noteInput.setText(notification.ack_note.orEmpty())
            noteInput.isEnabled = false
            // 已处理的通知也显示备注区域（只读）
            if (!notification.ack_note.isNullOrBlank()) {
                noteInputLayout.visibility = View.VISIBLE
                noteInputLayout.hint = "回执备注"
            }
        } else {
            noteInput.isEnabled = true
        }
        statusView.renderStatus(getString(R.string.notification_detail_final_status, actionState.finalStatusLabel))
    }

    /** 为 badgeContainer 添加独立 Badge TextView */
    private fun addBadgeView(badge: BadgeToken) {
        if (badge.label.isBlank()) return
        val tv = TextView(this).apply {
            text = badge.label
            textSize = 12f
            setTypeface(null, android.graphics.Typeface.BOLD)
            setTextColor(ContextCompat.getColor(this@NotificationDetailActivity, badge.textColorRes))
            setBackgroundResource(R.drawable.badge_bg)
            backgroundTintList = android.content.res.ColorStateList.valueOf(
                ContextCompat.getColor(this@NotificationDetailActivity, badge.backgroundRes)
            )
            val hPad = (10 * resources.displayMetrics.density).toInt()
            val vPad = (4 * resources.displayMetrics.density).toInt()
            setPadding(hPad, vPad, hPad, vPad)
        }
        val lp = LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.WRAP_CONTENT,
            LinearLayout.LayoutParams.WRAP_CONTENT,
        )
        lp.marginEnd = (8 * resources.displayMetrics.density).toInt()
        tv.layoutParams = lp
        badgeContainer.addView(tv)
    }

    /** 二次确认对话框 */
    private fun confirmAction(action: String) {
        val actionLabel = if (action == "acknowledged") "确认" else "拒绝"
        val note = noteInput.text?.toString()?.trim().orEmpty()

        // 拒绝前先校验理由
        if (action == "rejected" && note.isBlank()) {
            noteInputLayout.error = "拒绝通知必须填写理由"
            return
        }
        noteInputLayout.error = null

        AlertDialog.Builder(this)
            .setTitle("${actionLabel}通知")
            .setMessage("确定要${actionLabel}这条通知吗？此操作不可撤销。")
            .setPositiveButton(actionLabel) { _, _ -> submitAction(action) }
            .setNegativeButton("取消", null)
            .show()
    }

    private fun submitAction(action: String) {
        val notification = currentNotification ?: return
        val note = noteInput.text?.toString()?.trim().orEmpty().ifBlank { null }
        CollaborationUiMapper.validateAcknowledgement(action, note)?.let { error ->
            statusView.renderStatus(error)
            return
        }
        setLoading(true)
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    applicationContext.appContainer().notificationRepository.acknowledge(
                        notificationId = notification.notification_id,
                        action = action,
                        note = note,
                    )
                }
            }
            val receipt = result.getOrNull()
            if (receipt == null) {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.collab_ack_notification_failed)
                setLoading(false, getString(R.string.error_prefix, message))
                return@launch
            }
            bindNotification(updateNotification(notification, receipt))
            setLoading(false, getString(R.string.collab_ack_notification_success, if (action == "acknowledged") getString(R.string.collab_ack_action_acknowledged) else getString(R.string.collab_ack_action_rejected)))
        }
    }

    private fun updateNotification(notification: NotificationItem, receipt: NotificationReceipt): NotificationItem {
        return notification.copy(
            ack_status = receipt.ack_status,
            ack_at = receipt.ack_at,
            ack_note = receipt.ack_note,
            read_status = receipt.read_status,
            read_at = receipt.read_at,
            is_read = receipt.read_status.equals("read", ignoreCase = true),
        )
    }

    private fun markRead(notificationId: String) {
        lifecycleScope.launch {
            withContext(Dispatchers.IO) {
                runCatching { applicationContext.appContainer().notificationRepository.markRead(notificationId) }
            }
        }
    }

    private fun openReceiptGroup() {
        val notification = currentNotification ?: return
        val receiptGroupId = notification.receipt_group_id.orEmpty()
        if (receiptGroupId.isBlank()) {
            statusView.renderStatus(getString(R.string.notification_receipt_group_not_found))
            return
        }
        startActivity(NotificationReceiptGroupActivity.createIntent(this, receiptGroupId))
    }

    private fun setLoading(loading: Boolean, statusText: String? = null) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE
        acknowledgeButton.isEnabled = !loading
        rejectButton.isEnabled = !loading
        receiptGroupButton.isEnabled = !loading

        noteInput.isEnabled = !loading && acknowledgeButton.visibility == View.VISIBLE
        statusText?.let { statusView.renderStatus(it) }
    }

    companion object {
        private const val EXTRA_NOTIFICATION_JSON = "notification_json"

        fun createIntent(context: Context, notification: NotificationItem): Intent {
            return Intent(context, NotificationDetailActivity::class.java)
                .putExtra(EXTRA_NOTIFICATION_JSON, Gson().toJson(notification))
        }
    }

    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }
}
