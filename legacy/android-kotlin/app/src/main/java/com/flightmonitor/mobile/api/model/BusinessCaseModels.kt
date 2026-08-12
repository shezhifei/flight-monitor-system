package com.flightmonitor.mobile.api.model

data class BusinessCaseEnvelope(
    val success: Boolean = true,
    val data: BusinessCase? = null,
    val message: String? = null,
)

data class BusinessCase(
    val case_id: String,
    val case_type: String,
    val flight_id: String,
    val flight_no: String,
    val created_at: String,
    val created_by: String,
    val updated_by: String,
    val description: String,
    val status: String,
    val stand: String? = null,
    val gate: String? = null,
    val visibility_scope: String = "COMMON",
    val department_id: String? = null,
    val department_name_snapshot: String? = null,
    val finished_at: String? = null,
    val cancelled_at: String? = null,
    val log: List<Map<String, Any?>> = emptyList(),
    val context: Map<String, Any?> = emptyMap(),
    val terminal_metadata: BusinessCaseTerminalMetadata? = null,
    val append_count: Int = 0,
    val latest_append: BusinessCaseAppendEntry? = null,
    val append_entries: List<BusinessCaseAppendEntry> = emptyList(),
)

data class BusinessCaseTerminalMetadata(
    val timestamp: String,
    val operator: String,
    val action: String,
    val target_status: String,
    val reason: String? = null,
    val workflow_run_id: String? = null,
    val workflow_outcome: String? = null,
    val receipt_group_id: String? = null,
)

data class BusinessCaseAppendEntry(
    val append_id: String,
    val case_id: String,
    val content: String,
    val submitted_by: String,
    val submitted_operator_name: String? = null,
    val appended_at: String,
    val metadata: Map<String, Any?> = emptyMap(),
)

data class BusinessCaseAppendAcknowledgement(
    val acknowledged: Boolean,
    val acknowledged_at: String? = null,
    val append_id: String,
    val user_id: String,
)

data class BusinessCaseType(
    val id: String,
    val code: String,
    val name: String,
    val bpmn_xml: String? = null,
    val description: String? = null,
    val is_active: Boolean = true,
    val visibility_scope: String = "COMMON",
    val department_id: String? = null,
    val department_name_snapshot: String? = null,
    val created_at: String? = null,
    val updated_at: String? = null,
)

data class BusinessCaseCreateRequest(
    val case_type: String,
    val flight_id: String,
    val description: String,
    val visibility_scope: String = "DEPARTMENT",
    val status: String? = null,
    val context: Map<String, Any?> = emptyMap(),
)

data class BusinessCaseAppendRequest(
    val content: String,
    val mention_user_ids: List<String> = emptyList(),
)

data class BusinessCaseStatusUpdateRequest(
    val status: String,
)

data class BusinessCaseWorkflowStartRequest(
    val flight_id: String,
    val description: String,
    val extra_info: Map<String, Any?> = emptyMap(),
)

data class BusinessCaseWorkflowStartData(
    val run: BusinessCaseWorkflowRun,
    val business_case: BusinessCase,
    val receipt_group_id: String? = null,
    val recipient_snapshot: List<Map<String, Any?>> = emptyList(),
    val process_instance_id: String,
    val workflow_triggered: Boolean = false,
)

data class BusinessCaseWorkflowRunDetail(
    val run: BusinessCaseWorkflowRun,
    val business_case: BusinessCase,
    val process_instance: Map<String, Any?>? = null,
    val active_tasks: List<Map<String, Any?>> = emptyList(),
    val historic_tasks: List<Map<String, Any?>> = emptyList(),
    val receipt_group: Map<String, Any?>? = null,
)

data class BusinessCaseWorkflowRun(
    val run_id: String,
    val template_code: String,
    val case_id: String,
    val flight_id: String,
    val process_definition_key: String,
    val process_instance_id: String,
    val waiting_task_id: String? = null,
    val receipt_group_id: String? = null,
    val status: String,
    val outcome: String? = null,
    val recipient_snapshot: List<Map<String, Any?>> = emptyList(),
    val flight_context_snapshot: Map<String, Any?> = emptyMap(),
    val start_payload: Map<String, Any?> = emptyMap(),
    val started_by: String,
    val completed_at: String? = null,
    val failed_reason: String? = null,
    val created_at: String,
    val updated_at: String,
)

fun businessCaseStatusLabel(status: String?): String {
    return when (status?.trim()?.uppercase()) {
        "INITIAL" -> "初始化"
        "PENDING" -> "待处理"
        "PROCESSING" -> "处理中"
        "SUCCESS" -> "已完成"
        "FAILED" -> "失败"
        else -> status ?: "未知"
    }
}

fun businessCaseVisibilityLabel(scope: String?, departmentName: String?): String {
    return when (scope?.trim()?.uppercase()) {
        "DEPARTMENT" -> departmentName?.takeIf { it.isNotBlank() }?.let { "部门事项 · $it" } ?: "部门事项"
        "COMMON" -> "通用事项"
        else -> scope ?: "未知范围"
    }
}

@Suppress("UNCHECKED_CAST")
fun BusinessCaseAppendEntry.mentionUserIds(): List<String> {
    val values = metadata["mention_user_ids"] as? List<*> ?: return emptyList()
    return values.mapNotNull { it as? String }
}

@Suppress("UNCHECKED_CAST")
fun BusinessCaseAppendEntry.acknowledgmentMap(): Map<String, Map<String, Any?>> {
    val values = metadata["acknowledgments"] as? Map<*, *> ?: return emptyMap()
    return values.entries.mapNotNull { (key, value) ->
        val userId = key as? String ?: return@mapNotNull null
        val detail = value as? Map<*, *> ?: return@mapNotNull null
        userId to detail.entries.mapNotNull { (detailKey, detailValue) ->
            val typedKey = detailKey as? String ?: return@mapNotNull null
            typedKey to detailValue
        }.toMap()
    }.toMap()
}
