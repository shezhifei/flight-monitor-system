package com.flightmonitor.mobile.ui.widget

import android.content.Context
import android.graphics.Typeface
import android.util.AttributeSet
import android.view.LayoutInflater
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.TextView
import androidx.core.content.ContextCompat
import com.flightmonitor.mobile.R

class StatusMessageView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : FrameLayout(context, attrs, defStyleAttr) {

    private val iconContainer: FrameLayout
    private val iconView: ImageView
    private val titleView: TextView
    private val messageView: TextView

    init {
        LayoutInflater.from(context).inflate(R.layout.layout_status_message, this, true)
        iconContainer = findViewById(R.id.statusIconContainer)
        iconView = findViewById(R.id.statusIcon)
        titleView = findViewById(R.id.statusTitle)
        messageView = findViewById(R.id.statusMessage)

        // Hide by default
        visibility = View.GONE
    }

    /**
     * Renders a status message into this container.
     * Pass an empty/blank string to hide the view.
     */
    fun renderStatus(message: String, title: String? = null) {
        if (message.isBlank() && title.isNullOrBlank()) {
            visibility = View.GONE
            return
        }

        visibility = View.VISIBLE
        messageView.text = message
        messageView.visibility = if (message.isNotBlank()) View.VISIBLE else View.GONE

        if (!title.isNullOrBlank()) {
            titleView.text = title
            titleView.visibility = View.VISIBLE
            messageView.setTypeface(null, Typeface.NORMAL)
            messageView.setTextColor(ContextCompat.getColor(context, R.color.text_secondary))
        } else {
            titleView.visibility = View.GONE
            messageView.setTypeface(null, Typeface.BOLD)
            val tone = StatusTone.fromMessage(message)
            messageView.setTextColor(ContextCompat.getColor(context, tone.textColorRes))
        }

        val primaryTone = StatusTone.fromMessage(title ?: message)

        // Update icon & background
        iconContainer.setBackgroundResource(primaryTone.backgroundRes)
        iconView.setImageResource(primaryTone.iconRes)
        iconView.setColorFilter(ContextCompat.getColor(context, primaryTone.textColorRes))

        // Color the title if present
        if (!title.isNullOrBlank()) {
            titleView.setTextColor(ContextCompat.getColor(context, primaryTone.textColorRes))
        }
    }
}

enum class StatusTone(
    val backgroundRes: Int,
    val textColorRes: Int,
    val iconRes: Int,
) {
    INFO(
        backgroundRes = R.drawable.bg_status_info,
        textColorRes = R.color.status_info_text,
        iconRes = R.drawable.ic_status_info,
    ),
    SUCCESS(
        backgroundRes = R.drawable.bg_status_success,
        textColorRes = R.color.status_success_text,
        iconRes = R.drawable.ic_status_success,
    ),
    WARNING(
        backgroundRes = R.drawable.bg_status_warning,
        textColorRes = R.color.status_warning_text,
        iconRes = R.drawable.ic_warning,
    ),
    ERROR(
        backgroundRes = R.drawable.bg_status_error,
        textColorRes = R.color.status_error_text,
        iconRes = R.drawable.ic_status_error,
    ),
    ;

    companion object {
        fun fromMessage(message: String): StatusTone {
            val normalized = message.lowercase()
            return when {
                normalized.contains("错误") ||
                    normalized.contains("失败") ||
                    normalized.contains("拒绝") ||
                    normalized.contains("invalid") ||
                    normalized.contains("error") -> ERROR

                normalized.contains("已完成") ||
                    normalized.contains("成功") ||
                    normalized.contains("已加载") ||
                    normalized.contains("已更新") ||
                    normalized.contains("完成") ||
                    normalized.contains("connected") -> SUCCESS

                normalized.contains("待") ||
                    normalized.contains("请先") ||
                    normalized.contains("加载中") ||
                    normalized.contains("提交中") ||
                    normalized.contains("重连") -> WARNING

                else -> INFO
            }
        }
    }
}
