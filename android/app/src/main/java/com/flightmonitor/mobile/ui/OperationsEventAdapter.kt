package com.flightmonitor.mobile.ui

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.flightmonitor.mobile.R
import com.flightmonitor.mobile.api.model.MobileOperationsEventItem

class OperationsEventAdapter : ListAdapter<MobileOperationsEventItem, OperationsEventAdapter.EventViewHolder>(DiffCallback) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): EventViewHolder {
        val view = LayoutInflater.from(parent.context).inflate(R.layout.item_operation_event, parent, false)
        return EventViewHolder(view)
    }

    override fun onBindViewHolder(holder: EventViewHolder, position: Int) {
        holder.bind(getItem(position))
    }

    class EventViewHolder(itemView: View) : RecyclerView.ViewHolder(itemView) {
        private val severityIndicator: View = itemView.findViewById(R.id.severityIndicator)
        private val eventTitle: TextView = itemView.findViewById(R.id.eventTitle)
        private val eventStatus: TextView = itemView.findViewById(R.id.eventStatus)
        private val eventMessage: TextView = itemView.findViewById(R.id.eventMessage)
        private val eventTime: TextView = itemView.findViewById(R.id.eventTime)

        fun bind(event: MobileOperationsEventItem) {
            eventTitle.text = event.title
            eventStatus.text = event.severity.uppercase()
            
            val severityColor = when (event.severity.lowercase()) {
                "critical", "fatal", "high" -> R.color.status_error_text
                "warning", "medium" -> R.color.status_warning_text
                "info", "low" -> R.color.status_info_text
                else -> R.color.text_tertiary
            }
            severityIndicator.setBackgroundColor(itemView.context.resources.getColor(severityColor))
            eventStatus.setTextColor(itemView.context.resources.getColor(severityColor))

            eventMessage.text = "${event.event_type} - ${event.status}" + if(event.flight_id?.isNotBlank() == true) " ${event.flight_id}" else ""
            eventTime.text = event.occurred_at
        }
    }

    companion object DiffCallback : DiffUtil.ItemCallback<MobileOperationsEventItem>() {
        override fun areItemsTheSame(oldItem: MobileOperationsEventItem, newItem: MobileOperationsEventItem): Boolean {
            return oldItem.event_id == newItem.event_id
        }

        override fun areContentsTheSame(oldItem: MobileOperationsEventItem, newItem: MobileOperationsEventItem): Boolean {
            return oldItem == newItem
        }
    }
}
