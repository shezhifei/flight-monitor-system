package com.flightmonitor.mobile.ui

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.ShiftHandover

class ShiftHandoverAdapter(
    private val onHandoverSelected: (ShiftHandover) -> Unit
) : ListAdapter<ShiftHandover, ShiftHandoverAdapter.ViewHolder>(DiffCallback) {

    private var selectedHandoverId: String? = null

    fun setSelectedHandoverId(id: String?) {
        val previousSelectedId = selectedHandoverId
        selectedHandoverId = id
        
        // Find positions and update them specifically for better animation/performance
        if (previousSelectedId != null) {
            val prevPos = currentList.indexOfFirst { it.handover_id == previousSelectedId }
            if (prevPos != -1) notifyItemChanged(prevPos)
        }
        if (id != null) {
            val newPos = currentList.indexOfFirst { it.handover_id == id }
            if (newPos != -1) notifyItemChanged(newPos)
        }
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
        val view = LayoutInflater.from(parent.context).inflate(R.layout.item_shift_handover, parent, false)
        return ViewHolder(view, onHandoverSelected)
    }

    override fun onBindViewHolder(holder: ViewHolder, position: Int) {
        holder.bind(getItem(position), getItem(position).handover_id == selectedHandoverId)
    }

    class ViewHolder(
        itemView: View,
        private val onHandoverSelected: (ShiftHandover) -> Unit
    ) : RecyclerView.ViewHolder(itemView) {
        private val card: View = itemView.findViewById(R.id.handoverCard)
        private val title: TextView = itemView.findViewById(R.id.handoverTitle)
        private val riskLabel: TextView = itemView.findViewById(R.id.handoverRiskLabel)
        private val subtitle: TextView = itemView.findViewById(R.id.handoverSubtitle)

        fun bind(handover: ShiftHandover, isSelected: Boolean) {
            card.setBackgroundResource(
                if (isSelected) R.drawable.bg_status_info else R.drawable.bg_card_surface
            )

            title.text = "${handover.shift_date}  ${handover.shift_code}"
            title.setTextColor(
                itemView.context.resources.getColor(
                    if (isSelected) R.color.status_info_text else R.color.text_primary
                )
            )

            riskLabel.text = "风险:${handover.risk_level}"
            val riskColor = when (handover.risk_level.lowercase()) {
                "high", "critical" -> R.color.status_error_text
                "medium" -> R.color.status_warning_text
                else -> R.color.text_secondary
            }
            riskLabel.setTextColor(itemView.context.resources.getColor(riskColor))

            val pendingItems = handover.items.count { !it.acknowledged }
            val routeLabel = listOfNotNull(
                handover.from_operator_label,
                handover.to_operator_label,
            ).takeIf { it.size == 2 }?.joinToString(" → ")
            subtitle.text = listOfNotNull(
                routeLabel,
                "${handover.status}  ·  条目${handover.items.size}  ·  待签${pendingItems}",
                handover.summary?.takeIf { it.isNotBlank() },
            ).joinToString("\n")

            card.setOnClickListener {
                onHandoverSelected(handover)
            }
        }
    }

    companion object DiffCallback : DiffUtil.ItemCallback<ShiftHandover>() {
        override fun areItemsTheSame(oldItem: ShiftHandover, newItem: ShiftHandover): Boolean {
            return oldItem.handover_id == newItem.handover_id
        }

        override fun areContentsTheSame(oldItem: ShiftHandover, newItem: ShiftHandover): Boolean {
            return oldItem == newItem
        }
    }
}
