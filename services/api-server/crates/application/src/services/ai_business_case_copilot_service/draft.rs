//! Draft-related methods for `AiBusinessCaseCopilotService`.
//!
//! Split from `service.rs` to keep file sizes manageable. These methods handle
//! voice-transcript draft generation, LLM extraction, diagnostic previews,
//! and flight candidate matching.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::ai_copilot::{AiCopilotBatchStatus, AiCopilotBusinessCaseBatch};
use fms_domain::ports::ai_copilot_repository::AiCopilotBusinessCaseBatchRepository;

use super::config::{AiFlightMatchingConfig, AiLegBindingConfig, CopilotCaseTypeCatalogEntry};
use super::helpers::*;
use super::schemas::{
    AiCopilotCaseTypeDiagnostic, AiCopilotDraftAction, AiCopilotDraftDiagnosticResponse, AiCopilotDraftRequest,
    AiCopilotDraftResponse, AiCopilotMatchedFlight, LlmDraftAction, LlmDraftPayload,
};
use super::service::AiBusinessCaseCopilotService;

const DEFAULT_SOURCE_PAGE: &str = "flight_monitor";

impl<R> AiBusinessCaseCopilotService<R>
where
    R: AiCopilotBusinessCaseBatchRepository + Send + Sync + ?Sized,
{
    pub async fn draft_from_transcript(
        &self,
        request: AiCopilotDraftRequest,
        actor: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common_case_types: bool,
    ) -> Result<AiCopilotDraftResponse, DomainError> {
        let transcript = request.transcript.trim();
        if transcript.is_empty() {
            return Err(DomainError::ValidationError("transcript is required".into()));
        }
        let entity_id = request.entity_id.trim();
        if entity_id.is_empty() {
            return Err(DomainError::ValidationError("entity_id is required".into()));
        }

        let catalog = self
            .load_case_type_catalog(viewer_department_id, viewer_department_name, include_common_case_types)
            .await?;
        let max_candidates = request
            .context
            .get("max_candidate_case_types")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(8)
            .clamp(1, 20);
        let candidates_catalog = retrieve_candidate_case_types(transcript, &catalog, max_candidates);

        let llm_payload = self
            .extract_actions_with_llm(entity_id, transcript, &candidates_catalog)
            .await?;
        let summary = if llm_payload.summary.trim().is_empty() {
            summarize_transcript(transcript)
        } else {
            llm_payload.summary.trim().to_string()
        };

        let catalog_by_code: HashMap<&str, &CopilotCaseTypeCatalogEntry> =
            catalog.iter().map(|entry| (entry.code.as_str(), entry)).collect();

        let mut actions = Vec::new();
        for (idx, action) in llm_payload.actions.into_iter().enumerate() {
            let validation_res = validate_and_enrich_action(action, idx, &catalog_by_code);
            let normalized = validation_res.action;
            let mut needs_review = validation_res.needs_review;
            let mut review_reason = validation_res.review_reason;

            let entry_opt = catalog_by_code.get(normalized.case_type.as_str());
            let (matching_cfg, leg_binding_cfg) = match entry_opt {
                Some(entry) => {
                    let (merged_matching, merged_leg) = merge_copilot_flight_binding(
                        &entry.config.flight_matching,
                        &entry.config.leg_binding,
                        &entry.case_properties,
                    );
                    // Store as owned values on the stack for reference borrowing
                    let stack_matching = merged_matching;
                    let stack_leg = merged_leg;
                    (stack_matching, stack_leg)
                }
                None => {
                    static DEFAULT_MATCHING: AiFlightMatchingConfig = AiFlightMatchingConfig {
                        allow_numeric_suffix: Some(true),
                        prefer_leg: None,
                        exclude_cancelled: Some(true),
                        exclude_departed: Some(true),
                        exclude_actual_departure: Some(true),
                        window_hours_before: Some(3),
                        window_hours_after: Some(8),
                        min_auto_match_score: Some(0.85),
                    };
                    static DEFAULT_LEG: AiLegBindingConfig = AiLegBindingConfig {
                        allowed: vec![],
                        default: None,
                        required: false,
                    };
                    (DEFAULT_MATCHING.clone(), DEFAULT_LEG.clone())
                }
            };
            let matching_cfg_ref = &matching_cfg;
            let leg_binding_cfg_ref = &leg_binding_cfg;

            let candidates = self
                .match_flight_candidates(&normalized, matching_cfg_ref, leg_binding_cfg_ref)
                .await?;

            let min_score = matching_cfg_ref.min_auto_match_score.unwrap_or(0.85);
            let (matched_flight, flight_needs_review, flight_review_reason) =
                resolve_match_review(&candidates, min_score);

            if flight_needs_review {
                needs_review = true;
                review_reason = match (review_reason, flight_review_reason) {
                    (Some(r1), Some(r2)) => Some(format!("{}; {}", r1, r2)),
                    (Some(r1), None) => Some(r1),
                    (None, Some(r2)) => Some(r2),
                    (None, None) => None,
                };
            }

            actions.push(AiCopilotDraftAction {
                action_id: format!("act_{}", idx + 1),
                case_type: normalized.case_type,
                case_type_name: normalized.case_type_name,
                flight_number_raw: normalized.flight_number_raw,
                leg_type_hint: normalized.leg_type_hint.unwrap_or_else(|| "outbound".to_string()),
                description: normalized.description,
                remarks: normalized.remarks,
                fields: normalized.fields,
                confidence: normalized.confidence.unwrap_or(0.7).clamp(0.0, 1.0),
                needs_review,
                review_reason,
                matched_flight,
                candidates,
            });
        }

        if actions.is_empty() {
            return Err(DomainError::ValidationError(
                "未从语音内容中识别到可创建的业务事项".into(),
            ));
        }

        let now = Utc::now();
        let expires_at = now + Duration::hours(24);
        let batch_id = ulid::Ulid::new().to_string();
        let draft_actions = serde_json::to_value(&actions).map_err(|error| DomainError::Internal(error.to_string()))?;
        let batch = AiCopilotBusinessCaseBatch {
            batch_id: batch_id.clone(),
            entity_id: entity_id.to_string(),
            source_page: request
                .source_page
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_SOURCE_PAGE)
                .to_string(),
            transcript_summary: summary.clone(),
            transcript_text: transcript.to_string(),
            draft_actions,
            status: AiCopilotBatchStatus::Draft,
            created_by: actor.trim().to_string(),
            committed_case_ids: vec![],
            idempotency_key: None,
            notification_groups: json!([]),
            commit_request: None,
            created_action_case_ids: json!({}),
            commit_error: None,
            commit_started_at: None,
            commit_attempts: 0,
            commit_next_recovery_at: None,
            committed_at: None,
            workflow_dispatch_status: "not_required".to_string(),
            workflow_dispatch_request: None,
            workflow_dispatch_error: None,
            workflow_dispatch_attempts: 0,
            workflow_dispatch_next_retry_at: None,
            workflow_dispatched_at: None,
            created_at: now,
            updated_at: now,
            expires_at,
        };
        self.repo.save(&batch).await?;

        Ok(AiCopilotDraftResponse {
            batch_id,
            summary,
            transcript: transcript.to_string(),
            actions,
            expires_at,
        })
    }

    pub async fn diagnose_draft_from_transcript(
        &self,
        request: AiCopilotDraftRequest,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common_case_types: bool,
    ) -> Result<AiCopilotDraftDiagnosticResponse, DomainError> {
        let transcript = request.transcript.trim();
        if transcript.is_empty() {
            return Err(DomainError::ValidationError("transcript is required".into()));
        }
        let entity_id = request.entity_id.trim();
        if entity_id.is_empty() {
            return Err(DomainError::ValidationError("entity_id is required".into()));
        }

        let catalog = self
            .load_case_type_catalog(viewer_department_id, viewer_department_name, include_common_case_types)
            .await?;
        let max_candidates = request
            .context
            .get("max_candidate_case_types")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(8)
            .clamp(1, 20);
        let candidates_catalog = retrieve_candidate_case_types(transcript, &catalog, max_candidates);
        let candidate_case_types = candidates_catalog
            .iter()
            .map(|entry| AiCopilotCaseTypeDiagnostic {
                code: entry.code.clone(),
                name: entry.name.clone(),
            })
            .collect::<Vec<_>>();

        let prompt = build_extraction_prompt(transcript, &candidates_catalog);
        let raw_response = match self.ai_admin_service.complete_text(entity_id, &prompt).await {
            Ok(value) => value,
            Err(error) => {
                return Ok(AiCopilotDraftDiagnosticResponse {
                    ok: false,
                    entity_id: entity_id.to_string(),
                    transcript_summary: summarize_transcript(transcript),
                    candidate_case_types,
                    llm_raw_preview: None,
                    parsed_payload: None,
                    error_stage: Some("llm_call".to_string()),
                    error_message: Some(error.to_string()),
                });
            }
        };
        let llm_raw_preview = Some(raw_response.chars().take(4000).collect::<String>());
        let json_text = match extract_json_text(&raw_response) {
            Some(value) => value,
            None => {
                return Ok(AiCopilotDraftDiagnosticResponse {
                    ok: false,
                    entity_id: entity_id.to_string(),
                    transcript_summary: summarize_transcript(transcript),
                    candidate_case_types,
                    llm_raw_preview,
                    parsed_payload: None,
                    error_stage: Some("json_extract".to_string()),
                    error_message: Some("AI response did not contain JSON".to_string()),
                });
            }
        };
        let parsed_payload = match serde_json::from_str::<Value>(&json_text) {
            Ok(value) => value,
            Err(error) => {
                return Ok(AiCopilotDraftDiagnosticResponse {
                    ok: false,
                    entity_id: entity_id.to_string(),
                    transcript_summary: summarize_transcript(transcript),
                    candidate_case_types,
                    llm_raw_preview,
                    parsed_payload: None,
                    error_stage: Some("json_parse".to_string()),
                    error_message: Some(error.to_string()),
                });
            }
        };
        let typed_payload = match serde_json::from_value::<LlmDraftPayload>(parsed_payload.clone()) {
            Ok(value) => value,
            Err(error) => {
                return Ok(AiCopilotDraftDiagnosticResponse {
                    ok: false,
                    entity_id: entity_id.to_string(),
                    transcript_summary: summarize_transcript(transcript),
                    candidate_case_types,
                    llm_raw_preview,
                    parsed_payload: Some(parsed_payload),
                    error_stage: Some("schema_parse".to_string()),
                    error_message: Some(error.to_string()),
                });
            }
        };

        Ok(AiCopilotDraftDiagnosticResponse {
            ok: true,
            entity_id: entity_id.to_string(),
            transcript_summary: if typed_payload.summary.trim().is_empty() {
                summarize_transcript(transcript)
            } else {
                typed_payload.summary.trim().to_string()
            },
            candidate_case_types,
            llm_raw_preview,
            parsed_payload: Some(parsed_payload),
            error_stage: None,
            error_message: None,
        })
    }

    async fn extract_actions_with_llm(
        &self,
        entity_id: &str,
        transcript: &str,
        catalog: &[CopilotCaseTypeCatalogEntry],
    ) -> Result<LlmDraftPayload, DomainError> {
        let prompt = build_extraction_prompt(transcript, catalog);
        let response = self.ai_admin_service.complete_text(entity_id, &prompt).await?;
        parse_llm_payload(&response)
    }

    async fn match_flight_candidates(
        &self,
        action: &LlmDraftAction,
        config: &AiFlightMatchingConfig,
        leg_binding: &AiLegBindingConfig,
    ) -> Result<Vec<AiCopilotMatchedFlight>, DomainError> {
        let raw = action.flight_number_raw.trim().to_uppercase();
        if raw.is_empty() {
            return Ok(vec![]);
        }
        let flights = self.flight_repo.find_by_flight_number(&raw).await?;
        let all = if flights.is_empty() && raw.chars().all(|ch| ch.is_ascii_digit()) {
            self.flight_repo.find_all(500, 0).await?
        } else {
            flights
        };
        let mut seen = HashSet::new();
        let mut candidates = all
            .into_iter()
            .filter_map(|flight| match_flight(&flight, &raw, config, leg_binding))
            .filter(|candidate| seen.insert(candidate.flight_id.clone()))
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(5);
        Ok(candidates)
    }
}
