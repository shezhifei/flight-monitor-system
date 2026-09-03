use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use serde_json::{json, Value};

use crate::schemas::nl_query_schemas::{
    NLConversationListDataSchema, NLConversationMessageItemSchema, NLConversationMessagesDataSchema,
    NLQueryContextSchema, NLQueryResultSchema, PaginationSchema,
};
use crate::services::ai_runtime_service::AiRuntimeService;
use crate::types::ConcreteFlightService;

use super::helpers::{
    build_tags, build_title, cleanup_conversations, content_to_text, context_to_metadata, enforce_conversation_cap,
    normalize_order, normalize_user_id, resolve_scene_from_context, sanitize_tool_payload, split_summary,
    to_conversation_item, trim_conversation_messages,
};
use super::types::{
    ConversationMessage, ConversationRecord, NLQueryRuntimeContext, NLQueryServiceError, NLQueryState,
    RuntimeQueryEvent,
};

#[derive(Clone)]
pub struct NLQueryService {
    state: Arc<NLQueryState>,
    pub(super) flight_service: Arc<ConcreteFlightService>,
    pub(super) runtime_service: Arc<AiRuntimeService>,
}

impl NLQueryService {
    pub fn new(flight_service: Arc<ConcreteFlightService>, runtime_service: Arc<AiRuntimeService>) -> Self {
        Self {
            state: Arc::new(NLQueryState::default()),
            flight_service,
            runtime_service,
        }
    }

    pub async fn query(
        &self,
        question: &str,
        user_id: &str,
        conversation_id: Option<&str>,
        context: Option<NLQueryContextSchema>,
    ) -> Result<NLQueryResultSchema, NLQueryServiceError> {
        self.query_with_runtime(question, user_id, conversation_id, context, None)
            .await
    }

    pub async fn query_with_runtime(
        &self,
        question: &str,
        user_id: &str,
        conversation_id: Option<&str>,
        context: Option<NLQueryContextSchema>,
        runtime: Option<NLQueryRuntimeContext>,
    ) -> Result<NLQueryResultSchema, NLQueryServiceError> {
        let started_at = Instant::now();
        let normalized_question = question.trim();
        if normalized_question.is_empty() {
            return Err(NLQueryServiceError::Validation("question cannot be empty".to_string()));
        }
        let normalized_user_id = normalize_user_id(user_id)?;
        let normalized_context = context.unwrap_or_default();
        let runtime = runtime.unwrap_or_else(|| NLQueryRuntimeContext {
            request_id: format!("req_{}", uuid::Uuid::new_v4()),
            scene: resolve_scene_from_context(&normalized_context),
            event_sender: None,
        });

        let conversation_id = self
            .append_user_message(
                &normalized_user_id,
                conversation_id,
                normalized_question,
                &normalized_context,
            )
            .await?;

        runtime.emit(
            "progress",
            json!({
                "request_id": runtime.request_id,
                "scene": runtime.scene,
                "stage": "start",
                "message": "开始处理查询请求",
                "conversation_id": conversation_id.clone(),
            }),
        );

        runtime.emit(
            "progress",
            json!({
                "request_id": runtime.request_id,
                "scene": runtime.scene,
                "stage": "analysis",
                "message": "正在分析查询意图",
                "conversation_id": conversation_id.clone(),
            }),
        );

        let analysis = self
            .analyze_question(
                normalized_question,
                &normalized_context,
                &normalized_user_id,
                &conversation_id,
                &runtime,
            )
            .await?;

        runtime.emit(
            "progress",
            json!({
                "request_id": runtime.request_id,
                "scene": runtime.scene,
                "stage": "analysis_completed",
                "status": "completed",
                "interpretation": analysis.interpretation.clone(),
                "visualization_hint": analysis.visualization_hint.clone(),
                "conversation_id": conversation_id.clone(),
            }),
        );

        if let Some(runtime_event) = analysis.runtime_event.as_ref() {
            self.append_tool_message(&conversation_id, &normalized_user_id, runtime_event)
                .await?;
        }

        runtime.emit(
            "progress",
            json!({
                "request_id": runtime.request_id,
                "scene": runtime.scene,
                "stage": "responding",
                "status": "in_progress",
                "conversation_id": conversation_id.clone(),
            }),
        );

        self.emit_summary_text_deltas(&analysis.summary, &conversation_id, &runtime)
            .await;

        self.append_assistant_message(
            &conversation_id,
            &normalized_user_id,
            &analysis.summary,
            analysis.tool_calls.clone(),
            analysis.metadata.clone(),
        )
        .await?;

        let result = NLQueryResultSchema {
            query: normalized_question.to_string(),
            interpretation: analysis.interpretation,
            structured_data: analysis.structured_data,
            visualization_hint: analysis.visualization_hint,
            summary: analysis.summary,
            conversation_id,
            duration_ms: started_at.elapsed().as_millis() as i64,
        };

        runtime.emit(
            "progress",
            json!({
                "request_id": runtime.request_id,
                "scene": runtime.scene,
                "stage": "completed",
                "status": "completed",
                "duration_ms": result.duration_ms,
                "conversation_id": result.conversation_id.clone(),
            }),
        );

        Ok(result)
    }

    pub async fn get_runtime_execution(&self, run_id: &str) -> Option<Value> {
        let now = Utc::now();
        cleanup_conversations(&self.state.conversations, now);

        self.state.conversations.iter().find_map(|entry| {
            let record = entry.value();
            record.messages.iter().find_map(|message| {
                let execution_id = message
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.get("execution_id"))
                    .and_then(Value::as_str)?;
                if execution_id != run_id {
                    return None;
                }

                Some(json!({
                    "execution_id": run_id,
                    "run_id": run_id,
                    "status": message
                        .metadata
                        .as_ref()
                        .and_then(|meta| meta.get("status"))
                        .and_then(Value::as_str)
                        .unwrap_or("success"),
                    "phase": "tool_execute",
                    "message": "working",
                    "tool_name": message.name,
                    "tool_call_id": message.tool_call_id,
                    "conversation_id": record.conversation_id,
                }))
            })
        })
    }

    pub async fn get_suggestions(&self, _user_id: &str) -> Vec<String> {
        vec![
            "帮我看一下今日延误航班".to_string(),
            "统计当前各状态航班数量".to_string(),
            "查看 MU2451 的当前保障信息".to_string(),
            "列出近期待起飞的航班".to_string(),
            "有哪些航班存在登机口限制".to_string(),
        ]
    }

    pub async fn list_conversations(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
        status: Option<&str>,
    ) -> Result<NLConversationListDataSchema, NLQueryServiceError> {
        let normalized_user_id = normalize_user_id(user_id)?;
        let normalized_status = status
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty());

        let now = Utc::now();
        cleanup_conversations(&self.state.conversations, now);
        let mut items: Vec<_> = self
            .state
            .conversations
            .iter()
            .filter(|entry| entry.value().user_id == normalized_user_id)
            .filter(|entry| {
                normalized_status
                    .as_ref()
                    .map(|status| entry.value().status == *status)
                    .unwrap_or(true)
            })
            .map(|entry| entry.value().clone())
            .collect();
        items.sort_by_key(|left| std::cmp::Reverse(left.updated_at));

        let total_count = items.len();
        let paged: Vec<_> = items.into_iter().skip(offset).take(limit).collect();
        let has_more = offset + paged.len() < total_count;

        Ok(NLConversationListDataSchema {
            items: paged.into_iter().map(to_conversation_item).collect(),
            total: total_count,
            total_count,
            pagination: PaginationSchema {
                limit,
                offset,
                has_more,
                order: None,
            },
        })
    }

    pub async fn get_conversation_messages(
        &self,
        user_id: &str,
        conversation_id: &str,
        limit: usize,
        offset: usize,
        order: &str,
    ) -> Result<NLConversationMessagesDataSchema, NLQueryServiceError> {
        let normalized_user_id = normalize_user_id(user_id)?;
        let normalized_order = normalize_order(order)?;

        let now = Utc::now();
        cleanup_conversations(&self.state.conversations, now);
        let entry = self
            .state
            .conversations
            .get(conversation_id)
            .ok_or_else(|| NLQueryServiceError::NotFound("Conversation not found".to_string()))?;
        let conversation = entry.value();
        if conversation.user_id != normalized_user_id {
            return Err(NLQueryServiceError::NotFound("Conversation not found".to_string()));
        }

        let mut indexed_messages: Vec<_> = conversation.messages.iter().cloned().enumerate().collect();
        if normalized_order == "desc" {
            indexed_messages.reverse();
        }

        let total_count = indexed_messages.len();
        let paged: Vec<_> = indexed_messages.into_iter().skip(offset).take(limit).collect();
        let has_more = offset + paged.len() < total_count;

        Ok(NLConversationMessagesDataSchema {
            items: paged
                .into_iter()
                .map(|(message_index, message)| NLConversationMessageItemSchema {
                    message_index,
                    role: message.role,
                    content_text: content_to_text(&message.content_raw),
                    content_raw: message.content_raw,
                    name: message.name,
                    tool_calls: message.tool_calls,
                    tool_call_id: message.tool_call_id,
                    metadata: message.metadata,
                })
                .collect(),
            total: total_count,
            total_count,
            pagination: PaginationSchema {
                limit,
                offset,
                has_more,
                order: Some(normalized_order.to_string()),
            },
        })
    }

    pub async fn delete_conversation(&self, user_id: &str, conversation_id: &str) -> Result<bool, NLQueryServiceError> {
        let normalized_user_id = normalize_user_id(user_id)?;
        cleanup_conversations(&self.state.conversations, Utc::now());
        let Some(entry) = self.state.conversations.get(conversation_id) else {
            return Ok(false);
        };
        if entry.value().user_id != normalized_user_id {
            return Ok(false);
        }
        drop(entry);
        self.state.conversations.remove(conversation_id);
        Ok(true)
    }

    async fn append_user_message(
        &self,
        user_id: &str,
        conversation_id: Option<&str>,
        question: &str,
        context: &NLQueryContextSchema,
    ) -> Result<String, NLQueryServiceError> {
        let now = Utc::now();
        cleanup_conversations(&self.state.conversations, now);
        let target_id = match conversation_id.map(str::trim).filter(|value| !value.is_empty()) {
            Some(existing_id) => {
                let Some(mut entry) = self.state.conversations.get_mut(existing_id) else {
                    return Err(NLQueryServiceError::NotFound("Conversation not found".to_string()));
                };
                let conversation = entry.value_mut();
                if conversation.user_id != user_id {
                    return Err(NLQueryServiceError::NotFound("Conversation not found".to_string()));
                }
                conversation.messages.push(ConversationMessage {
                    role: "user".to_string(),
                    content_raw: json!(question),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                    metadata: context_to_metadata(context),
                });
                trim_conversation_messages(&mut conversation.messages);
                conversation.updated_at = now;
                conversation.last_activity_at = now;
                existing_id.to_string()
            }
            None => {
                let conversation_id = format!("conv_{}", ulid::Ulid::new());
                self.state.conversations.insert(
                    conversation_id.clone(),
                    ConversationRecord {
                        conversation_id: conversation_id.clone(),
                        user_id: user_id.to_string(),
                        title: Some(build_title(question)),
                        status: "active".to_string(),
                        model: Some("flight-query-assistant-v1".to_string()),
                        tags: build_tags(context),
                        messages: vec![ConversationMessage {
                            role: "user".to_string(),
                            content_raw: json!(question),
                            name: None,
                            tool_calls: None,
                            tool_call_id: None,
                            metadata: context_to_metadata(context),
                        }],
                        created_at: now,
                        updated_at: now,
                        last_activity_at: now,
                        ended_at: None,
                    },
                );
                enforce_conversation_cap(&self.state.conversations);
                conversation_id
            }
        };
        Ok(target_id)
    }

    async fn append_assistant_message(
        &self,
        conversation_id: &str,
        user_id: &str,
        summary: &str,
        tool_calls: Option<Vec<Value>>,
        metadata: Option<Value>,
    ) -> Result<(), NLQueryServiceError> {
        cleanup_conversations(&self.state.conversations, Utc::now());
        let Some(mut entry) = self.state.conversations.get_mut(conversation_id) else {
            return Err(NLQueryServiceError::NotFound("Conversation not found".to_string()));
        };
        let conversation = entry.value_mut();
        if conversation.user_id != user_id {
            return Err(NLQueryServiceError::NotFound("Conversation not found".to_string()));
        }
        let now = Utc::now();
        conversation.messages.push(ConversationMessage {
            role: "assistant".to_string(),
            content_raw: json!(summary),
            name: None,
            tool_calls,
            tool_call_id: None,
            metadata,
        });
        trim_conversation_messages(&mut conversation.messages);
        conversation.updated_at = now;
        conversation.last_activity_at = now;
        Ok(())
    }

    async fn append_tool_message(
        &self,
        conversation_id: &str,
        user_id: &str,
        runtime_event: &RuntimeQueryEvent,
    ) -> Result<(), NLQueryServiceError> {
        cleanup_conversations(&self.state.conversations, Utc::now());
        let Some(mut entry) = self.state.conversations.get_mut(conversation_id) else {
            return Err(NLQueryServiceError::NotFound("Conversation not found".to_string()));
        };
        let conversation = entry.value_mut();
        if conversation.user_id != user_id {
            return Err(NLQueryServiceError::NotFound("Conversation not found".to_string()));
        }
        let now = Utc::now();
        conversation.messages.push(ConversationMessage {
            role: "tool".to_string(),
            content_raw: sanitize_tool_payload(&runtime_event.result),
            name: Some(runtime_event.tool_name.clone()),
            tool_calls: None,
            tool_call_id: Some(runtime_event.tool_call_id.clone()),
            metadata: Some(json!({
                "execution_id": runtime_event.execution_id,
                "status": runtime_event.status,
            })),
        });
        trim_conversation_messages(&mut conversation.messages);
        conversation.updated_at = now;
        conversation.last_activity_at = now;
        Ok(())
    }

    async fn emit_summary_text_deltas(&self, summary: &str, conversation_id: &str, runtime: &NLQueryRuntimeContext) {
        let chunks = split_summary(summary, 80);
        let chunk_count = chunks.len();
        for (chunk_index, chunk) in chunks.into_iter().enumerate() {
            runtime.emit(
                "text_delta",
                json!({
                    "request_id": runtime.request_id,
                    "scene": runtime.scene,
                    "event": "text_delta",
                    "phase": "answer",
                    "status": "in_progress",
                    "chunk_index": chunk_index,
                    "chunk_count": chunk_count,
                    "conversation_id": conversation_id,
                    "delta": chunk,
                }),
            );
            tokio::task::yield_now().await;
        }
    }
}
