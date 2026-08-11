//! 事件驱动的派工规则 Repository 接口
//!
//! 提供调整规则和生成规则的持久化接口定义。

use crate::error::DomainError;
use async_trait::async_trait;

#[async_trait]
pub trait EventRuleRepository {
    async fn list_adjustment_rules(
        &self,
        params: &ListAdjustmentRulesParams,
    ) -> Result<Vec<AdjustmentRuleRecord>, DomainError>;

    async fn count_adjustment_rules(&self, params: &ListAdjustmentRulesParams) -> Result<i64, DomainError>;

    async fn get_adjustment_rule(&self, id: &str) -> Result<Option<AdjustmentRuleRecord>, DomainError>;

    async fn create_adjustment_rule(
        &self,
        payload: DispatchOrderAdjustmentRuleCreate,
        created_by: Option<&str>,
    ) -> Result<AdjustmentRuleRecord, DomainError>;

    async fn update_adjustment_rule(
        &self,
        id: &str,
        payload: DispatchOrderAdjustmentRuleUpdate,
    ) -> Result<AdjustmentRuleRecord, DomainError>;

    async fn delete_adjustment_rule(&self, id: &str) -> Result<(), DomainError>;

    async fn set_adjustment_rule_enabled(&self, id: &str, enabled: bool) -> Result<AdjustmentRuleRecord, DomainError>;

    async fn list_generation_rules(
        &self,
        params: &ListGenerationRulesParams,
    ) -> Result<Vec<GenerationRuleRecord>, DomainError>;

    async fn count_generation_rules(&self, params: &ListGenerationRulesParams) -> Result<i64, DomainError>;

    async fn get_generation_rule(&self, id: &str) -> Result<Option<GenerationRuleRecord>, DomainError>;

    async fn create_generation_rule(
        &self,
        payload: EventDrivenGenerationRuleCreate,
        created_by: Option<&str>,
    ) -> Result<GenerationRuleRecord, DomainError>;

    async fn update_generation_rule(
        &self,
        id: &str,
        payload: EventDrivenGenerationRuleUpdate,
    ) -> Result<GenerationRuleRecord, DomainError>;

    async fn delete_generation_rule(&self, id: &str) -> Result<(), DomainError>;

    async fn set_generation_rule_enabled(&self, id: &str, enabled: bool) -> Result<GenerationRuleRecord, DomainError>;
}

#[derive(Debug, Clone, Default)]
pub struct ListAdjustmentRulesParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub is_enabled: Option<bool>,
    pub department_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ListGenerationRulesParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub is_enabled: Option<bool>,
    pub department_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdjustmentRuleRecord {
    pub id: String,
    pub adjuster_type: String,
    pub name: String,
    pub description: Option<String>,
    pub event_patterns: Vec<String>,
    pub priority: i32,
    pub conditions: Option<serde_json::Value>,
    pub config: serde_json::Value,
    pub is_enabled: bool,
    pub department_id: Option<String>,
    pub department_name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GenerationRuleRecord {
    pub id: String,
    pub generator_type: String,
    pub name: String,
    pub description: Option<String>,
    pub event_patterns: Vec<String>,
    pub priority: i32,
    pub conditions: Option<serde_json::Value>,
    pub config: serde_json::Value,
    pub is_enabled: bool,
    pub department_id: Option<String>,
    pub department_name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<String>,
}

// ---------------------------------------------------------------------------
// Schemas (moved from application layer to break cyclic dependency)
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operator", rename_all = "UPPERCASE")]
pub enum ConditionOperator {
    AND { children: Vec<ConditionItem> },
    OR { children: Vec<ConditionItem> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionItem {
    pub field: String,
    pub op: String,
    #[serde(default)]
    pub value: serde_json::Value,
}

impl Default for ConditionOperator {
    fn default() -> Self {
        ConditionOperator::AND { children: vec![] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentActionType {
    AddCrewSlot,
    IncreaseCrewCount,
    UpgradeCrewLevel,
    AddEquipmentSlot,
    IncreaseEquipmentCount,
    ExtendDuration,
    ShortenDuration,
    AdvancePublish,
    DelayPublish,
    RequireDriverForEquipment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCrewRequirement {
    pub slot_code: String,
    pub qualification_code: String,
    pub required_count: i32,
    #[serde(default)]
    pub min_level_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationRuleConfig {
    pub task_type: String,
    #[serde(default)]
    pub duration_minutes_from: Option<String>,
    #[serde(default)]
    pub fixed_duration_minutes: Option<i32>,
    #[serde(default)]
    pub crew_requirements: Vec<CreateCrewRequirement>,
    #[serde(default)]
    pub equipment_requirements: Vec<CreateCrewRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOrderAdjustmentRuleCreate {
    pub adjuster_type: AdjustmentActionType,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub event_patterns: Vec<String>,
    #[serde(default = "default_rule_priority")]
    pub priority: i32,
    #[serde(default)]
    pub conditions: Option<ConditionOperator>,
    pub config: serde_json::Value,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub department_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOrderAdjustmentRuleUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub event_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub conditions: Option<ConditionOperator>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub department_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDrivenGenerationRuleCreate {
    #[serde(default = "default_generator_type")]
    pub generator_type: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub event_patterns: Vec<String>,
    #[serde(default = "default_rule_priority")]
    pub priority: i32,
    #[serde(default)]
    pub conditions: Option<ConditionOperator>,
    pub config: GenerationRuleConfig,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub department_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDrivenGenerationRuleUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub event_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub conditions: Option<ConditionOperator>,
    #[serde(default)]
    pub config: Option<GenerationRuleConfig>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub department_id: Option<String>,
}

fn default_generator_type() -> String {
    "event_generated".to_string()
}

fn default_rule_priority() -> i32 {
    100
}

fn default_true() -> bool {
    true
}
