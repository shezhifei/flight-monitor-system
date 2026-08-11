use chrono::Utc;
use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

use async_trait::async_trait;

use fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository;

use crate::schemas::flowable_draft_schemas::{
    FlowableDraftAssistantChatData, FlowableDraftAssistantChatRequest, FlowableProcessDraftData,
};

use super::assistant::{
    build_contextual_sections, build_general_sections, build_streamed_markdown, build_synthetic_draft,
    extract_ai_message_content, extract_ai_stream_delta, extract_model_name, humanize_process_name,
    normalize_process_key, parse_ai_json, validate_bpmn_xml, DraftAiConfig, DraftAiOutput, DRAFT_JSON_SYSTEM_PROMPT,
};
use super::assistant::{build_warnings, extract_requirements};
use super::document_parse::{parse_process_document, ParsedProcessDocument};
use super::error::FlowableDraftServiceError;
use super::stream::FlowableDraftAssistantStreamEvent;

#[derive(Clone)]
pub struct FlowableDraftService {
    ai_config_repo: Option<Arc<dyn AiEntityConfigRepository + Send + Sync>>,
    http_client: reqwest::Client,
    allow_synthetic_ai: bool,
}

#[derive(Debug, Clone)]
pub struct NoopAiEntityConfigRepository;

#[async_trait]
impl fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository for NoopAiEntityConfigRepository {
    async fn find_all(
        &self,
    ) -> Result<Vec<fms_domain::models::ai_entity_config::AiEntityConfigRecord>, fms_domain::error::DomainError> {
        Ok(vec![])
    }
    async fn find_by_id(
        &self,
        _id: &str,
    ) -> Result<Option<fms_domain::models::ai_entity_config::AiEntityConfigRecord>, fms_domain::error::DomainError>
    {
        Ok(None)
    }
    async fn save(
        &self,
        _id: &str,
        _config: &serde_json::Value,
    ) -> Result<fms_domain::models::ai_entity_config::AiEntityConfigRecord, fms_domain::error::DomainError> {
        Err(fms_domain::error::DomainError::Internal("noop repository".into()))
    }
    async fn delete(&self, _id: &str) -> Result<bool, fms_domain::error::DomainError> {
        Ok(false)
    }
}

impl FlowableDraftService {
    fn build_pooled_reqwest_client() -> reqwest::Client {
        crate::http_client::shared_http_client()
    }

    pub fn new() -> Self {
        Self {
            ai_config_repo: None,
            http_client: Self::build_pooled_reqwest_client(),
            allow_synthetic_ai: false,
        }
    }

    pub fn with_synthetic_ai_fallback(mut self, enabled: bool) -> Self {
        self.allow_synthetic_ai = enabled;
        self
    }
}

impl FlowableDraftService {
    pub fn with_ai_config_repo(mut self, repo: Arc<dyn AiEntityConfigRepository + Send + Sync>) -> Self {
        self.ai_config_repo = Some(repo);
        self
    }

    pub async fn generate_from_file(
        &self,
        filename: &str,
        file_bytes: &[u8],
        process_key: Option<&str>,
        process_name: Option<&str>,
        case_type_code: Option<&str>,
        locale: Option<&str>,
    ) -> Result<FlowableProcessDraftData, FlowableDraftServiceError> {
        let parsed = parse_process_document(filename, file_bytes)?;

        let resolved_key = normalize_process_key(process_key, process_name, case_type_code, filename);
        let resolved_name = process_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| humanize_process_name(&resolved_key));
        let resolved_locale = locale
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("zh-CN");
        let requirements = extract_requirements(&parsed.text);
        let mut warnings = parsed.warnings.clone();
        warnings.extend(build_warnings(&parsed.text, &requirements));
        let fallback_summary = super::assistant::build_summary_markdown(
            &resolved_key,
            &resolved_name,
            case_type_code,
            resolved_locale,
            &requirements,
            filename,
            &warnings,
        );

        let draft_payload = if let Some(config) = self.resolve_ai_config().await? {
            self.generate_ai_draft(
                &config,
                process_key,
                process_name,
                case_type_code,
                resolved_locale,
                &parsed,
                &requirements,
            )
            .await?
        } else if self.allow_synthetic_ai {
            build_synthetic_draft(
                &resolved_key,
                &resolved_name,
                case_type_code,
                resolved_locale,
                &parsed,
                &requirements,
                &warnings,
            )
        } else {
            return Err(FlowableDraftServiceError::AIUnavailable(
                "AI 不可用：未检测到有效 AI 配置".to_string(),
            ));
        };

        warnings.extend(draft_payload.warnings.iter().cloned());
        warnings.sort();
        warnings.dedup();
        let draft_summary_markdown = if draft_payload.draft_summary_markdown.trim().is_empty() {
            fallback_summary
        } else {
            draft_payload.draft_summary_markdown
        };

        Ok(FlowableProcessDraftData {
            draft_bpmn_xml: draft_payload.draft_bpmn_xml,
            draft_summary_markdown,
            extracted_requirements: if draft_payload.extracted_requirements.is_empty() {
                requirements
            } else {
                draft_payload.extracted_requirements
            },
            warnings,
            source_meta: parsed.source_meta,
            generated_at: Utc::now().to_rfc3339(),
            model: draft_payload.model,
        })
    }

    pub async fn chat_assistant(
        &self,
        payload: FlowableDraftAssistantChatRequest,
        user_id: &str,
    ) -> Result<FlowableDraftAssistantChatData, FlowableDraftServiceError> {
        self.chat_assistant_with_stream(payload, user_id, None).await
    }

    pub async fn chat_assistant_with_stream(
        &self,
        payload: FlowableDraftAssistantChatRequest,
        user_id: &str,
        event_sender: Option<mpsc::Sender<FlowableDraftAssistantStreamEvent>>,
    ) -> Result<FlowableDraftAssistantChatData, FlowableDraftServiceError> {
        let message = payload.message.trim();
        if message.is_empty() {
            return Err(FlowableDraftServiceError::InvalidRequest(
                "message cannot be empty".into(),
            ));
        }

        let mode = if payload.mode.trim().eq_ignore_ascii_case("general") {
            "general".to_string()
        } else {
            "contextual".to_string()
        };

        let mut warnings = payload
            .context
            .as_ref()
            .map(|ctx| ctx.warnings.clone())
            .unwrap_or_default();
        let sections = if mode == "general" {
            build_general_sections(message, user_id)
        } else {
            build_contextual_sections(message, user_id, payload.context.as_ref(), &mut warnings)
        };
        let generated_at = Utc::now().to_rfc3339();
        let default_model = "flowable-draft-chat-v1".to_string();

        if let Some(sender) = event_sender.as_ref() {
            let _ = sender.try_send(FlowableDraftAssistantStreamEvent::Progress {
                stage: "start".to_string(),
                message: "开始处理流程助手请求".to_string(),
                mode: mode.clone(),
            });
        }

        let (mut answer_markdown, model) = if let Some(config) = self.resolve_ai_config().await? {
            self.chat_assistant_via_ai(
                &config,
                &payload,
                user_id,
                message,
                &mode,
                &generated_at,
                &warnings,
                event_sender.as_ref(),
            )
            .await?
        } else if self.allow_synthetic_ai {
            (
                build_streamed_markdown(&mode, &sections, warnings.len(), &default_model, event_sender.as_ref()).await,
                default_model,
            )
        } else {
            return Err(FlowableDraftServiceError::AIUnavailable(
                "AI 不可用：未检测到有效 AI 配置".to_string(),
            ));
        };
        let review_note = "提示：AI 输出仅供参考，部署前请务必人工检阅。";
        if !answer_markdown.contains(review_note) {
            answer_markdown = format!("{}\n\n> {}", answer_markdown.trim(), review_note);
        }

        Ok(FlowableDraftAssistantChatData {
            answer_markdown,
            mode,
            warnings,
            generated_at,
            model,
        })
    }

    async fn resolve_ai_config(&self) -> Result<Option<DraftAiConfig>, FlowableDraftServiceError> {
        let Some(repo) = self.ai_config_repo.as_ref() else {
            return Ok(None);
        };
        let items = repo
            .find_all()
            .await
            .map_err(|error| FlowableDraftServiceError::AIUnavailable(error.to_string()))?;
        let record = items.iter().find(|item| item.id == "default").or_else(|| {
            items.iter().find(|item| {
                item.config
                    .get("api_key")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .is_some_and(|value| !value.is_empty())
            })
        });

        let Some(record) = record else {
            return Ok(None);
        };
        let api_key = record
            .config
            .get("api_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if api_key.is_empty() {
            return Ok(None);
        }

        Ok(Some(DraftAiConfig {
            base_url: record
                .config
                .get("base_url")
                .and_then(Value::as_str)
                .unwrap_or("https://api.openai.com/v1")
                .trim_end_matches('/')
                .to_string(),
            api_key,
            default_model: record
                .config
                .get("default_model")
                .and_then(Value::as_str)
                .unwrap_or("gpt-4o-mini")
                .to_string(),
            api_format: record
                .config
                .get("api_format")
                .and_then(Value::as_str)
                .unwrap_or("chat_completions")
                .to_string(),
            timeout_seconds: record.config.get("timeout").and_then(Value::as_f64).unwrap_or(30.0) as u64,
            max_tokens: record.config.get("max_tokens").and_then(Value::as_u64).unwrap_or(2400) as u32,
            max_retries: record.config.get("max_retries").and_then(Value::as_u64).unwrap_or(1) as usize,
            retry_delay_seconds: record.config.get("retry_delay").and_then(Value::as_f64).unwrap_or(0.5),
        }))
    }

    async fn generate_ai_draft(
        &self,
        config: &DraftAiConfig,
        process_key: Option<&str>,
        process_name: Option<&str>,
        case_type_code: Option<&str>,
        locale: &str,
        parsed: &ParsedProcessDocument,
        requirements: &[String],
    ) -> Result<DraftAiOutput, FlowableDraftServiceError> {
        let (payload, mut model_name) = self
            .request_ai_json(
                config,
                DRAFT_JSON_SYSTEM_PROMPT,
                self.build_generation_instructions(
                    process_key,
                    process_name,
                    case_type_code,
                    locale,
                    parsed,
                    requirements,
                ),
            )
            .await?;

        let mut output = self.normalize_ai_draft_output(&payload, &model_name, requirements);
        let original_summary = output.draft_summary_markdown.clone();
        let mut original_warnings = output.warnings.clone();
        let mut original_requirements = output.extracted_requirements.clone();
        if let Some(validation_error) = validate_bpmn_xml(&output.draft_bpmn_xml) {
            let (corrected_payload, corrected_model) = self
                .request_ai_json(
                    config,
                    DRAFT_JSON_SYSTEM_PROMPT,
                    self.build_correction_instructions(
                        process_key,
                        process_name,
                        case_type_code,
                        locale,
                        parsed,
                        requirements,
                        &output.draft_bpmn_xml,
                        &validation_error,
                    ),
                )
                .await?;
            model_name = corrected_model;
            output = self.normalize_ai_draft_output(&corrected_payload, &model_name, requirements);
            let mut merged_requirements = original_requirements.clone();
            merged_requirements.extend(output.extracted_requirements.clone());
            merged_requirements.sort();
            merged_requirements.dedup();
            output.extracted_requirements = merged_requirements;
            if output.draft_summary_markdown.trim().is_empty() {
                output.draft_summary_markdown = original_summary;
            }
            if output.warnings.is_empty() {
                output.warnings = original_warnings.clone();
            } else {
                original_warnings.extend(output.warnings.clone());
                original_warnings.sort();
                original_warnings.dedup();
                output.warnings = original_warnings.clone();
            }
            if let Some(final_error) = validate_bpmn_xml(&output.draft_bpmn_xml) {
                return Err(FlowableDraftServiceError::BpmnDraftValidation {
                    code: "INVALID_BPMN_DRAFT".to_string(),
                    message: final_error,
                });
            }
        }

        Ok(output)
    }

    async fn chat_assistant_via_ai(
        &self,
        config: &DraftAiConfig,
        payload: &FlowableDraftAssistantChatRequest,
        user_id: &str,
        message: &str,
        mode: &str,
        _generated_at: &str,
        warnings: &[String],
        event_sender: Option<&mpsc::Sender<FlowableDraftAssistantStreamEvent>>,
    ) -> Result<(String, String), FlowableDraftServiceError> {
        let system_prompt = if mode == "general" {
            "你是机场流程建模助手。请用简体中文回答，优先给出可执行建议。输出请使用 Markdown，结构包含：结论、建议作业类型、人工复核点。".to_string()
        } else {
            "你是机场流程草案审阅助手。请结合提供的草案上下文回答，优先指出风险、歧义与需人工确认内容。输出请使用 Markdown，结构包含：结论、基于当前草案的建议、人工复核点。".to_string()
        };
        let user_prompt = if mode == "general" {
            format!("用户问题（通用流程问答模式）:\n{}\n\n用户: {}", message, user_id)
        } else {
            format!(
                "用户问题（围绕当前草案模式）:\n{}\n\n当前草案上下文(JSON):\n{}\n\n用户: {}",
                message,
                serde_json::to_string(&payload.context).unwrap_or_else(|_| "{}".to_string()),
                user_id
            )
        };

        if let Some(sender) = event_sender {
            let _ = sender.try_send(FlowableDraftAssistantStreamEvent::Progress {
                stage: "ai_request".to_string(),
                message: "正在建立流式会话".to_string(),
                mode: mode.to_string(),
            });
        }

        let mut answer_parts = Vec::new();
        let mut stream_failed = false;
        let mut accumulated_chars = 0usize;
        if let Some(sender) = event_sender {
            match self.request_ai_stream_text(config, &system_prompt, &user_prompt).await {
                Ok((chunks, model_name)) => {
                    for delta in chunks {
                        accumulated_chars += delta.chars().count();
                        answer_parts.push(delta.clone());
                        let _ = sender.try_send(FlowableDraftAssistantStreamEvent::TextDelta {
                            mode: mode.to_string(),
                            delta,
                            accumulated_chars,
                        });
                    }
                    let _ = sender.try_send(FlowableDraftAssistantStreamEvent::Completed {
                        mode: mode.to_string(),
                        warning_count: warnings.len(),
                        model: model_name.clone(),
                    });
                    return Ok((answer_parts.join(""), model_name));
                }
                Err(error) => {
                    stream_failed = true;
                    let _ = sender.try_send(FlowableDraftAssistantStreamEvent::Error {
                        mode: mode.to_string(),
                        message: format!("流式请求失败：{}", error),
                    });
                }
            }
        }

        let (content, model_name) = self.request_ai_text(config, &system_prompt, &user_prompt).await?;
        if stream_failed {
            warn!("flowable assistant stream fallback to non-stream response");
            if let Some(sender) = event_sender {
                let _ = sender.try_send(FlowableDraftAssistantStreamEvent::Completed {
                    mode: mode.to_string(),
                    warning_count: warnings.len(),
                    model: model_name.clone(),
                });
            }
        }
        Ok((content, model_name))
    }

    pub(super) fn build_generation_instructions(
        &self,
        process_key: Option<&str>,
        process_name: Option<&str>,
        case_type_code: Option<&str>,
        locale: &str,
        parsed: &ParsedProcessDocument,
        requirements: &[String],
    ) -> String {
        let payload = json!({
            "target": {
                "process_key": process_key.unwrap_or("").trim(),
                "process_name": process_name.unwrap_or("").trim(),
                "case_type_code": case_type_code.unwrap_or("").trim(),
                "locale": locale,
            },
            "source_meta": parsed.source_meta,
            "warnings": parsed.warnings,
            "requirements_hint": requirements,
            "source_text": parsed.text,
        });
        format!(
            "请基于输入文档生成 Flowable 可导入的 BPMN 2.0 草案。任务要求：\n1) 至少包含 startEvent -> 至少一个任务节点 -> endEvent。\n2) process id/name 优先使用 process_key/process_name；缺失时合理推断。\n3) draft_summary_markdown 使用简体中文，说明主干流程与风险。\n4) extracted_requirements 提炼关键业务规则列表。\n5) warnings 返回潜在歧义或信息缺失点。\n6) 如果文档中描述了可复用或独立的子流程，使用 callActivity 元素引用已部署的子流程定义，calledElement 属性指定子流程的 processDefinitionKey。使用 <flowable:in source=\"主流程变量\" target=\"子流程变量\"/> 传入变量，使用 <flowable:out source=\"子流程变量\" target=\"主流程变量\"/> 取出输出变量。\n7) 在 callActivity 之后使用 exclusiveGateway，根据子流程输出变量值编写 conditionExpression 实现分支选择，例如 ${{subProcessOutcome == 'resolved'}} 和 ${{subProcessOutcome == 'escalated'}}。\n\n输入JSON:\n{}",
            serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
        )
    }

    fn build_correction_instructions(
        &self,
        process_key: Option<&str>,
        process_name: Option<&str>,
        case_type_code: Option<&str>,
        locale: &str,
        parsed: &ParsedProcessDocument,
        requirements: &[String],
        invalid_xml: &str,
        validation_error: &str,
    ) -> String {
        let payload = json!({
            "target": {
                "process_key": process_key.unwrap_or("").trim(),
                "process_name": process_name.unwrap_or("").trim(),
                "case_type_code": case_type_code.unwrap_or("").trim(),
                "locale": locale,
            },
            "source_meta": parsed.source_meta,
            "requirements_hint": requirements,
            "validation_error": validation_error,
            "invalid_bpmn_xml": invalid_xml,
        });
        format!(
            "上一次输出的 BPMN XML 未通过服务端校验。请仅输出修正后的 JSON 对象（同样包含 draft_bpmn_xml/draft_summary_markdown/extracted_requirements/warnings），并确保 XML 可解析且含 definitions/process/startEvent/endEvent，以及 bpmndi:BPMNDiagram/bpmndi:BPMNPlane 图形信息。\n\n修正输入JSON:\n{}",
            serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
        )
    }

    pub(super) fn normalize_ai_draft_output(
        &self,
        payload: &Value,
        model_name: &str,
        fallback_requirements: &[String],
    ) -> DraftAiOutput {
        let draft_bpmn_xml = payload
            .get("draft_bpmn_xml")
            .or_else(|| payload.get("bpmn_xml"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let draft_summary_markdown = payload
            .get("draft_summary_markdown")
            .or_else(|| payload.get("summary_markdown"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        DraftAiOutput {
            draft_bpmn_xml,
            draft_summary_markdown,
            extracted_requirements: payload
                .get("extracted_requirements")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|items| !items.is_empty())
                .unwrap_or_else(|| fallback_requirements.to_vec()),
            warnings: payload
                .get("warnings")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            model: model_name.to_string(),
        }
    }

    async fn request_ai_json(
        &self,
        config: &DraftAiConfig,
        system_prompt: &str,
        user_content: String,
    ) -> Result<(Value, String), FlowableDraftServiceError> {
        let response = self
            .request_ai_response_with_prompt(config, system_prompt, &user_content, true, false)
            .await?;
        let model = extract_model_name(&response).unwrap_or_else(|| config.default_model.clone());
        let content = extract_ai_message_content(&response);
        let payload = parse_ai_json(&content);
        Ok((payload, model))
    }

    async fn request_ai_text(
        &self,
        config: &DraftAiConfig,
        system_prompt: &str,
        user_content: &str,
    ) -> Result<(String, String), FlowableDraftServiceError> {
        let response = self
            .request_ai_response_with_prompt(config, system_prompt, user_content, false, false)
            .await?;
        let model = extract_model_name(&response).unwrap_or_else(|| config.default_model.clone());
        Ok((extract_ai_message_content(&response), model))
    }

    async fn request_ai_stream_text(
        &self,
        config: &DraftAiConfig,
        system_prompt: &str,
        user_content: &str,
    ) -> Result<(Vec<String>, String), FlowableDraftServiceError> {
        let endpoint = if config.api_format.eq_ignore_ascii_case("responses") {
            format!("{}/responses", config.base_url)
        } else {
            format!("{}/chat/completions", config.base_url)
        };
        let payload = if config.api_format.eq_ignore_ascii_case("responses") {
            json!({
                "model": config.default_model,
                "instructions": system_prompt,
                "input": [{"role": "user", "content": user_content}],
                "temperature": 0.2,
                "max_output_tokens": config.max_tokens,
                "stream": true,
            })
        } else {
            json!({
                "model": config.default_model,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": user_content}
                ],
                "temperature": 0.2,
                "max_tokens": config.max_tokens,
                "stream": true,
            })
        };

        let response = self
            .http_client
            .post(endpoint)
            .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
            .header(CONTENT_TYPE, "application/json")
            .timeout(std::time::Duration::from_secs(config.timeout_seconds.max(1)))
            .json(&payload)
            .send()
            .await
            .map_err(|error| FlowableDraftServiceError::AIUnavailable(error.to_string()))?;

        let mut model_name = config.default_model.clone();
        let mut chunks = Vec::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        while let Some(item) = stream.next().await {
            let item = item.map_err(|error| FlowableDraftServiceError::AIUnavailable(error.to_string()))?;
            buffer.push_str(&String::from_utf8_lossy(&item));
            while let Some(pos) = buffer.find('\n') {
                let line = buffer.drain(..=pos).collect::<String>();
                let trimmed = line.trim();
                if let Some(data) = trimmed.strip_prefix("data:") {
                    let data = data.trim();
                    if data == "[DONE]" || data.is_empty() {
                        continue;
                    }
                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                        if let Some(model) = extract_model_name(&json) {
                            model_name = model;
                        }
                        if let Some(delta) = extract_ai_stream_delta(&json) {
                            if !delta.is_empty() {
                                chunks.push(delta);
                            }
                        }
                    }
                }
            }
        }
        Ok((chunks, model_name))
    }

    async fn request_ai_response_with_prompt(
        &self,
        config: &DraftAiConfig,
        system_prompt: &str,
        user_content: &str,
        expect_json: bool,
        stream: bool,
    ) -> Result<Value, FlowableDraftServiceError> {
        let endpoint = if config.api_format.eq_ignore_ascii_case("responses") {
            format!("{}/responses", config.base_url)
        } else {
            format!("{}/chat/completions", config.base_url)
        };
        let payload = if config.api_format.eq_ignore_ascii_case("responses") {
            json!({
                "model": config.default_model,
                "instructions": system_prompt,
                "input": [{"role": "user", "content": user_content}],
                "temperature": 0.2,
                "max_output_tokens": config.max_tokens,
                "stream": stream,
            })
        } else {
            let mut body = json!({
                "model": config.default_model,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": user_content}
                ],
                "temperature": 0.2,
                "max_tokens": config.max_tokens,
                "stream": stream,
            });
            if expect_json {
                body["response_format"] = json!({"type": "json_object"});
            }
            body
        };

        let mut last_error = None;
        for attempt in 0..=config.max_retries {
            let response = self
                .http_client
                .post(&endpoint)
                .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
                .header(CONTENT_TYPE, "application/json")
                .timeout(std::time::Duration::from_secs(config.timeout_seconds.max(1)))
                .json(&payload)
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    let json = response
                        .json::<Value>()
                        .await
                        .map_err(|error| FlowableDraftServiceError::AIUnavailable(error.to_string()))?;
                    if status.is_success() {
                        return Ok(json);
                    }
                    last_error = Some(format!("AI request failed with status {}: {}", status, json));
                }
                Err(error) => last_error = Some(error.to_string()),
            }

            if attempt < config.max_retries {
                tokio::time::sleep(std::time::Duration::from_secs_f64(config.retry_delay_seconds)).await;
            }
        }

        Err(FlowableDraftServiceError::AIUnavailable(
            last_error.unwrap_or_else(|| "AI request failed".to_string()),
        ))
    }
}
