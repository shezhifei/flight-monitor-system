package com.flightmonitor.mobile.ui

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.ShiftHandoverItem
import com.google.android.material.button.MaterialButton
import com.google.android.material.card.MaterialCardView

class ShiftHandoverItemAdapter(
    private val onAcknowledgeClicked: (ShiftHandoverItem) -> Unit
) : ListAdapter<ShiftHandoverItem, ShiftHandoverItemAdapter.ViewHolder>(DiffCallback) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
        val view = LayoutInflater.from(parent.context).inflate(R.layout.item_shift_handover_detail, parent, false)
        return ViewHolder(view, onAcknowledgeClicked)
    }

    override fun onBindViewHolder(holder: ViewHolder, position: Int) {
        holder.bind(getItem(position))
    }

    class ViewHolder(
        itemView: View,
        private val onAcknowledgeClicked: (ShiftHandoverItem) -> Unit
    ) : RecyclerView.ViewHolder(itemView) {
        private val card: MaterialCardView = itemView.findViewById(R.id.handoverItemCard)
        private val title: TextView = itemView.findViewById(R.id.itemTitle)
        private val statusTag: TextView = itemView.findViewById(R.id.itemStatusTag)
        private val details: TextView = itemView.findViewById(R.id.itemDetails)
        private val ackButton: MaterialButton = itemView.findViewById(R.id.itemAckButton)

        fun bind(item: ShiftHandoverItem) {
            title.text = item.title

            statusTag.text = if (item.acknowledged) "已签收" else "待签收"
            statusTag.setTextColor(
                itemView.context.resources.getColor(
                    if (item.acknowledged) R.color.status_success_text else R.color.status_warning_text
                )
            )

            details.text = buildString {
                append(item.item_type)
                item.owner_user_id?.takeIf { it.isNotBlank() }?.let {
                    append("  ·  责任人:$it")
                }
            }

            if (item.acknowledged) {
                card.setCardBackgroundColor(itemView.context.resources.getColor(R.color.status_success_bg))
                ackButton.visibility = View.GONE
            } else {
                card.setCardBackgroundColor(itemView.context.resources.getColor(R.color.surface_card))
                ackButton.visibility = View.VISIBLE
                ackButton.setOnClickListener {
                    onAcknowledgeClicked(item)
                }
            }
        }
    }

    companion object DiffCallback : DiffUtil.ItemCallback<ShiftHandoverItem>() {
        override fun areItemsTheSame(oldItem: ShiftHandoverItem, newItem: ShiftHandoverItem): Boolean {
            return oldItem.item_id == newItem.item_id
        }

        override fun areContentsTheSame(oldItem: ShiftHandoverItem, newItem: ShiftHandoverItem): Boolean {
            return oldItem == newItem
        }
    }
}
