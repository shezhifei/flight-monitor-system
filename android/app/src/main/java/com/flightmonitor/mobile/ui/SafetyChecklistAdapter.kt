package com.flightmonitor.mobile.ui

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.DispatchSafetyChecklistItemStatus
import com.google.android.material.button.MaterialButton
import com.google.android.material.button.MaterialButtonToggleGroup
import com.google.android.material.card.MaterialCardView

class SafetyChecklistAdapter(
    private val onItemResultSelected: (String, String) -> Unit
) : ListAdapter<DispatchSafetyChecklistItemStatus, SafetyChecklistAdapter.ViewHolder>(DiffCallback) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
        val view = LayoutInflater.from(parent.context).inflate(R.layout.item_safety_checklist, parent, false)
        return ViewHolder(view, onItemResultSelected)
    }

    override fun onBindViewHolder(holder: ViewHolder, position: Int) {
        holder.bind(getItem(position))
    }

    class ViewHolder(
        itemView: View,
        private val onResultSelected: (String, String) -> Unit
    ) : RecyclerView.ViewHolder(itemView) {

        private val itemCard: MaterialCardView = itemView.findViewById(R.id.safetyItemCard)
        private val statusIcon: TextView = itemView.findViewById(R.id.itemStatusIcon)
        private val title: TextView = itemView.findViewById(R.id.itemTitle)
        private val requiredTag: TextView = itemView.findViewById(R.id.itemRequiredTag)
        private val stateHint: TextView = itemView.findViewById(R.id.itemStateHint)
        private val actionGroup: MaterialButtonToggleGroup = itemView.findViewById(R.id.itemActionGroup)
        private val actionNa: MaterialButton = itemView.findViewById(R.id.actionNa)

        fun bind(item: DispatchSafetyChecklistItemStatus) {
            title.text = item.title

            requiredTag.text = if (item.required) "必填" else "可选"
            requiredTag.setTextColor(
                itemView.context.resources.getColor(
                    if (item.required) R.color.status_warning_text else R.color.text_secondary
                )
            )

            // 状态渲染
            when (item.result?.lowercase()) {
                "pass" -> {
                    itemCard.setCardBackgroundColor(itemView.context.resources.getColor(R.color.status_success_bg))
                    statusIcon.text = "✅"
                    stateHint.text = "结果: ✓ 通过  ·  ${item.item_code}"
                }
                "fail" -> {
                    itemCard.setCardBackgroundColor(itemView.context.resources.getColor(R.color.status_error_bg))
                    statusIcon.text = "❌"
                    stateHint.text = "结果: ✗ 不通过  ·  ${item.item_code}"
                }
                "na" -> {
                    itemCard.setCardBackgroundColor(itemView.context.resources.getColor(R.color.surface_muted))
                    statusIcon.text = "➖"
                    stateHint.text = "结果: 不适用  ·  ${item.item_code}"
                }
                else -> {
                    itemCard.setCardBackgroundColor(itemView.context.resources.getColor(R.color.surface_card))
                    statusIcon.text = "⏳"
                    stateHint.text = "待检查  ·  ${item.item_code}"
                }
            }

            // 操作组
            if (item.status == "pending") {
                actionGroup.visibility = View.VISIBLE
                actionNa.visibility = if (item.allow_na) View.VISIBLE else View.GONE
                
                // Clear state without triggering listeners
                actionGroup.clearOnButtonCheckedListeners()
                actionGroup.clearChecked()

                actionGroup.addOnButtonCheckedListener { group, checkedId, isChecked ->
                    if (isChecked) {
                        val result = when (checkedId) {
                            R.id.actionPass -> "pass"
                            R.id.actionFail -> "fail"
                            R.id.actionNa -> "na"
                            else -> return@addOnButtonCheckedListener
                        }
                        onResultSelected(item.item_code, result)
                    }
                }
            } else {
                actionGroup.visibility = View.GONE
            }
        }
    }

    companion object DiffCallback : DiffUtil.ItemCallback<DispatchSafetyChecklistItemStatus>() {
        override fun areItemsTheSame(
            oldItem: DispatchSafetyChecklistItemStatus,
            newItem: DispatchSafetyChecklistItemStatus
        ): Boolean {
            return oldItem.item_code == newItem.item_code
        }

        override fun areContentsTheSame(
            oldItem: DispatchSafetyChecklistItemStatus,
            newItem: DispatchSafetyChecklistItemStatus
        ): Boolean {
            return oldItem == newItem
        }
    }
}
