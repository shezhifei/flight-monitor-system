package com.flightmonitor.mobile.data

import com.flightmonitor.mobile.api.DispatchApi
import com.flightmonitor.mobile.api.model.DispatchActionOutcome
import com.flightmonitor.mobile.api.model.DispatchIssueReportRequest
import com.flightmonitor.mobile.api.model.DispatchOrderAcceptRequest
import com.flightmonitor.mobile.api.model.DispatchOrderCheckInRequest
import com.flightmonitor.mobile.api.model.DispatchOrderCheckOutRequest
import com.flightmonitor.mobile.api.model.DispatchOrderCompleteRequest
import com.flightmonitor.mobile.api.model.DispatchOrderEtaReportRequest
import com.flightmonitor.mobile.api.model.DispatchOrderItem
import com.flightmonitor.mobile.api.model.DispatchOrderStartRequest
import com.flightmonitor.mobile.api.model.DispatchSyncAction
import com.flightmonitor.mobile.api.model.DispatchSyncOutcome
import com.flightmonitor.mobile.api.model.DispatchSyncRequest
import com.flightmonitor.mobile.api.model.DispatchSyncResponse
import org.json.JSONArray
import org.json.JSONObject
import retrofit2.HttpException
import java.io.IOException
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import java.util.UUID

class DispatchRepository(
    private val dispatchApi: DispatchApi,
    private val offlineQueue: DispatchOfflineQueue,
) {
    suspend fun listMyOrders(status: String? = null): List<DispatchOrderItem> {
        val response = dispatchApi.listMyOrders(status = status)
        return response.data ?: emptyList()
    }

    suspend fun acceptOrder(orderId: String, note: String?): DispatchActionOutcome {
        return executeOrQueue(
            actionType = "accept",
            orderId = orderId,
            queuePayload = mapOf("note" to note),
            onlineCall = {
                dispatchApi.acceptOrder(
                    orderId = orderId,
                    payload = DispatchOrderAcceptRequest(
                        note = note,
                        client_action_id = null,
                    ),
                )
            },
        )
    }

    suspend fun checkInOrder(orderId: String, note: String?): DispatchActionOutcome {
        return executeOrQueue(
            actionType = "checkin",
            orderId = orderId,
            queuePayload = mapOf("note" to note),
            onlineCall = {
                dispatchApi.checkInOrder(
                    orderId = orderId,
                    payload = DispatchOrderCheckInRequest(
                        note = note,
                    ),
                )
            },
        )
    }

    suspend fun checkOutOrder(orderId: String, note: String?): DispatchActionOutcome {
        val clientRecordedAt = nowIsoUtc()
        return executeOrQueue(
            actionType = "checkout",
            orderId = orderId,
            queuePayload = mapOf("note" to note, "recorded_at" to clientRecordedAt),
            onlineCall = {
                dispatchApi.checkoutOrder(
                    orderId = orderId,
                    payload = DispatchOrderCheckOutRequest(
                        note = note,
                        recorded_at = clientRecordedAt,
                    ),
                )
            },
        )
    }

    suspend fun startOrder(orderId: String, notes: String?): DispatchActionOutcome {
        return executeOrQueue(
            actionType = "start",
            orderId = orderId,
            queuePayload = mapOf(
                "notes" to notes,
                "actual_start_time" to nowIsoUtc(),
            ),
            onlineCall = {
                dispatchApi.startOrder(
                    orderId = orderId,
                    payload = DispatchOrderStartRequest(
                        actual_start_time = nowIsoUtc(),
                        notes = notes,
                    ),
                )
            },
        )
    }

    suspend fun completeOrder(orderId: String, completionNotes: String?, actualEndTime: String? = null): DispatchActionOutcome {
        val resolvedActualEndTime = actualEndTime ?: nowIsoUtc()
        return executeOrQueue(
            actionType = "complete",
            orderId = orderId,
            queuePayload = mapOf(
                "completion_notes" to completionNotes,
                "actual_end_time" to resolvedActualEndTime,
            ),
            onlineCall = {
                dispatchApi.completeOrder(
                    orderId = orderId,
                    payload = DispatchOrderCompleteRequest(
                        actual_end_time = resolvedActualEndTime,
                        completion_notes = completionNotes,
                    ),
                )
            },
        )
    }

    suspend fun reportEstimatedCompletion(orderId: String, estimatedCompletionTime: String, note: String?): DispatchActionOutcome {
        return executeOrQueue(
            actionType = "eta_report",
            orderId = orderId,
            queuePayload = mapOf(
                "estimated_completion_time" to estimatedCompletionTime,
                "note" to note,
            ),
            onlineCall = {
                dispatchApi.reportEstimatedCompletion(
                    orderId = orderId,
                    payload = DispatchOrderEtaReportRequest(
                        estimated_completion_time = estimatedCompletionTime,
                        note = note,
                        client_action_id = null,
                    ),
                )
            },
        )
    }

    suspend fun reportIssue(
        orderId: String,
        title: String,
        description: String?,
        severity: String = "medium",
        attachments: List<String> = emptyList(),
    ): DispatchActionOutcome {
        val normalizedTitle = title.trim().ifEmpty { "现场异常上报" }
        return executeOrQueue(
            actionType = "report_issue",
            orderId = orderId,
            queuePayload = mapOf(
                "title" to normalizedTitle,
                "description" to description,
                "severity" to severity,
                "issue_type" to "dispatch_issue",
                "attachments" to attachments,
            ),
            onlineCall = {
                dispatchApi.reportIssue(
                    orderId = orderId,
                    payload = DispatchIssueReportRequest(
                        title = normalizedTitle,
                        description = description,
                        severity = severity,
                        issue_type = "dispatch_issue",
                        attachments = attachments,
                    ),
                )
            },
        )
    }

    suspend fun syncOfflineActions(): DispatchSyncOutcome {
        val actions = offlineQueue.all()
        if (actions.isEmpty()) {
            return DispatchSyncOutcome(
                total = 0,
                applied = 0,
                duplicates = 0,
                failed = 0,
                remainingQueueSize = 0,
            )
        }

        val response = dispatchApi.syncActions(
            payload = DispatchSyncRequest(actions = actions),
        ).data ?: DispatchSyncResponse(
            total = actions.size,
            applied = 0,
            duplicates = 0,
            failed = actions.size,
            results = emptyList(),
        )
        val removableIds = response.results
            .filter { it.status == "applied" || it.status == "duplicate" }
            .map { it.client_action_id }
            .toSet()
        offlineQueue.removeByClientActionIds(removableIds)

        return DispatchSyncOutcome(
            total = response.total,
            applied = response.applied,
            duplicates = response.duplicates,
            failed = response.failed,
            remainingQueueSize = offlineQueue.size(),
        )
    }

    fun pendingQueueSize(): Int = offlineQueue.size()

    private suspend fun executeOrQueue(
        actionType: String,
        orderId: String,
        queuePayload: Map<String, Any?>,
        onlineCall: suspend () -> Any?,
    ): DispatchActionOutcome {
        return try {
            onlineCall.invoke()
            DispatchActionOutcome(
                actionType = actionType,
                orderId = orderId,
                queued = false,
                message = "动作已提交",
            )
        } catch (error: IOException) {
            val queuedAction = DispatchSyncAction(
                client_action_id = createClientActionId(),
                action_type = actionType,
                dispatch_order_id = orderId,
                action_timestamp = nowIsoUtc(),
                payload = queuePayload,
            )
            offlineQueue.enqueue(queuedAction)
            DispatchActionOutcome(
                actionType = actionType,
                orderId = orderId,
                queued = true,
                message = "网络不可用，动作已加入离线队列",
            )
        } catch (error: HttpException) {
            DispatchActionOutcome(
                actionType = actionType,
                orderId = orderId,
                queued = false,
                message = parseHttpExceptionMessage(error),
            )
        } catch (error: Exception) {
            DispatchActionOutcome(
                actionType = actionType,
                orderId = orderId,
                queued = false,
                message = "执行失败：${error.message ?: error.javaClass.simpleName}",
            )
        }
    }

    private fun createClientActionId(): String {
        return UUID.randomUUID().toString().replace("-", "")
    }

    private fun parseHttpExceptionMessage(error: HttpException): String {
        val fallback = "服务端拒绝：HTTP ${error.code()}"
        val responseBody = runCatching {
            error.response()?.errorBody()?.string()
        }.getOrNull().orEmpty().trim()
        if (responseBody.isBlank()) {
            return fallback
        }

        val parsedMessage = runCatching { extractDetailMessage(responseBody) }.getOrNull()
        return parsedMessage?.takeIf { it.isNotBlank() }?.let { "服务端拒绝：$it" } ?: fallback
    }

    private fun extractDetailMessage(rawJson: String): String? {
        val root = JSONObject(rawJson)
        if (!root.has("detail")) {
            return root.optString("message").takeIf { it.isNotBlank() }
        }

        val detail = root.get("detail")
        return when (detail) {
            is String -> detail.ifBlank { null }
            is JSONObject -> {
                val message = detail.optString("message").ifBlank {
                    detail.optString("error").ifBlank { "" }
                }
                val pendingRequired = parseJsonArray(detail.optJSONArray("pending_required_items"))
                val failedRequired = parseJsonArray(detail.optJSONArray("failed_required_items"))
                buildString {
                    if (message.isNotBlank()) {
                        append(message)
                    }
                    if (pendingRequired.isNotEmpty()) {
                        if (isNotEmpty()) {
                            append("；")
                        }
                        append("待补门禁项: ")
                        append(pendingRequired.joinToString(separator = ","))
                    }
                    if (failedRequired.isNotEmpty()) {
                        if (isNotEmpty()) {
                            append("；")
                        }
                        append("失败门禁项: ")
                        append(failedRequired.joinToString(separator = ","))
                    }
                }.ifBlank { null }
            }
            is JSONArray -> parseJsonArray(detail).joinToString(separator = ",").ifBlank { null }
            else -> null
        }
    }

    private fun parseJsonArray(array: JSONArray?): List<String> {
        if (array == null || array.length() == 0) {
            return emptyList()
        }
        return buildList {
            for (index in 0 until array.length()) {
                val value = array.opt(index)?.toString()?.trim().orEmpty()
                if (value.isNotBlank()) {
                    add(value)
                }
            }
        }
    }

    private fun nowIsoUtc(): String {
        val formatter = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US)
        formatter.timeZone = TimeZone.getTimeZone("UTC")
        return formatter.format(Date())
    }
}
