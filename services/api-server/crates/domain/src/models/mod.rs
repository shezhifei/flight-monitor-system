//! 领域模型定义
//!
//! 对应 Python `src/domain/models/` 目录下全部模型。

pub mod ai_context_envelope;
pub mod ai_copilot;
pub mod ai_entity_config;
pub mod ai_execution;
pub mod ai_execution_readiness;
pub mod ai_job;
pub mod ai_media;
pub mod ai_ontology;
pub mod ai_proposal;
pub mod ai_realtime_audio;
pub mod ai_structured_output;
pub mod anomaly;
pub mod business_case;
pub mod business_case_workflow;
pub mod dispatch;
pub mod dispatch_collaboration;
pub mod flight;
pub mod flight_leg;
pub mod flight_state;
pub mod flowable;
pub mod label;
pub mod micro_model;
pub mod mission_type;
pub mod mobile;
pub mod notification;
pub mod online_history;
pub mod ontology_v1;
pub mod ontology_v1_rules;
pub mod operator_identity;
pub mod permission_template;
pub mod session_runtime;
pub mod shift_handover;
pub mod todo;
pub mod tool_authorization;
pub mod tool_governance;
pub mod user;
pub mod value_objects;
pub mod workflow_form;
