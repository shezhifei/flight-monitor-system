use std::collections::HashMap;

use serde_json::{json, Value};

use crate::schemas::nl_query_schemas::NLQueryContextSchema;
use crate::services::ai_runtime_service::AiToolExecutionSpec;

use super::helpers::{
    attach_runtime_metadata, build_assistant_metadata, build_runtime_result_preview, contains_any,
    extract_flight_number, fallback_text, flight_label, schedule_suffix, status_matches,
};
use super::service::NLQueryService;
use super::types::{NLQueryRuntimeContext, NLQueryServiceError, QueryAnalysis, RuntimeQueryEvent};

impl NLQueryService {
    pub(super) async fn analyze_question(
        &self,
        question: &str,
        context: &NLQueryContextSchema,
        user_id: &str,
        conversation_id: &str,
        runtime: &NLQueryRuntimeContext,
    ) -> Result<QueryAnalysis, NLQueryServiceError> {
        let question_lower = question.to_lowercase();

        if let Some(flight_id) = context
            .selected_flight_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return self
                .analyze_specific_flight(question, Some(flight_id), None, user_id, conversation_id, runtime)
                .await;
        }

        if let Some(flight_no) = context
            .selected_flight_no
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return self
                .analyze_specific_flight(question, None, Some(flight_no), user_id, conversation_id, runtime)
                .await;
        }

        if let Some(flight_no) = extract_flight_number(question) {
            return self
                .analyze_specific_flight(question, None, Some(&flight_no), user_id, conversation_id, runtime)
                .await;
        }

        if contains_any(&question_lower, &["多少", "统计", "count", "总数", "数量"]) {
            return self
                .analyze_status_summary(question, user_id, conversation_id, runtime)
                .await;
        }

        if contains_any(&question_lower, &["延误", "delay", "delayed"]) {
            return self
                .analyze_delayed_flights(question, user_id, conversation_id, runtime)
                .await;
        }

        if contains_any(&question_lower, &["起飞", "出港", "departure"]) {
            return self
                .analyze_upcoming_flights("departure", question, user_id, conversation_id, runtime)
                .await;
        }

        if contains_any(&question_lower, &["到达", "进港", "arrival"]) {
            return self
                .analyze_upcoming_flights("arrival", question, user_id, conversation_id, runtime)
                .await;
        }

        self.analyze_default_overview(question, user_id, conversation_id, runtime)
            .await
    }

    async fn analyze_specific_flight(
        &self,
        question: &str,
        flight_id: Option<&str>,
        flight_no: Option<&str>,
        user_id: &str,
        conversation_id: &str,
        runtime: &NLQueryRuntimeContext,
    ) -> Result<QueryAnalysis, NLQueryServiceError> {
        let flight = if let Some(flight_id) = flight_id {
            self.flight_service
                .get_flight(flight_id)
                .await
                .map_err(|error| NLQueryServiceError::Internal(error.to_string()))?
        } else if let Some(flight_no) = flight_no {
            self.flight_service
                .search_flights(Some(flight_no), None, None, None, None, 1, 20)
                .await
                .map_err(|error| NLQueryServiceError::Internal(error.to_string()))?
                .into_iter()
                .next()
        } else {
            None
        };
        let runtime_event = self
            .record_runtime_query(
                "get_flight_detail",
                user_id,
                conversation_id,
                runtime,
                "detail",
                "flights",
                json!({
                    "question": question,
                    "flight_id": flight_id,
                    "flight_no": flight_no,
                }),
            )
            .await;

        match flight {
            Some(flight) => {
                let summary = format!(
                    "已定位航班 {}。当前状态 {}，机位 {}，登机口 {}{}。",
                    fallback_text(flight.flight_number.as_deref(), "未知航班"),
                    fallback_text(flight.status.as_deref(), "未知"),
                    fallback_text(flight.stand.as_deref(), "未分配"),
                    fallback_text(flight.gate.as_deref(), "未分配"),
                    schedule_suffix(&flight),
                );
                let structured_data = attach_runtime_metadata(
                    json!({
                        "kind": "flight_detail",
                        "item": flight,
                    }),
                    runtime_event.as_ref(),
                );
                Ok(QueryAnalysis {
                    interpretation: "识别为单航班运行态势查询".to_string(),
                    structured_data,
                    visualization_hint: Some("detail".to_string()),
                    summary,
                    tool_calls: runtime_event.as_ref().map(RuntimeQueryEvent::assistant_tool_calls),
                    metadata: Some(build_assistant_metadata(
                        "识别为单航班运行态势查询",
                        Some("detail"),
                        runtime_event.as_ref(),
                    )),
                    runtime_event,
                })
            }
            None => Ok(QueryAnalysis {
                interpretation: "识别为单航班运行态势查询".to_string(),
                structured_data: attach_runtime_metadata(
                    json!({
                        "kind": "flight_detail",
                        "item": Value::Null,
                    }),
                    runtime_event.as_ref(),
                ),
                visualization_hint: Some("detail".to_string()),
                summary: format!("暂未找到与“{question}”对应的航班，可尝试提供更完整的航班号或直接选择航班。"),
                tool_calls: runtime_event.as_ref().map(RuntimeQueryEvent::assistant_tool_calls),
                metadata: Some(build_assistant_metadata(
                    "识别为单航班运行态势查询",
                    Some("detail"),
                    runtime_event.as_ref(),
                )),
                runtime_event,
            }),
        }
    }

    async fn analyze_status_summary(
        &self,
        question: &str,
        user_id: &str,
        conversation_id: &str,
        runtime: &NLQueryRuntimeContext,
    ) -> Result<QueryAnalysis, NLQueryServiceError> {
        let response = self
            .flight_service
            .list_flights(1, 200, None)
            .await
            .map_err(|error| NLQueryServiceError::Internal(error.to_string()))?;
        let runtime_event = self
            .record_runtime_query(
                "get_flight_status_summary",
                user_id,
                conversation_id,
                runtime,
                "aggregate",
                "flights",
                json!({
                    "question": question,
                    "group_by": "status",
                    "limit": 200,
                }),
            )
            .await;

        let mut counters: HashMap<String, usize> = HashMap::new();
        for item in &response.items {
            let key = item
                .status
                .as_deref()
                .map(|value| value.trim().to_uppercase())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "UNKNOWN".to_string());
            *counters.entry(key).or_insert(0) += 1;
        }

        let mut status_items: Vec<_> = counters.into_iter().collect();
        status_items.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

        let summary = if status_items.is_empty() {
            "当前没有可用于统计的航班数据。".to_string()
        } else {
            let preview = status_items
                .iter()
                .take(5)
                .map(|(status, count)| format!("{status} {count} 架"))
                .collect::<Vec<_>>()
                .join("，");
            format!(
                "已汇总当前航班状态分布，共 {} 架航班；其中 {}。",
                response.total, preview
            )
        };

        Ok(QueryAnalysis {
            interpretation: "识别为航班状态统计查询".to_string(),
            structured_data: attach_runtime_metadata(
                json!({
                    "kind": "flight_status_summary",
                    "items": status_items
                        .into_iter()
                        .map(|(status, count)| json!({"status": status, "count": count}))
                        .collect::<Vec<_>>(),
                    "total": response.total,
                }),
                runtime_event.as_ref(),
            ),
            visualization_hint: Some("bar_chart".to_string()),
            summary,
            tool_calls: runtime_event.as_ref().map(RuntimeQueryEvent::assistant_tool_calls),
            metadata: Some(build_assistant_metadata(
                "识别为航班状态统计查询",
                Some("bar_chart"),
                runtime_event.as_ref(),
            )),
            runtime_event,
        })
    }

    async fn analyze_delayed_flights(
        &self,
        question: &str,
        user_id: &str,
        conversation_id: &str,
        runtime: &NLQueryRuntimeContext,
    ) -> Result<QueryAnalysis, NLQueryServiceError> {
        let response = self
            .flight_service
            .list_flights(1, 200, None)
            .await
            .map_err(|error| NLQueryServiceError::Internal(error.to_string()))?;
        let runtime_event = self
            .record_runtime_query(
                "get_delayed_flights",
                user_id,
                conversation_id,
                runtime,
                "search",
                "flights",
                json!({
                    "question": question,
                    "status": ["DELAY", "DELAYED", "LATE"],
                    "limit": 20,
                }),
            )
            .await;

        let delayed = response
            .items
            .into_iter()
            .filter(|flight| status_matches(flight, &["DELAY", "DELAYED", "LATE"]))
            .take(20)
            .collect::<Vec<_>>();

        let summary = if delayed.is_empty() {
            "当前未检索到明确标记为延误的航班。".to_string()
        } else {
            let preview = delayed.iter().take(5).map(flight_label).collect::<Vec<_>>().join("，");
            format!("当前共识别到 {} 架延误航班，优先关注：{}。", delayed.len(), preview)
        };

        Ok(QueryAnalysis {
            interpretation: "识别为延误航班筛选查询".to_string(),
            structured_data: attach_runtime_metadata(
                json!({
                    "kind": "flight_list",
                    "items": delayed,
                    "total": delayed.len(),
                    "filter": "delayed",
                }),
                runtime_event.as_ref(),
            ),
            visualization_hint: Some("table".to_string()),
            summary,
            tool_calls: runtime_event.as_ref().map(RuntimeQueryEvent::assistant_tool_calls),
            metadata: Some(build_assistant_metadata(
                "识别为延误航班筛选查询",
                Some("table"),
                runtime_event.as_ref(),
            )),
            runtime_event,
        })
    }

    async fn analyze_upcoming_flights(
        &self,
        direction: &str,
        question: &str,
        user_id: &str,
        conversation_id: &str,
        runtime: &NLQueryRuntimeContext,
    ) -> Result<QueryAnalysis, NLQueryServiceError> {
        let response = self
            .flight_service
            .list_flights(1, 50, None)
            .await
            .map_err(|error| NLQueryServiceError::Internal(error.to_string()))?;
        let runtime_event = self
            .record_runtime_query(
                if direction == "arrival" {
                    "get_arrival_flights"
                } else {
                    "get_departure_flights"
                },
                user_id,
                conversation_id,
                runtime,
                "search",
                "flights",
                json!({
                    "question": question,
                    "direction": direction,
                    "limit": 10,
                }),
            )
            .await;
        let items = response.items.into_iter().take(10).collect::<Vec<_>>();
        let label = if direction == "arrival" { "到达" } else { "起飞" };
        let summary = if items.is_empty() {
            format!("当前没有可展示的{label}航班。")
        } else {
            let preview = items.iter().take(5).map(flight_label).collect::<Vec<_>>().join("，");
            format!(
                "已整理近期{label}航班，共返回 {} 条；可优先查看：{}。",
                items.len(),
                preview
            )
        };
        Ok(QueryAnalysis {
            interpretation: format!("识别为{label}航班列表查询"),
            structured_data: attach_runtime_metadata(
                json!({
                    "kind": "flight_list",
                    "items": items,
                    "total": items.len(),
                    "direction": direction,
                }),
                runtime_event.as_ref(),
            ),
            visualization_hint: Some("table".to_string()),
            summary,
            tool_calls: runtime_event.as_ref().map(RuntimeQueryEvent::assistant_tool_calls),
            metadata: Some(build_assistant_metadata(
                &format!("识别为{label}航班列表查询"),
                Some("table"),
                runtime_event.as_ref(),
            )),
            runtime_event,
        })
    }

    async fn analyze_default_overview(
        &self,
        question: &str,
        user_id: &str,
        conversation_id: &str,
        runtime: &NLQueryRuntimeContext,
    ) -> Result<QueryAnalysis, NLQueryServiceError> {
        let response = self
            .flight_service
            .list_flights(1, 20, None)
            .await
            .map_err(|error| NLQueryServiceError::Internal(error.to_string()))?;
        let runtime_event = self
            .record_runtime_query(
                "get_flight_overview",
                user_id,
                conversation_id,
                runtime,
                "search",
                "flights",
                json!({
                    "question": question,
                    "limit": 10,
                }),
            )
            .await;
        let items = response.items.into_iter().take(10).collect::<Vec<_>>();
        let summary = if items.is_empty() {
            "当前没有可返回的航班数据。".to_string()
        } else {
            let preview = items.iter().take(5).map(flight_label).collect::<Vec<_>>().join("，");
            format!(
                "结合“{question}”，先给出当前航班运行概览，共 {} 条；可从这些航班继续追问：{}。",
                items.len(),
                preview
            )
        };
        Ok(QueryAnalysis {
            interpretation: "识别为通用航班运行概览查询".to_string(),
            structured_data: attach_runtime_metadata(
                json!({
                    "kind": "flight_list",
                    "items": items,
                    "total": items.len(),
                }),
                runtime_event.as_ref(),
            ),
            visualization_hint: Some("table".to_string()),
            summary,
            tool_calls: runtime_event.as_ref().map(RuntimeQueryEvent::assistant_tool_calls),
            metadata: Some(build_assistant_metadata(
                "识别为通用航班运行概览查询",
                Some("table"),
                runtime_event.as_ref(),
            )),
            runtime_event,
        })
    }

    async fn record_runtime_query(
        &self,
        tool_name: &str,
        user_id: &str,
        conversation_id: &str,
        runtime: &NLQueryRuntimeContext,
        route_intent: &str,
        dataset: &str,
        arguments: Value,
    ) -> Option<RuntimeQueryEvent> {
        runtime.emit(
            "tool_call",
            json!({
                "request_id": runtime.request_id,
                "scene": runtime.scene,
                "event": "tool_call",
                "tool_name": tool_name,
                "conversation_id": conversation_id,
                "arguments": arguments.clone(),
                "status": "in_progress",
            }),
        );
        let payload = self
            .runtime_service
            .execute_tool(
                AiToolExecutionSpec {
                    tool_name: tool_name.to_string(),
                    category: "query".to_string(),
                    operation_level: "l0_read".to_string(),
                    side_effect: false,
                    query_intent: Some(route_intent.to_string()),
                    query_dataset: Some(dataset.to_string()),
                },
                arguments.clone(),
                Some(user_id.to_string()),
                Vec::new(),
            )
            .await;

        let result = payload
            .get("data")
            .and_then(|item| item.get("result"))
            .cloned()
            .unwrap_or(Value::Null);
        let result_data = payload.get("result_data").unwrap_or(&Value::Null);
        let status = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("success")
            .to_string();
        let execution_id = result_data
            .get("execution_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let tool_call_id = result
            .get("tool_call_id")
            .and_then(Value::as_str)
            .or_else(|| result_data.get("tool_call_id").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string();
        let duration_ms = result_data.get("duration_ms").and_then(Value::as_i64).or_else(|| {
            result
                .get("execution_receipt")
                .and_then(|item| item.get("duration_ms"))
                .and_then(Value::as_i64)
        });

        let mut mismatch = status != "success";
        let mut mismatch_reason = if mismatch {
            "execution_status_not_success".to_string()
        } else {
            "none".to_string()
        };

        if execution_id.is_empty() || tool_call_id.is_empty() {
            mismatch = true;
            mismatch_reason = "missing_runtime_metadata".to_string();
            self.runtime_service
                .record_query_tool_selection(&status, mismatch, tool_name, &mismatch_reason)
                .await;
            return None;
        }

        self.runtime_service
            .record_query_tool_selection(&status, mismatch, tool_name, &mismatch_reason)
            .await;

        let event = RuntimeQueryEvent {
            execution_id,
            tool_call_id,
            tool_name: tool_name.to_string(),
            arguments,
            result,
            status,
            duration_ms,
        };

        runtime.emit(
            "tool_result",
            json!({
                "request_id": runtime.request_id,
                "scene": runtime.scene,
                "event": "tool_result",
                "tool_name": event.tool_name.clone(),
                "tool_call_id": event.tool_call_id.clone(),
                "execution_id": event.execution_id.clone(),
                "conversation_id": conversation_id,
                "status": event.status.clone(),
                "duration_ms": event.duration_ms,
                "result": event.result.clone(),
                "result_preview": build_runtime_result_preview(&event.result),
            }),
        );

        Some(event)
    }
}
