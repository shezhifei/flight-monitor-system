package com.flightmonitor.mobile.ui

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.BusinessCase
import com.flightmonitor.mobile.api.model.businessCaseStatusLabel
import com.flightmonitor.mobile.api.model.businessCaseVisibilityLabel

class BusinessCaseAdapter(
    private val onCaseSelected: (BusinessCase) -> Unit,
) : ListAdapter<BusinessCase, BusinessCaseAdapter.ViewHolder>(DiffCallback) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
        val view = LayoutInflater.from(parent.context)
            .inflate(R.layout.item_business_case, parent, false)
        return ViewHolder(view, onCaseSelected)
    }

    override fun onBindViewHolder(holder: ViewHolder, position: Int) {
        holder.bind(getItem(position))
    }

    class ViewHolder(
        itemView: View,
        private val onCaseSelected: (BusinessCase) -> Unit,
    ) : RecyclerView.ViewHolder(itemView) {
        private val card: View = itemView.findViewById(R.id.businessCaseCard)
        private val titleView: TextView = itemView.findViewById(R.id.businessCaseTitle)
        private val statusView: TextView = itemView.findViewById(R.id.businessCaseStatus)
        private val summaryView: TextView = itemView.findViewById(R.id.businessCaseSummary)
        private val metaView: TextView = itemView.findViewById(R.id.businessCaseMeta)

        fun bind(item: BusinessCase) {
            titleView.text = listOfNotNull(
                item.flight_no.takeIf { it.isNotBlank() },
                item.case_type,
            ).joinToString(" · ")
            statusView.text = businessCaseStatusLabel(item.status)
            summaryView.text = item.description.ifBlank { "暂无描述" }
            metaView.text = listOfNotNull(
                "航班ID ${item.flight_id}",
                businessCaseVisibilityLabel(item.visibility_scope, item.department_name_snapshot),
                "追加 ${item.append_count}",
                item.gate?.takeIf { it.isNotBlank() }?.let { "登机口 $it" },
                item.stand?.takeIf { it.isNotBlank() }?.let { "机位 $it" },
            ).joinToString("  ·  ")
            card.setOnClickListener { onCaseSelected(item) }
        }
    }

    companion object DiffCallback : DiffUtil.ItemCallback<BusinessCase>() {
        override fun areItemsTheSame(oldItem: BusinessCase, newItem: BusinessCase): Boolean {
            return oldItem.case_id == newItem.case_id
        }

        override fun areContentsTheSame(oldItem: BusinessCase, newItem: BusinessCase): Boolean {
            return oldItem == newItem
        }
    }
}
