package com.flightmonitor.mobile.data

import android.content.Context
import android.content.SharedPreferences
import com.flightmonitor.mobile.api.model.DispatchSyncAction
import com.google.gson.Gson
import com.google.gson.reflect.TypeToken

class DispatchOfflineQueue(
    context: Context,
) {
    private val preferences: SharedPreferences =
        context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    private val gson = Gson()
    private val listType = object : TypeToken<List<DispatchSyncAction>>() {}.type

    @Synchronized
    fun enqueue(action: DispatchSyncAction) {
        val actions = readActions().toMutableList()
        actions.add(action)
        saveActions(actions)
    }

    @Synchronized
    fun all(): List<DispatchSyncAction> = readActions()

    @Synchronized
    fun removeByClientActionIds(ids: Set<String>) {
        if (ids.isEmpty()) {
            return
        }
        val filtered = readActions()
            .filterNot { ids.contains(it.client_action_id) }
        saveActions(filtered)
    }

    @Synchronized
    fun clear() {
        preferences.edit().remove(KEY_ACTIONS).apply()
    }

    @Synchronized
    fun size(): Int = readActions().size

    private fun readActions(): List<DispatchSyncAction> {
        val raw = preferences.getString(KEY_ACTIONS, null) ?: return emptyList()
        return runCatching {
            gson.fromJson<List<DispatchSyncAction>>(raw, listType)
        }.getOrNull()?.filterNotNull() ?: emptyList()
    }

    private fun saveActions(actions: List<DispatchSyncAction>) {
        val encoded = gson.toJson(actions)
        preferences.edit().putString(KEY_ACTIONS, encoded).apply()
    }

    private companion object {
        private const val PREFS_NAME = "dispatch_offline_queue"
        private const val KEY_ACTIONS = "pending_actions"
    }
}
