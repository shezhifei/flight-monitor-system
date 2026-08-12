package com.flightmonitor.mobile.ui

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.BusinessCaseAppendEntry
import com.flightmonitor.mobile.api.model.acknowledgmentMap
import com.flightmonitor.mobile.api.model.mentionUserIds
import com.google.android.material.button.MaterialButton

class BusinessCaseAppendAdapter(
    private val currentUserId: () -> String?,
    private val onAcknowledge: (BusinessCaseAppendEntry) -> Unit,
) : ListAdapter<BusinessCaseAppendEntry, BusinessCaseAppendAdapter.ViewHolder>(DiffCallback) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
        val view = LayoutInflater.from(parent.context)
            .inflate(R.layout.item_business_case_append, parent, false)
        return ViewHolder(view, currentUserId, onAcknowledge)
    }

    override fun onBindViewHolder(holder: ViewHolder, position: Int) {
        holder.bind(getItem(position))
    }

    class ViewHolder(
        itemView: View,
        private val currentUserId: () -> String?,
        private val onAcknowledge: (BusinessCaseAppendEntry) -> Unit,
    ) : RecyclerView.ViewHolder(itemView) {
        private val contentView: TextView = itemView.findViewById(R.id.appendContent)
        private val metaView: TextView = itemView.findViewById(R.id.appendMeta)
        private val statusView: TextView = itemView.findViewById(R.id.appendStatus)
        private val acknowledgeButton: MaterialButton = itemView.findViewById(R.id.appendAcknowledgeButton)

        fun bind(item: BusinessCaseAppendEntry) {
            contentView.text = item.content
            metaView.text = listOfNotNull(
                item.submitted_operator_name ?: item.submitted_by,
                item.appended_at,
            ).joinToString("  ·  ")

            val mentionIds = item.mentionUserIds()
            val acknowledgments = item.acknowledgmentMap()
            val selfUserId = currentUserId()
            val selfCanAck = !selfUserId.isNullOrBlank() && mentionIds.contains(selfUserId)
            val selfAcked = !selfUserId.isNullOrBlank() && acknowledgments.containsKey(selfUserId)

            statusView.text = buildString {
                if (mentionIds.isNotEmpty()) {
                    append("提及 ${mentionIds.size} 人")
                } else {
                    append("未指定确认人")
                }
                append("  ·  已确认 ${acknowledgments.size}")
                if (selfAcked) {
                    append("  ·  我已确认")
                }
            }

            acknowledgeButton.visibility = if (selfCanAck && !selfAcked) View.VISIBLE else View.GONE
            acknowledgeButton.setOnClickListener { onAcknowledge(item) }
        }
    }

    companion object DiffCallback : DiffUtil.ItemCallback<BusinessCaseAppendEntry>() {
        override fun areItemsTheSame(
            oldItem: BusinessCaseAppendEntry,
            newItem: BusinessCaseAppendEntry,
        ): Boolean {
            return oldItem.append_id == newItem.append_id
        }

        override fun areContentsTheSame(
            oldItem: BusinessCaseAppendEntry,
            newItem: BusinessCaseAppendEntry,
        ): Boolean {
            return oldItem == newItem
        }
    }
}
