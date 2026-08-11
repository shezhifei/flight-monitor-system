//! 微模型统一执行器 (dispatcher)
//!
//! 将 API 路由层的 model_id dispatch 抽离到 application service 层，
//! 每个模型分支执行 typed input 反序列化 → deterministic heuristic → typed output。

use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;

use fms_domain::models::micro_model::MicroModelRegistry;

use crate::schemas::micro_model_schemas::{
    AnomalyTriageInput, AnomalyTriageOutput, ConfidenceLevel, DispatchReplanInput, FlightRiskInput,
    MicroModelConfidence, OpsBriefingInput, OpsBriefingKeyEvent, OpsBriefingOutput, StandConflictInput,
    StandConflictOutput,
};
use crate::services::flight_risk_micro_model::FlightRiskMicroModel;

/// Result of a micro-model execution.
#[derive(Debug, Clone)]
pub struct MicroModelExecuteResult {
    pub model_version: String,
    pub output: Value,
    pub execution_time_ms: u64,
    pub proposal_candidates: Vec<Value>,
}

pub struct MicroModelExecutor {
    registry: Arc<MicroModelRegistry>,
}

impl MicroModelExecutor {
    pub fn new(registry: Arc<MicroModelRegistry>) -> Self {
        Self { registry }
    }

    /// Execute a micro-model by model_id with the given input.
    /// Returns typed output, execution time, and advisory proposal candidates.
    pub fn execute(&self, model_id: &str, input: &Value) -> Result<MicroModelExecuteResult, String> {
        let spec = self
            .registry
            .get(model_id)
            .ok_or_else(|| format!("model not found: {}", model_id))?;
        let model_version = spec.version.clone();

        match model_id {
            "flight_risk_v1" => self.execute_flight_risk(input, &model_version),
            "dispatch_replan_v1" => self.execute_dispatch_replan(input, &model_version),
            "stand_conflict_v1" => self.execute_stand_conflict(input, &model_version),
            "anomaly_triage_v1" => self.execute_anomaly_triage(input, &model_version),
            "ops_briefing_v1" => self.execute_ops_briefing(input, &model_version),
            _ => Err(format!("model {} has no executor implementation", model_id)),
        }
    }

    fn execute_flight_risk(&self, input: &Value, model_version: &str) -> Result<MicroModelExecuteResult, String> {
        let typed_input: FlightRiskInput =
            serde_json::from_value(input.clone()).map_err(|e| format!("invalid flight_risk_v1 input: {}", e))?;

        let model = FlightRiskMicroModel::new();
        let output = model.execute(&typed_input);

        let candidates: Vec<Value> = output
            .proposals
            .iter()
            .map(|p| {
                json!({
                    "object_type": p.object_type,
                    "object_id": p.object_id,
                    "action_name": p.action_name,
                    "arguments": p.arguments,
                    "rationale": p.rationale,
                    "priority": p.priority,
                    "risk_level": p.risk_if_not_acted,
                })
            })
            .collect();

        Ok(MicroModelExecuteResult {
            model_version: model_version.to_string(),
            output: serde_json::to_value(&output).unwrap_or(Value::Null),
            execution_time_ms: output.execution_time_ms,
            proposal_candidates: candidates,
        })
    }

    fn execute_dispatch_replan(&self, input: &Value, model_version: &str) -> Result<MicroModelExecuteResult, String> {
        let typed_input: DispatchReplanInput =
            serde_json::from_value(input.clone()).map_err(|e| format!("invalid dispatch_replan_v1 input: {}", e))?;

        let start = Instant::now();

        // Deterministic heuristic: compute based on input fields
        let order_count = typed_input.dispatch_order_ids.len();
        let has_orders = order_count > 0;
        let objective_label = typed_input.optimization_objective.label();
        let max_proposals = typed_input.max_proposals.min(10);

        let replan_recommended = has_orders || objective_label == "minimize_delay";
        let severity = if order_count >= 5 {
            "high"
        } else if order_count >= 2 {
            "medium"
        } else {
            "low"
        };
        let optimization_score: f64 = if replan_recommended {
            0.72 + (order_count as f64 * 0.03).min(0.2)
        } else {
            0.0
        };

        let mut candidates = Vec::new();
        if replan_recommended {
            for (i, order_id) in typed_input.dispatch_order_ids.iter().take(max_proposals).enumerate() {
                candidates.push(json!({
                    "object_type": "DispatchOrder",
                    "object_id": order_id,
                    "action_name": "recommend_replan",
                    "arguments": {
                        "reason": format!("Operational {} replan for {} optimization", severity, objective_label),
                        "strategy": objective_label,
                        "priority_rank": i + 1,
                    },
                    "confidence": optimization_score,
                    "reasoning": format!("Order {} affected by delay pattern, {} optimization applies", order_id, objective_label),
                    "risk_level": severity,
                }));
            }
        }

        let execution_time_ms = start.elapsed().as_millis() as u64;

        let output = json!({
            "model_id": "dispatch_replan_v1",
            "model_version": model_version,
            "shift_id": typed_input.shift_id,
            "replan_recommended": replan_recommended,
            "severity": severity,
            "optimization_objective": objective_label,
            "optimization_score": optimization_score,
            "affected_orders": order_count,
            "execution_time_ms": execution_time_ms,
        });

        Ok(MicroModelExecuteResult {
            model_version: model_version.to_string(),
            output,
            execution_time_ms,
            proposal_candidates: candidates,
        })
    }

    fn execute_stand_conflict(&self, input: &Value, model_version: &str) -> Result<MicroModelExecuteResult, String> {
        let typed_input: StandConflictInput =
            serde_json::from_value(input.clone()).map_err(|e| format!("invalid stand_conflict_v1 input: {}", e))?;

        let start = Instant::now();

        // Deterministic heuristic: conflict detected if conflict_flight_id is provided
        // and window is narrow enough
        let conflict_detected = typed_input.conflict_flight_id.is_some() && typed_input.conflict_window_minutes < 60;

        let recommended_stand = if conflict_detected {
            // Simple heuristic: suggest alternative stand based on current stand
            let alt = format!("{}A", typed_input.current_stand_id);
            Some(alt)
        } else {
            None
        };

        let conflict_details = if conflict_detected {
            format!(
                "Stand {} has time overlap conflict within {} minute window with flight {}",
                typed_input.current_stand_id,
                typed_input.conflict_window_minutes,
                typed_input.conflict_flight_id.as_deref().unwrap_or("unknown"),
            )
        } else {
            format!("No conflict detected at stand {}", typed_input.current_stand_id)
        };

        let confidence_score = if conflict_detected { 0.85 } else { 0.95 };

        let mut output = StandConflictOutput::new();
        output.conflict_detected = conflict_detected;
        output.recommended_stand = recommended_stand.clone();
        output.conflict_details = conflict_details;
        output.confidence = MicroModelConfidence {
            score: confidence_score,
            level: ConfidenceLevel::from_score(confidence_score),
            reasons: if conflict_detected {
                vec!["deterministic conflict window analysis".to_string()]
            } else {
                vec!["no overlapping bookings detected".to_string()]
            },
        };
        output.execution_time_ms = start.elapsed().as_millis() as u64;

        let mut candidates = Vec::new();
        if conflict_detected {
            candidates.push(json!({
                "object_type": "Flight",
                "object_id": typed_input.flight_id,
                "action_name": "change_stand",
                "arguments": {
                    "new_stand_id": recommended_stand.as_deref().unwrap_or("unknown"),
                },
                "confidence": confidence_score,
                "reasoning": format!(
                    "Original stand {} has time overlap conflict with {}",
                    typed_input.current_stand_id,
                    typed_input.conflict_flight_id.as_deref().unwrap_or("unknown"),
                ),
                "risk_level": "medium",
            }));
        }

        let execution_time_ms = output.execution_time_ms;

        Ok(MicroModelExecuteResult {
            model_version: model_version.to_string(),
            output: serde_json::to_value(&output).unwrap_or(Value::Null),
            execution_time_ms,
            proposal_candidates: candidates,
        })
    }

    fn execute_anomaly_triage(&self, input: &Value, model_version: &str) -> Result<MicroModelExecuteResult, String> {
        let typed_input: AnomalyTriageInput =
            serde_json::from_value(input.clone()).map_err(|e| format!("invalid anomaly_triage_v1 input: {}", e))?;

        let start = Instant::now();

        // Deterministic heuristic based on severity and duration
        let severity_normalized = typed_input.severity.trim().to_ascii_lowercase();
        let duration = typed_input.duration_minutes.unwrap_or(0);

        let (should_escalate, assigned_tier, recommended_action) = match severity_normalized.as_str() {
            "critical" => (true, "supervisor", "escalate"),
            "high" => {
                if duration > 15 {
                    (true, "supervisor", "escalate")
                } else {
                    (true, "team_lead", "notify")
                }
            }
            "medium" => {
                if duration > 30 {
                    (true, "team_lead", "notify")
                } else {
                    (false, "operator", "acknowledge")
                }
            }
            _ => (false, "operator", "monitor"),
        };

        let confidence_score = match severity_normalized.as_str() {
            "critical" | "high" => 0.92,
            "medium" => 0.78,
            _ => 0.65,
        };

        let reasoning = format!(
            "Anomaly {} with severity={} duration={}m: {} to {}",
            typed_input.anomaly_id, typed_input.severity, duration, recommended_action, assigned_tier,
        );

        let mut output = AnomalyTriageOutput::new();
        output.should_escalate = should_escalate;
        output.assigned_tier = assigned_tier.to_string();
        output.recommended_action = recommended_action.to_string();
        output.reasoning = reasoning.clone();
        output.confidence = MicroModelConfidence {
            score: confidence_score,
            level: ConfidenceLevel::from_score(confidence_score),
            reasons: vec![format!("severity={}, duration={}m", typed_input.severity, duration)],
        };
        output.execution_time_ms = start.elapsed().as_millis() as u64;

        let mut candidates = Vec::new();
        if should_escalate {
            candidates.push(json!({
                "object_type": "Anomaly",
                "object_id": typed_input.anomaly_id,
                "action_name": recommended_action,
                "arguments": {
                    "reason": reasoning,
                    "assigned_tier": assigned_tier,
                },
                "confidence": confidence_score,
                "reasoning": reasoning,
                "risk_level": typed_input.severity,
            }));
        }

        let execution_time_ms = output.execution_time_ms;

        Ok(MicroModelExecuteResult {
            model_version: model_version.to_string(),
            output: serde_json::to_value(&output).unwrap_or(Value::Null),
            execution_time_ms,
            proposal_candidates: candidates,
        })
    }

    fn execute_ops_briefing(&self, input: &Value, model_version: &str) -> Result<MicroModelExecuteResult, String> {
        let typed_input: OpsBriefingInput =
            serde_json::from_value(input.clone()).map_err(|e| format!("invalid ops_briefing_v1 input: {}", e))?;

        let start = Instant::now();

        // Deterministic heuristic based on input parameters
        let flight_count = typed_input.include_flight_ids.len();
        let has_focus = !typed_input.focus_areas.is_empty();

        let briefing = format!(
            "班组 {} 运行复盘：共涉及 {} 个航班{}。",
            typed_input.shift_id,
            if flight_count > 0 {
                flight_count.to_string()
            } else {
                "未指定".to_string()
            },
            if has_focus {
                format!("，重点关注：{}", typed_input.focus_areas.join("、"))
            } else {
                "".to_string()
            },
        );

        let mut key_events = Vec::new();
        for (i, flight_id) in typed_input.include_flight_ids.iter().take(5).enumerate() {
            key_events.push(OpsBriefingKeyEvent {
                event_type: "flight_turnaround".to_string(),
                description: format!("航班 {} 保障时序记录", flight_id),
                severity: if i == 0 {
                    "medium".to_string()
                } else {
                    "low".to_string()
                },
                related_object_id: Some(flight_id.clone()),
            });
        }

        let mut recommendations = Vec::new();
        if flight_count > 3 {
            recommendations.push("建议复核大批量航班的资源分配合理性".to_string());
        }
        if has_focus {
            for area in &typed_input.focus_areas {
                recommendations.push(format!("建议持续跟踪关注领域: {}", area));
            }
        }
        recommendations.push("建议在下一班组交接时同步本次复盘要点".to_string());

        let confidence_score = if flight_count > 0 { 0.82 } else { 0.6 };

        let mut output = OpsBriefingOutput::new(&typed_input.shift_id);
        output.briefing = briefing;
        output.key_events = key_events;
        output.recommendations = recommendations.clone();
        output.confidence = MicroModelConfidence {
            score: confidence_score,
            level: ConfidenceLevel::from_score(confidence_score),
            reasons: vec![format!(
                "flight_count={}, focus_areas={}",
                flight_count,
                typed_input.focus_areas.len()
            )],
        };
        output.execution_time_ms = start.elapsed().as_millis() as u64;

        // Generate a Todo.create candidate for the briefing follow-up
        let mut candidates = Vec::new();
        if !recommendations.is_empty() {
            candidates.push(json!({
                "object_type": "Todo",
                "object_id": format!("todo_briefing_{}", typed_input.shift_id),
                "action_name": "create",
                "arguments": {
                    "title": format!("复盘跟进: 班组 {} 运行要点", typed_input.shift_id),
                    "description": recommendations.first().cloned().unwrap_or_default(),
                },
                "confidence": confidence_score,
                "reasoning": format!("班组 {} 复盘生成了 {} 条建议", typed_input.shift_id, recommendations.len()),
                "risk_level": "low",
            }));
        }

        let execution_time_ms = output.execution_time_ms;

        Ok(MicroModelExecuteResult {
            model_version: model_version.to_string(),
            output: serde_json::to_value(&output).unwrap_or(Value::Null),
            execution_time_ms,
            proposal_candidates: candidates,
        })
    }
}
