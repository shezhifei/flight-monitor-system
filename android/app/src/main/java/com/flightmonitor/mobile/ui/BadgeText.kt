package com.flightmonitor.mobile.ui

import android.content.Context
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.style.BackgroundColorSpan
import android.text.style.ForegroundColorSpan
import androidx.core.content.ContextCompat
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.ui.model.OriginKind
import com.flightmonitor.mobile.ui.model.OriginUiModel
import com.flightmonitor.mobile.ui.model.ReceiptStatusKind
import com.flightmonitor.mobile.ui.model.ReceiptStatusUiModel

data class BadgeToken(
    val label: String,
    val backgroundRes: Int,
    val textColorRes: Int,
)

fun buildOriginBadge(origin: OriginUiModel): BadgeToken {
    return when (origin.kind) {
        OriginKind.WORKFLOW -> BadgeToken(origin.label, R.color.origin_workflow_bg, R.color.origin_workflow_text)
        OriginKind.MANUAL -> BadgeToken(origin.label, R.color.origin_manual_bg, R.color.origin_manual_text)
    }
}

fun buildReceiptStatusBadge(status: ReceiptStatusUiModel): BadgeToken {
    return when (status.kind) {
        ReceiptStatusKind.ACKNOWLEDGED -> BadgeToken(status.label, R.color.status_success_bg, R.color.status_success_text)
        ReceiptStatusKind.REJECTED -> BadgeToken(status.label, R.color.status_error_bg, R.color.status_error_text)
        ReceiptStatusKind.PENDING -> BadgeToken(status.label, R.color.ack_pending_bg, R.color.ack_pending_text)
    }
}

fun buildReceiptRequiredBadge(required: Boolean): BadgeToken? {
    return if (required) {
        BadgeToken("需回执", R.color.status_warning_bg, R.color.status_warning_text)
    } else {
        null
    }
}

fun badgeLine(context: Context, badges: List<BadgeToken>, suffix: String): CharSequence {
    val builder = SpannableStringBuilder()
    badges.filter { it.label.isNotBlank() }.forEachIndexed { index, badge ->
        val start = builder.length
        builder.append(" ")
        builder.append(badge.label)
        builder.append(" ")
        val end = builder.length
        builder.setSpan(
            BackgroundColorSpan(ContextCompat.getColor(context, badge.backgroundRes)),
            start,
            end,
            Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        builder.setSpan(
            ForegroundColorSpan(ContextCompat.getColor(context, badge.textColorRes)),
            start,
            end,
            Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        if (index < badges.lastIndex) {
            builder.append("  ")
        }
    }
    if (suffix.isNotBlank()) {
        if (builder.isNotEmpty()) {
            builder.append(" ")
        }
        builder.append(suffix)
    }
    return builder
}
