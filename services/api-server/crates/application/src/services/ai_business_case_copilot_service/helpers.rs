use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::ai_copilot::AiCopilotBusinessCaseBatch;
use fms_domain::models::business_case::FlightBusinessCase;
use fms_domain::models::flight::Flight;
use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ports::ai_copilot_repository::AiCopilotBusinessCaseBatchRepository;

use crate::services::business_case_service::BusinessCaseServiceOps;
use crate::services::business_case_workflow_service::{BusinessCaseWorkflowBatchItem, WorkflowActor};

use super::access::AiCopilotBatchAccess;
use super::config::{
    AiFlightMatchingConfig, AiLegBindingConfig, BusinessCaseProperties, CaseDuplicatePolicy,
    CopilotCaseTypeCatalogEntry, PreparedCommitAction,
};
use super::schemas::{
    AiCopilotApprovedAction, AiCopilotBatchStatusResponse, AiCopilotMatchedFlight, AiCopilotNotificationGroup,
    LlmDraftAction, LlmDraftPayload, StoredWorkflowDispatchRequest,
};

#[derive(Debug, Clone, Serialize)]
pub(super) struct CaseTypePromptView {
    code: String,
    name: String,
    description: Option<String>,
    aliases: Vec<String>,
    trigger_phrases: Vec<String>,
    leg_binding: serde_json::Value,
    fields: serde_json::Value,
    forbidden_fields: Vec<String>,
    description_template: Option<String>,
    remarks_template: Option<String>,
    examples: Vec<serde_json::Value>,
    confidence_threshold: Option<f64>,
    extensions: serde_json::Value,
}

pub(super) fn build_extraction_prompt(transcript: &str, catalog: &[CopilotCaseTypeCatalogEntry]) -> String {
    let views = catalog
        .iter()
        .map(|entry| CaseTypePromptView {
            code: entry.code.clone(),
            name: entry.name.clone(),
            description: entry.description.clone(),
            aliases: entry.config.aliases.clone(),
            trigger_phrases: entry.config.trigger_phrases.clone(),
            leg_binding: serde_json::to_value(&entry.config.leg_binding).unwrap_or(serde_json::Value::Null),
            fields: serde_json::to_value(&entry.config.fields).unwrap_or(serde_json::Value::Null),
            forbidden_fields: entry.config.forbidden_fields.clone(),
            description_template: entry.config.description_template.clone(),
            remarks_template: entry.config.remarks_template.clone(),
            examples: entry.config.examples.clone(),
            confidence_threshold: entry.config.confidence_threshold,
            extensions: serde_json::to_value(&entry.config.extensions).unwrap_or(serde_json::Value::Null),
        })
        .collect::<Vec<_>>();

    let catalog_json =
        fms_infrastructure::observability::serialize_json_pretty(&views).unwrap_or_else(|_| "[]".to_string());

    format!(
        r#"你是航班监控系统的语音业务事项抽取器。只输出 JSON，不要输出解释。
只能从 candidate_case_types 中选择 case_type；不要输出未列出的事项类型。
若信息缺失或无法确定，仍输出候选动作，但设置 needs_review=true 并说明 review_reason。
不得编造航班号、登机口、机位、旅客姓名或未出现在语音中的字段。

输出 JSON schema:
{{"summary":"对话摘要","actions":[{{"case_type":"业务事项编码","case_type_name":"事项名称","flight_number_raw":"航班号原文","leg_type_hint":"outbound|inbound|unknown","description":"事项描述","remarks":"备注","fields":{{}},"confidence":0.0,"needs_review":false,"review_reason":null}}]}}

candidate_case_types:
{}

Transcript:
{}"#,
        catalog_json, transcript
    )
}

pub(super) fn retrieve_candidate_case_types(
    transcript: &str,
    catalog: &[CopilotCaseTypeCatalogEntry],
    limit: usize,
) -> Vec<CopilotCaseTypeCatalogEntry> {
    let normalized = transcript.to_ascii_lowercase();
    let mut scored = catalog
        .iter()
        .map(|entry| {
            let mut score = 0;
            for value in entry.config.aliases.iter().chain(entry.config.trigger_phrases.iter()) {
                let value = value.trim();
                if !value.is_empty() && normalized.contains(&value.to_ascii_lowercase()) {
                    score += 3;
                }
            }
            if normalized.contains(&entry.name.to_ascii_lowercase()) {
                score += 2;
            }
            if normalized.contains(&entry.code.to_ascii_lowercase()) {
                score += 1;
            }
            (score, entry.clone())
        })
        .collect::<Vec<_>>();

    use std::cmp::Ordering;
    scored.sort_by(|a, b| match b.0.cmp(&a.0) {
        Ordering::Equal => a.1.code.cmp(&b.1.code),
        other => other,
    });

    let mut result = scored
        .into_iter()
        .filter(|(score, _)| *score > 0)
        .map(|(_, entry)| entry)
        .collect::<Vec<_>>();

    if result.is_empty() {
        result = catalog.iter().take(limit).cloned().collect();
    } else {
        result.truncate(limit);
    }
    result
}

pub(super) fn parse_llm_payload(raw: &str) -> Result<LlmDraftPayload, DomainError> {
    let json_text = extract_json_text(raw)
        .ok_or_else(|| DomainError::ValidationError("AI response did not contain JSON".into()))?;
    serde_json::from_str::<LlmDraftPayload>(&json_text)
        .map_err(|error| DomainError::ValidationError(format!("AI JSON 解析失败: {error}")))
}

pub(super) fn extract_json_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        return Some(trimmed.to_string());
    }
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let after = after.strip_prefix("json").unwrap_or(after).trim_start();
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim().to_string());
        }
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    (end > start).then(|| trimmed[start..=end].to_string())
}

pub(super) fn summarize_transcript(transcript: &str) -> String {
    let mut text = transcript.trim().chars().take(120).collect::<String>();
    if transcript.chars().count() > 120 {
        text.push_str("...");
    }
    text
}

pub(super) struct AiCopilotDraftActionValidationResult {
    pub(super) action: LlmDraftAction,
    pub(super) needs_review: bool,
    pub(super) review_reason: Option<String>,
}

pub(super) fn validate_and_enrich_action(
    mut action: LlmDraftAction,
    idx: usize,
    catalog_by_code: &HashMap<&str, &CopilotCaseTypeCatalogEntry>,
) -> AiCopilotDraftActionValidationResult {
    let mut needs_review = false;
    let mut review_reasons = Vec::new();

    let case_type_trimmed = action.case_type.trim();
    let entry_opt = catalog_by_code.get(case_type_trimmed);

    if case_type_trimmed.is_empty() {
        needs_review = true;
        review_reasons.push("未识别到事项类型".to_string());
    } else if entry_opt.is_none() {
        needs_review = true;
        review_reasons.push("事项类型不在当前 AI 配置候选中".to_string());
    }

    action.case_type = case_type_trimmed.to_string();
    action.flight_number_raw = action.flight_number_raw.trim().to_uppercase();

    let leg_hint = action.leg_type_hint.as_deref().map(str::trim).unwrap_or("unknown");

    let mut resolved_leg = leg_hint.to_string();

    if let Some(entry) = entry_opt {
        action.case_type_name = Some(entry.name.clone());
        let leg_config = &entry.config.leg_binding;
        let case_props = &entry.case_properties;

        // Prefer case_properties binding_policy for leg resolution
        let effective_default_leg = case_props
            .binding_policy
            .default_leg_type
            .as_deref()
            .or(leg_config.default.as_deref());
        let effective_allowed_legs = if case_props.binding_policy.allowed_leg_types.is_empty() {
            &leg_config.allowed
        } else {
            &case_props.binding_policy.allowed_leg_types
        };

        if (leg_hint == "unknown" || leg_hint.is_empty()) && effective_default_leg.is_some() {
            resolved_leg = effective_default_leg.as_deref().unwrap().to_string();
        }

        if !effective_allowed_legs.is_empty() && !effective_allowed_legs.contains(&resolved_leg) {
            needs_review = true;
            review_reasons.push(format!("航段类型 {} 不在配置的允许列表中", resolved_leg));
        }

        let fields_map = action.fields.as_object_mut();
        if let Some(fields_obj) = fields_map {
            for forbidden in &entry.config.forbidden_fields {
                if fields_obj.remove(forbidden).is_some() {
                    needs_review = true;
                    review_reasons.push(format!("包含了被禁止的字段: {}", forbidden));
                }
            }

            // Check required fields from case_properties extra_info_schema
            for (field_name, field_schema) in &case_props.extra_info_schema.fields {
                if field_schema.required {
                    let has_val = fields_obj
                        .get(field_name)
                        .map(|v| match v {
                            Value::Null => false,
                            Value::String(s) => !s.trim().is_empty(),
                            _ => true,
                        })
                        .unwrap_or(false);
                    if !has_val {
                        needs_review = true;
                        review_reasons.push(format!(
                            "缺少必需字段: {}",
                            field_schema.label.as_deref().unwrap_or(field_name)
                        ));
                    }
                }
            }

            // Check required fields from AI extraction config (backward compat)
            for (field_name, field_cfg) in &entry.config.fields {
                if field_cfg.required {
                    let has_val = fields_obj
                        .get(field_name)
                        .map(|v| match v {
                            Value::Null => false,
                            Value::String(s) => !s.trim().is_empty(),
                            _ => true,
                        })
                        .unwrap_or(false);
                    if !has_val {
                        needs_review = true;
                        review_reasons.push(format!(
                            "缺少必需字段: {}",
                            field_cfg.label.as_deref().unwrap_or(field_name)
                        ));
                    }
                }
            }
        } else {
            action.fields = serde_json::json!({});
            for (field_name, field_schema) in &case_props.extra_info_schema.fields {
                if field_schema.required {
                    needs_review = true;
                    review_reasons.push(format!(
                        "缺少必需字段: {}",
                        field_schema.label.as_deref().unwrap_or(field_name)
                    ));
                }
            }
            for (field_name, field_cfg) in &entry.config.fields {
                if field_cfg.required {
                    needs_review = true;
                    review_reasons.push(format!(
                        "缺少必需字段: {}",
                        field_cfg.label.as_deref().unwrap_or(field_name)
                    ));
                }
            }
        }

        // Render description/remarks from templates
        if action.description.trim().is_empty() {
            if let Some(ref desc_tpl) = entry.config.description_template {
                action.description = render_action_template(desc_tpl, &action.fields);
            }
        }
        if action.remarks.trim().is_empty() {
            // Prefer case_properties summary_template, then AI remarks_template
            if let Some(ref summary_tpl) = case_props.extra_info_schema.summary_template {
                action.remarks = render_action_template(summary_tpl, &action.fields);
            } else if let Some(ref remarks_tpl) = entry.config.remarks_template {
                action.remarks = render_action_template(remarks_tpl, &action.fields);
            }
        }
    }

    action.leg_type_hint = Some(resolved_leg);

    if action.description.trim().is_empty() {
        action.description = if action.remarks.trim().is_empty() {
            format!("AI Copilot 识别事项 {}", idx + 1)
        } else {
            action.remarks.trim().to_string()
        };
    }

    let review_reason = if review_reasons.is_empty() {
        None
    } else {
        Some(review_reasons.join("; "))
    };

    AiCopilotDraftActionValidationResult {
        action,
        needs_review,
        review_reason,
    }
}

pub(super) fn merge_copilot_flight_binding(
    ai_cfg: &AiFlightMatchingConfig,
    ai_leg: &AiLegBindingConfig,
    case_props: &BusinessCaseProperties,
) -> (AiFlightMatchingConfig, AiLegBindingConfig) {
    let bp = &case_props.binding_policy;
    let fp = &bp.flight_match_policy;

    let matching = AiFlightMatchingConfig {
        allow_numeric_suffix: fp.allow_numeric_suffix.or(ai_cfg.allow_numeric_suffix),
        prefer_leg: ai_cfg.prefer_leg.clone(),
        exclude_cancelled: fp.exclude_cancelled.or(ai_cfg.exclude_cancelled),
        exclude_departed: fp.exclude_departed.or(ai_cfg.exclude_departed),
        exclude_actual_departure: fp.exclude_actual_departure.or(ai_cfg.exclude_actual_departure),
        window_hours_before: fp.time_window_hours_before.or(ai_cfg.window_hours_before),
        window_hours_after: fp.time_window_hours_after.or(ai_cfg.window_hours_after),
        min_auto_match_score: fp.min_auto_match_score.or(ai_cfg.min_auto_match_score),
    };

    let leg_binding = AiLegBindingConfig {
        allowed: if bp.allowed_leg_types.is_empty() {
            ai_leg.allowed.clone()
        } else {
            bp.allowed_leg_types.clone()
        },
        default: bp.default_leg_type.clone().or_else(|| ai_leg.default.clone()),
        required: bp.leg_type_required || ai_leg.required,
    };

    (matching, leg_binding)
}

pub(super) fn render_action_template(template: &str, fields: &serde_json::Value) -> String {
    let mut rendered = template.to_string();
    if let Some(obj) = fields.as_object() {
        for (k, v) in obj {
            let placeholder = format!("{{{{{}}}}}", k);
            let val_str = match v {
                Value::Null => "".to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => v.to_string(),
            };
            rendered = rendered.replace(&placeholder, &val_str);
        }
    }
    rendered
}

pub(super) fn match_flight(
    flight: &Flight,
    raw: &str,
    config: &AiFlightMatchingConfig,
    leg_binding: &AiLegBindingConfig,
) -> Option<AiCopilotMatchedFlight> {
    if config.exclude_cancelled.unwrap_or(true) && matches!(flight.status, FlightStatus::Cancelled) {
        return None;
    }
    if config.exclude_departed.unwrap_or(true) && matches!(flight.status, FlightStatus::Departed) {
        return None;
    }
    if config.exclude_actual_departure.unwrap_or(true) && flight.actual_departure.is_some() {
        return None;
    }

    let mut legs_to_try = Vec::new();
    if leg_binding.allowed.is_empty() {
        legs_to_try.push("outbound");
        legs_to_try.push("inbound");
    } else {
        for leg in &leg_binding.allowed {
            if leg == "outbound" || leg == "inbound" {
                legs_to_try.push(leg.as_str());
            }
        }
    }

    let mut best_candidate: Option<AiCopilotMatchedFlight> = None;

    for leg_type in legs_to_try {
        let (leg_opt, reference_time) = if leg_type == "outbound" {
            (
                &flight.outbound_leg,
                flight.estimated_departure.or(flight.scheduled_departure),
            )
        } else {
            (
                &flight.inbound_leg,
                flight.estimated_arrival.or(flight.scheduled_arrival),
            )
        };

        let Some(leg) = leg_opt else {
            continue;
        };

        let leg_flight_no = leg.flight_no.trim().to_uppercase();
        let exact = leg_flight_no == raw;
        let suffix = config.allow_numeric_suffix.unwrap_or(true)
            && raw.chars().all(|ch| ch.is_ascii_digit())
            && leg_flight_no.ends_with(raw);
        if !exact && !suffix {
            continue;
        }

        if let Some(ts) = reference_time {
            let now = Utc::now();
            let hours_diff = (ts - now).num_minutes() as f64 / 60.0;
            let before_limit = config.window_hours_before.unwrap_or(3) as f64;
            let after_limit = config.window_hours_after.unwrap_or(8) as f64;
            if hours_diff > before_limit || hours_diff < -after_limit {
                continue;
            }
        }

        let time_score = reference_time
            .map(|ts| {
                let diff_minutes = (ts - Utc::now()).num_minutes().unsigned_abs() as f64;
                (1.0 - (diff_minutes / (12.0 * 60.0))).clamp(0.0, 1.0)
            })
            .unwrap_or(0.25);

        let mut score = if exact { 1.0 } else { 0.82 } + time_score * 0.15;

        if let Some(ref prefer) = config.prefer_leg {
            if prefer == leg_type {
                score += 0.05;
            }
        }

        let candidate = AiCopilotMatchedFlight {
            flight_id: flight.flight_id.to_string(),
            flight_no: leg_flight_no,
            leg_type: leg_type.to_string(),
            score,
            scheduled_departure: flight.scheduled_departure,
            estimated_departure: flight.estimated_departure,
            status: Some(flight.status.label().to_string()),
        };

        if best_candidate
            .as_ref()
            .map_or(true, |best| candidate.score > best.score)
        {
            best_candidate = Some(candidate);
        }
    }

    best_candidate
}

pub(super) fn resolve_match_review(
    candidates: &[AiCopilotMatchedFlight],
    min_auto_match_score: f64,
) -> (Option<AiCopilotMatchedFlight>, bool, Option<String>) {
    let Some(best) = candidates.first() else {
        return (None, true, Some("未找到匹配的未起飞出港航班".to_string()));
    };
    if best.score < min_auto_match_score {
        return (
            Some(best.clone()),
            true,
            Some(format!("匹配置信度较弱 ({:.2} < {})", best.score, min_auto_match_score)),
        );
    }
    if candidates.len() > 1 && (best.score - candidates[1].score).abs() < 0.08 {
        return (
            Some(best.clone()),
            true,
            Some("存在多个相近候选航班，需要人工确认".to_string()),
        );
    }
    (Some(best.clone()), false, None)
}

pub(super) fn validate_approved_action(action: &AiCopilotApprovedAction) -> Result<(), DomainError> {
    if action.action_id.trim().is_empty() {
        return Err(DomainError::ValidationError("action_id is required".into()));
    }
    if action.case_type.trim().is_empty() {
        return Err(DomainError::ValidationError("case_type is required".into()));
    }
    if action.flight_id.trim().is_empty() {
        return Err(DomainError::ValidationError("flight_id is required".into()));
    }
    if action.flight_no.trim().is_empty() {
        return Err(DomainError::ValidationError("flight_no is required".into()));
    }
    Ok(())
}

pub(super) fn batch_to_status_response(batch: AiCopilotBusinessCaseBatch) -> AiCopilotBatchStatusResponse {
    AiCopilotBatchStatusResponse {
        batch_id: batch.batch_id,
        entity_id: batch.entity_id,
        source_page: batch.source_page,
        transcript_summary: batch.transcript_summary,
        draft_actions: batch.draft_actions,
        status: batch.status,
        created_by: batch.created_by,
        committed_case_ids: batch.committed_case_ids,
        notification_groups: batch.notification_groups,
        commit_error: batch.commit_error,
        committed_at: batch.committed_at,
        workflow_dispatch_status: batch.workflow_dispatch_status,
        workflow_dispatch_error: batch.workflow_dispatch_error,
        workflow_dispatch_attempts: batch.workflow_dispatch_attempts,
        workflow_dispatch_next_retry_at: batch.workflow_dispatch_next_retry_at,
        workflow_dispatched_at: batch.workflow_dispatched_at,
        created_at: batch.created_at,
        updated_at: batch.updated_at,
        expires_at: batch.expires_at,
    }
}

pub(super) fn append_unique_case_ids<I>(target: &mut Vec<String>, case_ids: I)
where
    I: IntoIterator<Item = String>,
{
    let mut seen = target.iter().cloned().collect::<HashSet<_>>();
    for case_id in case_ids {
        let case_id = case_id.trim();
        if !case_id.is_empty() && seen.insert(case_id.to_string()) {
            target.push(case_id.to_string());
        }
    }
}

pub(super) fn is_terminal_commit_recovery_error(error: &DomainError) -> bool {
    matches!(error, DomainError::ValidationError(_) | DomainError::NotFound { .. })
}

pub(super) fn batch_not_found(batch_id: &str) -> DomainError {
    DomainError::NotFound {
        entity_type: "ai_copilot_business_case_batch",
        id: batch_id.to_string(),
    }
}

pub(super) fn ensure_batch_visible(
    batch: &AiCopilotBusinessCaseBatch,
    access: &AiCopilotBatchAccess,
) -> Result<(), DomainError> {
    if access.can_access(batch) {
        Ok(())
    } else {
        Err(batch_not_found(&batch.batch_id))
    }
}

pub(super) fn ensure_batch_ops_access(
    batch: &AiCopilotBusinessCaseBatch,
    access: &AiCopilotBatchAccess,
) -> Result<(), DomainError> {
    if access.can_access_all() {
        Ok(())
    } else {
        Err(batch_not_found(&batch.batch_id))
    }
}

pub(super) fn notification_groups_from_value(value: &Value) -> Vec<AiCopilotNotificationGroup> {
    serde_json::from_value::<Vec<AiCopilotNotificationGroup>>(value.clone()).unwrap_or_default()
}

pub(super) fn build_commit_error_payload(stage: &str, error: &DomainError, cleanup_succeeded: bool) -> Value {
    json!({
        "stage": stage,
        "message": error.to_string(),
        "cleanup_succeeded": cleanup_succeeded,
        "recorded_at": Utc::now(),
    })
}

pub(super) fn build_workflow_dispatch_request(
    items: &[BusinessCaseWorkflowBatchItem],
    actor: &WorkflowActor,
    case_ids: &[String],
) -> Value {
    json!({
        "items": items.iter().map(|item| {
            json!({
                "template_code": item.template_code,
                "case_id": item.case_id,
            })
        }).collect::<Vec<_>>(),
        "case_ids": case_ids,
        "actor": {
            "actor": actor.actor,
            "user_id": actor.user_id,
            "username": actor.username,
            "name_snapshot": actor.name_snapshot,
            "context_type": actor.context_type,
            "context_id": actor.context_id,
        },
        "created_at": Utc::now(),
    })
}

pub(super) fn workflow_items_from_dispatch_request(
    value: &Value,
) -> Result<Vec<BusinessCaseWorkflowBatchItem>, DomainError> {
    let request = serde_json::from_value::<StoredWorkflowDispatchRequest>(value.clone())
        .map_err(|error| DomainError::ValidationError(format!("流程派发请求快照格式无效: {error}")))?;
    let mut items = Vec::new();
    for item in request.items {
        let template_code = item.template_code.trim();
        let case_id = item.case_id.trim();
        if template_code.is_empty() || case_id.is_empty() {
            return Err(DomainError::ValidationError(
                "流程派发请求快照包含空事项类型或事项 ID".into(),
            ));
        }
        items.push(BusinessCaseWorkflowBatchItem {
            template_code: template_code.to_string(),
            case_id: case_id.to_string(),
        });
    }
    if items.is_empty() {
        return Err(DomainError::ValidationError("流程派发请求快照不包含可重试事项".into()));
    }
    Ok(items)
}

pub(super) fn reject_duplicate_copilot_action_ids_in_batch(
    actions: &[PreparedCommitAction],
) -> Result<(), DomainError> {
    let mut seen = HashSet::new();
    for action in actions {
        let action_id = action.action.action_id.trim();
        if !seen.insert(action_id.to_ascii_lowercase()) {
            return Err(DomainError::ValidationError(format!(
                "批次内存在重复 action_id: {}",
                action.action.action_id
            )));
        }
    }
    Ok(())
}

pub(super) fn reject_duplicate_copilot_actions_in_batch(actions: &[PreparedCommitAction]) -> Result<(), DomainError> {
    for (idx, current) in actions.iter().enumerate() {
        if !current.duplicate_policy.enabled {
            continue;
        }
        if let Some(existing) = actions[..idx]
            .iter()
            .find(|existing| is_duplicate_prepared_copilot_action(existing, current))
        {
            return Err(DomainError::ValidationError(format!(
                "批次内存在重复业务事项，航班 {} 事项类型 {} 与动作 {} 重复",
                current.flight_no, current.action.case_type, existing.action.action_id
            )));
        }
    }
    Ok(())
}

pub(super) async fn reject_duplicate_copilot_action(
    business_case_service: &dyn BusinessCaseServiceOps,
    prepared: &PreparedCommitAction,
    viewer_department_id: Option<&str>,
    viewer_department_name: Option<&str>,
) -> Result<(), DomainError> {
    if !prepared.duplicate_policy.enabled {
        return Ok(());
    }
    let existing_cases = business_case_service
        .get_by_flight_for_viewer(&prepared.flight_id, viewer_department_id, viewer_department_name)
        .await?;
    if let Some(existing) = existing_cases
        .iter()
        .find(|existing| is_duplicate_copilot_case(existing, prepared))
    {
        return Err(DomainError::ValidationError(format!(
            "已存在相同业务事项，航班 {} 事项类型 {} 与既有事项 {} 重复",
            prepared.flight_no, prepared.action.case_type, existing.case_id
        )));
    }
    Ok(())
}

pub(super) fn is_duplicate_copilot_case(existing: &FlightBusinessCase, prepared: &PreparedCommitAction) -> bool {
    let policy = &prepared.duplicate_policy;
    if !policy.enabled || existing.case_type != prepared.action.case_type {
        return false;
    }
    if !is_active_duplicate_status(existing.status.as_str(), policy) {
        return false;
    }
    if policy.include_bound_leg
        && normalized_context_string(&existing.context, "bound_leg_type")
            != normalized_context_string(&prepared.context, "bound_leg_type")
    {
        return false;
    }
    if policy.include_extra_info
        && normalized_context_string(&existing.context, "extra_info")
            != normalized_context_string(&prepared.context, "extra_info")
    {
        return false;
    }
    for field in &policy.fields {
        let field = field.trim();
        if field.is_empty() {
            continue;
        }
        if normalized_context_string(&existing.context, field) != normalized_context_string(&prepared.context, field) {
            return false;
        }
    }
    true
}

pub(super) fn is_duplicate_prepared_copilot_action(
    existing: &PreparedCommitAction,
    prepared: &PreparedCommitAction,
) -> bool {
    let policy = &prepared.duplicate_policy;
    if !policy.enabled
        || existing.action.case_type != prepared.action.case_type
        || existing.flight_id != prepared.flight_id
    {
        return false;
    }
    if policy.include_bound_leg
        && normalized_context_string(&existing.context, "bound_leg_type")
            != normalized_context_string(&prepared.context, "bound_leg_type")
    {
        return false;
    }
    if policy.include_extra_info
        && normalized_context_string(&existing.context, "extra_info")
            != normalized_context_string(&prepared.context, "extra_info")
    {
        return false;
    }
    for field in &policy.fields {
        let field = field.trim();
        if field.is_empty() {
            continue;
        }
        if normalized_context_string(&existing.context, field) != normalized_context_string(&prepared.context, field) {
            return false;
        }
    }
    true
}

pub(super) fn is_active_duplicate_status(status: &str, policy: &CaseDuplicatePolicy) -> bool {
    if policy.active_statuses.is_empty() {
        let normalized = status.trim().to_ascii_uppercase();
        return !matches!(
            normalized.as_str(),
            "FINISHED" | "COMPLETED" | "RESOLVED" | "CANCELLED" | "CANCELED" | "REJECTED"
        );
    }
    policy
        .active_statuses
        .iter()
        .any(|item| item.trim().eq_ignore_ascii_case(status.trim()))
}

pub(super) fn normalized_context_string(context: &HashMap<String, Value>, key: &str) -> Option<String> {
    context.get(key).and_then(|value| match value {
        Value::Null => None,
        Value::String(s) => {
            let normalized = s.trim();
            (!normalized.is_empty()).then(|| normalized.to_ascii_uppercase())
        }
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => Some(value.to_string()),
    })
}

pub(super) fn build_notification_groups(
    actions: &[AiCopilotApprovedAction],
    case_ids: &[String],
) -> Vec<AiCopilotNotificationGroup> {
    let mut by_type: HashMap<String, Vec<(String, &AiCopilotApprovedAction)>> = HashMap::new();
    for (idx, action) in actions.iter().enumerate() {
        by_type
            .entry(action.case_type.clone())
            .or_default()
            .push((case_ids.get(idx).cloned().unwrap_or_default(), action));
    }
    by_type
        .into_iter()
        .map(|(case_type, items)| {
            let count = items.len();
            let body = items
                .iter()
                .map(|(_, action)| {
                    format!(
                        "{} {}",
                        action.flight_no,
                        action.remarks.as_deref().unwrap_or("").trim()
                    )
                    .trim()
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join(" / ");
            AiCopilotNotificationGroup {
                group_id: ulid::Ulid::new().to_string(),
                case_type,
                case_ids: items.into_iter().map(|(case_id, _)| case_id).collect(),
                title: format!("{count} 个航班业务事项"),
                body,
            }
        })
        .collect()
}
