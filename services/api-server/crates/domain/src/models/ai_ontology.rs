use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologySchema {
    pub version: String,
    pub description: String,
    pub objects: HashMap<String, OntologyObjectDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyObjectDef {
    pub name: String,
    pub description: String,
    pub object_id_strategy: String,
    pub fields: HashMap<String, OntologyFieldDef>,
    pub relations: HashMap<String, OntologyRelationDef>,
    pub actions: HashMap<String, OntologyActionDef>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct OntologyFieldDef {
    pub name: String,
    pub field_type: String,
    pub description: String,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_name_target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filterable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_when: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyRelationDef {
    pub name: String,
    pub target_object: String,
    pub relation_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyActionDef {
    pub name: String,
    pub description: String,
    pub category: String,
    pub parameters: HashMap<String, OntologyActionParameter>,
    pub parameters_schema: Value,
    pub required_permissions: Vec<String>,
    pub risk_level: String,
    pub approval_strategy: String,
    pub approval_policy: String,
    pub constraints: Vec<OntologyConstraint>,
    pub execution_mapping: Option<String>,
    pub idempotency_key_strategy: Option<String>,
    #[serde(default)]
    pub compensation: Option<CompensationMetadata>,
}

impl Default for OntologyActionDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            category: String::new(),
            parameters: HashMap::new(),
            parameters_schema: Value::Null,
            required_permissions: Vec::new(),
            risk_level: String::new(),
            approval_strategy: String::new(),
            approval_policy: String::new(),
            constraints: Vec::new(),
            execution_mapping: None,
            idempotency_key_strategy: None,
            compensation: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyActionParameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyConstraint {
    pub constraint_type: String,
    pub expression: String,
    pub description: String,
}

/// Per-action compensation metadata consumed by `CompensationPlanner`.
///
/// `mode` mirrors [`crate::models::ai_execution::AiCompensationMode`];
/// `inverse_action_name` and `before_snapshot_required` are only
/// meaningful for the corresponding modes. `irreversible_fields`
/// names object columns that, once written, cannot be cleanly undone
/// (e.g. notification body, dispatch order publish) — the planner
/// uses this list to refuse `restore_snapshot` and fall back to
/// `followup_action` / `irreversible`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompensationMetadata {
    pub mode: String,
    #[serde(default = "default_true")]
    pub requires_approval: bool,
    #[serde(default)]
    pub irreversible_fields: Vec<String>,
    #[serde(default)]
    pub inverse_action_name: Option<String>,
    #[serde(default)]
    pub before_snapshot_required: bool,
    #[serde(default)]
    pub followup_action_name: Option<String>,
    #[serde(default)]
    pub followup_args: Option<Value>,
}

fn default_true() -> bool {
    true
}

/// Concrete binding from an ontology action to a `DomainActionExecutor`
/// handler. The string `execution_mapping` stored on the ontology is
/// parsed into a `DomainActionExecutor.<Object>.<Action>` triple by
/// the application service; this struct is the parsed form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionExecutionMapping {
    pub executor: String,
    pub object_type: String,
    pub action_name: String,
}
