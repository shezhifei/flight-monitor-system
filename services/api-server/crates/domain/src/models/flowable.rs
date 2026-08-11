//! Flowable BPM 领域模型
//!
//! 对应 Python `src/domain/models/flowable_models.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 流程状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcessStatus {
    Active,
    Completed,
    Terminated,
    Suspended,
    Incident,
}

/// 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowableTaskStatus {
    Created,
    Completed,
    Canceled,
    Failed,
}

/// 流程定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessDefinition {
    pub process_definition_id: String,
    pub process_definition_key: String,
    pub process_definition_name: String,
    pub version: i32,
    pub resource_name: String,
    pub deployment_id: String,
    pub tenant_id: Option<String>,
    pub description: Option<String>,
}

/// 流程实例
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInstance {
    pub process_instance_id: String,
    pub process_definition_id: String,
    pub process_definition_key: String,
    pub business_key: Option<String>,
    #[serde(default = "default_active")]
    pub status: ProcessStatus,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub variables: Option<HashMap<String, serde_json::Value>>,
    pub tenant_id: Option<String>,
}

fn default_active() -> ProcessStatus {
    ProcessStatus::Active
}

impl ProcessInstance {
    pub fn is_active(&self) -> bool {
        self.status == ProcessStatus::Active
    }
    pub fn is_completed(&self) -> bool {
        self.status == ProcessStatus::Completed
    }
    pub fn duration_secs(&self) -> Option<f64> {
        match (self.start_time, self.end_time) {
            (Some(s), Some(e)) => Some((e - s).num_milliseconds() as f64 / 1000.0),
            _ => None,
        }
    }
}

/// 流程任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowableTask {
    pub task_id: String,
    pub task_definition_key: String,
    pub process_instance_id: String,
    pub process_definition_id: String,
    pub task_name: String,
    #[serde(default = "default_created")]
    pub status: FlowableTaskStatus,
    pub assignee: Option<String>,
    #[serde(default)]
    pub candidate_groups: Vec<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub follow_up_date: Option<DateTime<Utc>>,
    #[serde(default = "default_priority")]
    pub priority: i32,
    pub variables: Option<HashMap<String, serde_json::Value>>,
    pub created_time: Option<DateTime<Utc>>,
    pub completed_time: Option<DateTime<Utc>>,
}

fn default_created() -> FlowableTaskStatus {
    FlowableTaskStatus::Created
}
fn default_priority() -> i32 {
    50
}

impl FlowableTask {
    pub fn is_completed(&self) -> bool {
        self.status == FlowableTaskStatus::Completed
    }
    pub fn is_overdue(&self) -> bool {
        match (self.due_date, self.completed_time) {
            (Some(d), None) => Utc::now() > d,
            _ => false,
        }
    }
}

/// 部署结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentResult {
    pub deployment_id: String,
    pub deployment_name: String,
    pub deployed_process_definitions: Vec<ProcessDefinition>,
    pub deployed_decision_definitions: Option<Vec<serde_json::Value>>,
    pub tenant_id: Option<String>,
    pub deployment_time: Option<DateTime<Utc>>,
}

/// 流程变量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessVariable {
    pub name: String,
    pub value: serde_json::Value,
    #[serde(rename = "type")]
    pub var_type: String,
    pub value_info: Option<HashMap<String, serde_json::Value>>,
}

/// 子流程定义引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubProcessDefinition {
    pub called_element: String,
    pub source_activity_id: String,
    pub in_variable_mappings: Option<HashMap<String, String>>,
    pub out_variable_mappings: Option<HashMap<String, String>>,
    #[serde(default)]
    pub inherit_variables: bool,
}

/// 子流程执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubProcessResult {
    pub subprocess_instance_id: String,
    pub parent_process_instance_id: String,
    pub called_element: String,
    #[serde(default = "default_completed")]
    pub status: ProcessStatus,
    pub output_variables: Option<HashMap<String, serde_json::Value>>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
}

fn default_completed() -> ProcessStatus {
    ProcessStatus::Completed
}
