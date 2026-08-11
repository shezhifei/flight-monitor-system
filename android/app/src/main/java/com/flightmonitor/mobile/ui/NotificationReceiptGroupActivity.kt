package com.flightmonitor.mobile.ui

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.di.appContainer
import com.flightmonitor.mobile.ui.model.CollaborationUiMapper
import com.flightmonitor.mobile.ui.model.NotificationReceiptGroupItemUiModel
import com.flightmonitor.mobile.ui.model.ReceiptStatusKind
import com.flightmonitor.mobile.ui.widget.StatusMessageView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class NotificationReceiptGroupActivity : AppCompatActivity() {
    private lateinit var metaView: TextView
    private lateinit var summaryView: TextView
    private lateinit var receiptProgressBar: ProgressBar
    private lateinit var progressAcknowledgedLabel: TextView
    private lateinit var progressRejectedLabel: TextView
    private lateinit var progressPendingLabel: TextView
    private lateinit var itemsContainer: LinearLayout
    private lateinit var progressView: ProgressBar
    private lateinit var statusView: StatusMessageView


    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        UIWindowInsets.applySystemBarStyle(this, isLightStatusIcon = false)
        setContentView(R.layout.activity_notification_receipt_group)

        // WindowInsets edge-to-edge
        val rootView = findViewById<View>(android.R.id.content)
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(rootView) { view, insets ->
            val systemBars = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            view.setPadding(view.paddingLeft, systemBars.top, view.paddingRight, systemBars.bottom)
            insets
        }

        metaView = findViewById(R.id.receiptGroupMetaView)
        summaryView = findViewById(R.id.receiptGroupSummaryView)
        receiptProgressBar = findViewById(R.id.receiptProgressBar)
        progressAcknowledgedLabel = findViewById(R.id.progressAcknowledgedLabel)
        progressRejectedLabel = findViewById(R.id.progressRejectedLabel)
        progressPendingLabel = findViewById(R.id.progressPendingLabel)
        itemsContainer = findViewById(R.id.receiptGroupItemsContainer)
        progressView = findViewById(R.id.receiptGroupProgress)
        statusView = findViewById(R.id.receiptGroupStatusView)

        // Toolbar setup
        val toolbar = findViewById<androidx.appcompat.widget.Toolbar>(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)

        val receiptGroupId = intent.getStringExtra(EXTRA_RECEIPT_GROUP_ID).orEmpty()
        if (receiptGroupId.isBlank()) {
            statusView.renderStatus(getString(R.string.notification_receipt_group_not_found))
            return
        }
        loadReceiptGroup(receiptGroupId)
    }

    private fun loadReceiptGroup(receiptGroupId: String) {
        setLoading(true)
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { applicationContext.appContainer().notificationRepository.getReceiptGroup(receiptGroupId) }
            }
            val group = result.getOrNull()
            if (group == null) {
                val message = result.exceptionOrNull()?.message ?: getString(R.string.notification_receipt_group_load_failed)
                setLoading(false, getString(R.string.error_prefix, message))
                return@launch
            }
            val ui = CollaborationUiMapper.mapReceiptGroup(group)
            metaView.text = badgeLine(
                this@NotificationReceiptGroupActivity,
                buildList {
                    add(buildOriginBadge(ui.origin))
                    buildReceiptRequiredBadge(ui.receiptRequired)?.let { add(it) }
                },
                ui.title.orEmpty(),
            )
            summaryView.text = getString(
                R.string.notification_receipt_group_summary,
                ui.summary.totalCount,
                ui.summary.pendingCount,
                ui.summary.acknowledgedCount,
                ui.summary.rejectedCount,
                ui.summary.latestUpdatedAt ?: "-",
            )

            // 可视化进度条
            val total = ui.summary.totalCount.coerceAtLeast(1)
            val ackPercent = (ui.summary.acknowledgedCount * 100) / total
            receiptProgressBar.max = 100
            receiptProgressBar.progress = ackPercent
            progressAcknowledgedLabel.text = "✓ 已确认 ${ui.summary.acknowledgedCount}"
            progressRejectedLabel.text = "✗ 已拒绝 ${ui.summary.rejectedCount}"
            progressPendingLabel.text = "○ 待回执 ${ui.summary.pendingCount}"

            // 卡片式成员列表
            renderReceiptItems(ui.items)

            setLoading(false, getString(R.string.collab_refresh_success))
        }
    }

    /** 为每个回执成员生成独立卡片 */
    private fun renderReceiptItems(items: List<NotificationReceiptGroupItemUiModel>) {
        itemsContainer.removeAllViews()
        if (items.isEmpty()) return

        val inflater = LayoutInflater.from(this)
        for (item in items) {
            val cardView = inflater.inflate(R.layout.item_receipt_group_member, itemsContainer, false)
            bindReceiptMemberCard(cardView, item)
            itemsContainer.addView(cardView)
        }
    }

    private fun bindReceiptMemberCard(view: View, item: NotificationReceiptGroupItemUiModel) {
        val statusColorBar = view.findViewById<View>(R.id.statusColorBar)
        val userIdView = view.findViewById<TextView>(R.id.userIdView)
        val receiptStatusBadge = view.findViewById<TextView>(R.id.receiptStatusBadge)
        val deliveryStatusView = view.findViewById<TextView>(R.id.deliveryStatusView)
        val readStatusView = view.findViewById<TextView>(R.id.readStatusView)
        val ackNoteView = view.findViewById<TextView>(R.id.ackNoteView)

        // 左侧颜色条
        val barColor = when (item.receiptStatus.kind) {
            ReceiptStatusKind.ACKNOWLEDGED -> R.color.status_success_text
            ReceiptStatusKind.REJECTED -> R.color.status_error_text
            ReceiptStatusKind.PENDING -> R.color.ack_pending_text
        }
        statusColorBar.setBackgroundColor(ContextCompat.getColor(this, barColor))

        // 用户 ID
        userIdView.text = item.userId

        // 回执状态 badge
        receiptStatusBadge.text = item.receiptStatus.label
        val badgeToken = buildReceiptStatusBadge(item.receiptStatus)
        receiptStatusBadge.setTextColor(ContextCompat.getColor(this, badgeToken.textColorRes))
        receiptStatusBadge.backgroundTintList = android.content.res.ColorStateList.valueOf(
            ContextCompat.getColor(this, badgeToken.backgroundRes)
        )

        // 投递 / 阅读状态
        deliveryStatusView.text = mapDeliveryStatus(item.deliveryStatus)
        readStatusView.text = mapReadStatus(item.readStatus)

        // 备注（仅拒绝时显示）
        if (!item.ackNote.isNullOrBlank()) {
            ackNoteView.visibility = View.VISIBLE
            ackNoteView.text = "理由: ${item.ackNote}"
        } else {
            ackNoteView.visibility = View.GONE
        }
    }

    private fun mapDeliveryStatus(status: String): String {
        return when (status.lowercase()) {
            "sent" -> "已发送"
            "delivered" -> "已送达"
            "failed" -> "发送失败"
            else -> status
        }
    }

    private fun mapReadStatus(status: String): String {
        return when (status.lowercase()) {
            "read" -> "已阅读"
            "unread" -> "未阅读"
            else -> status
        }
    }

    private fun setLoading(loading: Boolean, statusText: String? = null) {
        progressView.visibility = if (loading) View.VISIBLE else View.GONE

        statusText?.let { statusView.renderStatus(it) }
    }

    companion object {
        private const val EXTRA_RECEIPT_GROUP_ID = "receipt_group_id"

        fun createIntent(context: Context, receiptGroupId: String): Intent {
            return Intent(context, NotificationReceiptGroupActivity::class.java)
                .putExtra(EXTRA_RECEIPT_GROUP_ID, receiptGroupId)
        }
    }

    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }
}
