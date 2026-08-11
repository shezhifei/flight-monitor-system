//! AI extraction / business case property config types and the helpers that
//! normalize and merge legacy `ai_extraction_config` with case-property
//! `ai_copilot` overrides.
//!
//! All public config structs are re-exported via `crate::services::...::Foo`
//! for backwards compatibility.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ── AI extraction config (legacy) ────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AiLegBindingConfig {
    #[serde(default)]
    pub allowed: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AiFlightMatchingConfig {
    #[serde(default)]
    pub allow_numeric_suffix: Option<bool>,
    #[serde(default)]
    pub prefer_leg: Option<String>,
    #[serde(default)]
    pub exclude_cancelled: Option<bool>,
    #[serde(default)]
    pub exclude_departed: Option<bool>,
    #[serde(default)]
    pub exclude_actual_departure: Option<bool>,
    #[serde(default)]
    pub window_hours_before: Option<i64>,
    #[serde(default)]
    pub window_hours_after: Option<i64>,
    #[serde(default)]
    pub min_auto_match_score: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AiFieldConfig {
    #[serde(default, rename = "type")]
    pub field_type: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub enum_values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct BusinessCaseAiExtractionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub trigger_phrases: Vec<String>,
    #[serde(default)]
    pub leg_binding: AiLegBindingConfig,
    #[serde(default)]
    pub flight_matching: AiFlightMatchingConfig,
    #[serde(default)]
    pub fields: HashMap<String, AiFieldConfig>,
    #[serde(default)]
    pub forbidden_fields: Vec<String>,
    #[serde(default)]
    pub description_template: Option<String>,
    #[serde(default)]
    pub remarks_template: Option<String>,
    #[serde(default)]
    pub examples: Vec<serde_json::Value>,
    #[serde(default)]
    pub confidence_threshold: Option<f64>,
    #[serde(default, flatten)]
    pub extensions: HashMap<String, serde_json::Value>,
}

pub fn parse_ai_extraction_config(raw: &serde_json::Value) -> Option<BusinessCaseAiExtractionConfig> {
    let parsed: BusinessCaseAiExtractionConfig = serde_json::from_value(raw.clone()).ok()?;
    if parsed.enabled {
        Some(parsed)
    } else {
        None
    }
}

// ── Business case properties (new) ──────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CaseBindingPolicy {
    #[serde(default)]
    pub flight_required: bool,
    #[serde(default)]
    pub allowed_leg_types: Vec<String>,
    #[serde(default)]
    pub default_leg_type: Option<String>,
    #[serde(default)]
    pub leg_type_required: bool,
    #[serde(default)]
    pub flight_match_policy: CaseFlightMatchPolicy,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CaseFlightMatchPolicy {
    #[serde(default)]
    pub allow_numeric_suffix: Option<bool>,
    #[serde(default)]
    pub exclude_cancelled: Option<bool>,
    #[serde(default)]
    pub exclude_departed: Option<bool>,
    #[serde(default)]
    pub exclude_actual_departure: Option<bool>,
    #[serde(default)]
    pub time_window_hours_before: Option<i64>,
    #[serde(default)]
    pub time_window_hours_after: Option<i64>,
    #[serde(default)]
    pub min_auto_match_score: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ExtraInfoFieldSchema {
    #[serde(default, rename = "type")]
    pub field_type: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub enum_values: Vec<String>,
    #[serde(default)]
    pub display_in_notification: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ExtraInfoSchema {
    #[serde(default)]
    pub fields: HashMap<String, ExtraInfoFieldSchema>,
    #[serde(default)]
    pub summary_template: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CaseWorkflowPolicy {
    #[serde(default)]
    pub batch_notification_enabled: bool,
    #[serde(default)]
    pub batch_receipt_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CaseDuplicatePolicy {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub include_extra_info: bool,
    #[serde(default)]
    pub include_bound_leg: bool,
    #[serde(default)]
    pub active_statuses: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct BusinessCaseProperties {
    #[serde(default)]
    pub binding_policy: CaseBindingPolicy,
    #[serde(default)]
    pub extra_info_schema: ExtraInfoSchema,
    #[serde(default)]
    pub workflow_policy: CaseWorkflowPolicy,
    #[serde(default)]
    pub duplicate_policy: CaseDuplicatePolicy,
}

pub fn parse_case_properties(raw: &serde_json::Value) -> BusinessCaseProperties {
    serde_json::from_value(raw.clone()).unwrap_or_default()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct CasePropertiesAiCopilotConfig {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default, alias = "trigger_phrases")]
    utterances: Option<Vec<String>>,
    #[serde(default, alias = "leg_type_hint")]
    leg_type: Option<String>,
    #[serde(default)]
    required_fields: Option<Vec<String>>,
    #[serde(default)]
    field_hints: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    examples: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    remarks_template: Option<String>,
    #[serde(default)]
    confidence_threshold: Option<f64>,
    #[serde(default, flatten)]
    extensions: HashMap<String, serde_json::Value>,
}

pub(crate) fn normalize_business_case_ai_extraction_config(
    ai_extraction_config: &serde_json::Value,
    case_properties_raw: &serde_json::Value,
    case_properties: &BusinessCaseProperties,
) -> Option<BusinessCaseAiExtractionConfig> {
    let mut config = derive_ai_extraction_config_from_case_properties(case_properties);

    if let Some(legacy_config) = parse_ai_extraction_config(ai_extraction_config) {
        apply_legacy_ai_extraction_config(&mut config, legacy_config, ai_extraction_config);
    }

    if let Some(ai_copilot_raw) = case_properties_raw.get("ai_copilot") {
        let ai_copilot: CasePropertiesAiCopilotConfig =
            serde_json::from_value(ai_copilot_raw.clone()).unwrap_or_default();
        if ai_copilot.enabled == Some(false) {
            return None;
        }
        apply_case_properties_ai_copilot_config(&mut config, ai_copilot);
    }

    config.enabled.then_some(config)
}

pub(crate) fn derive_ai_extraction_config_from_case_properties(
    case_properties: &BusinessCaseProperties,
) -> BusinessCaseAiExtractionConfig {
    let binding_policy = &case_properties.binding_policy;
    let flight_match_policy = &binding_policy.flight_match_policy;
    let fields = case_properties
        .extra_info_schema
        .fields
        .iter()
        .map(|(field_name, schema)| {
            (
                field_name.clone(),
                AiFieldConfig {
                    field_type: schema.field_type.clone(),
                    label: schema.label.clone(),
                    required: schema.required,
                    enum_values: schema.enum_values.clone(),
                    ..Default::default()
                },
            )
        })
        .collect();

    BusinessCaseAiExtractionConfig {
        enabled: false,
        leg_binding: AiLegBindingConfig {
            allowed: binding_policy.allowed_leg_types.clone(),
            default: binding_policy.default_leg_type.clone(),
            required: binding_policy.leg_type_required,
        },
        flight_matching: AiFlightMatchingConfig {
            allow_numeric_suffix: flight_match_policy.allow_numeric_suffix,
            prefer_leg: binding_policy.default_leg_type.clone(),
            exclude_cancelled: flight_match_policy.exclude_cancelled,
            exclude_departed: flight_match_policy.exclude_departed,
            exclude_actual_departure: flight_match_policy.exclude_actual_departure,
            window_hours_before: flight_match_policy.time_window_hours_before,
            window_hours_after: flight_match_policy.time_window_hours_after,
            min_auto_match_score: flight_match_policy.min_auto_match_score,
        },
        fields,
        remarks_template: case_properties.extra_info_schema.summary_template.clone(),
        ..Default::default()
    }
}

pub(crate) fn apply_legacy_ai_extraction_config(
    target: &mut BusinessCaseAiExtractionConfig,
    legacy: BusinessCaseAiExtractionConfig,
    raw: &serde_json::Value,
) {
    target.enabled = true;
    if raw.get("aliases").is_some() {
        target.aliases = legacy.aliases;
    }
    if raw.get("trigger_phrases").is_some() {
        target.trigger_phrases = legacy.trigger_phrases;
    }
    if raw.get("leg_binding").is_some() {
        target.leg_binding = legacy.leg_binding;
    }
    if raw.get("flight_matching").is_some() {
        target.flight_matching = legacy.flight_matching;
    }
    if raw.get("fields").is_some() {
        target.fields = legacy.fields;
    }
    if raw.get("forbidden_fields").is_some() {
        target.forbidden_fields = legacy.forbidden_fields;
    }
    if raw.get("description_template").is_some() {
        target.description_template = legacy.description_template;
    }
    if raw.get("remarks_template").is_some() {
        target.remarks_template = legacy.remarks_template;
    }
    if raw.get("examples").is_some() {
        target.examples = legacy.examples;
    }
    if raw.get("confidence_threshold").is_some() {
        target.confidence_threshold = legacy.confidence_threshold;
    }
    target.extensions.extend(legacy.extensions);
}

pub(crate) fn apply_case_properties_ai_copilot_config(
    target: &mut BusinessCaseAiExtractionConfig,
    ai_copilot: CasePropertiesAiCopilotConfig,
) {
    if ai_copilot.enabled == Some(true) {
        target.enabled = true;
    }
    if let Some(aliases) = ai_copilot.aliases {
        target.aliases = aliases;
    }
    if let Some(utterances) = ai_copilot.utterances {
        target.trigger_phrases = utterances;
    }
    if let Some(leg_type) = normalize_optional_string(ai_copilot.leg_type) {
        target.leg_binding.default = Some(leg_type.clone());
        if target.leg_binding.allowed.is_empty() {
            target.leg_binding.allowed = vec![leg_type.clone()];
        }
        target.flight_matching.prefer_leg = Some(leg_type);
    }
    if let Some(field_hints) = ai_copilot.field_hints {
        target.extensions.insert("field_hints".to_string(), json!(field_hints));
        let field_hints = target
            .extensions
            .get("field_hints")
            .and_then(|value| value.as_object())
            .cloned()
            .unwrap_or_default();
        for (field_name, hint) in field_hints {
            apply_field_hint(&mut target.fields, &field_name, &hint);
        }
    }
    if let Some(required_fields) = ai_copilot.required_fields {
        for field_name in required_fields
            .into_iter()
            .map(|field| field.trim().to_string())
            .filter(|field| !field.is_empty())
        {
            target.fields.entry(field_name).or_default().required = true;
        }
    }
    if let Some(examples) = ai_copilot.examples {
        target.examples = examples;
    }
    if let Some(remarks_template) = normalize_optional_string(ai_copilot.remarks_template) {
        target.remarks_template = Some(remarks_template);
    }
    if let Some(confidence_threshold) = ai_copilot.confidence_threshold {
        target.confidence_threshold = Some(confidence_threshold);
    }
    target.extensions.extend(ai_copilot.extensions);
}

pub(crate) fn apply_field_hint(
    fields: &mut HashMap<String, AiFieldConfig>,
    field_name: &str,
    hint: &serde_json::Value,
) {
    let field_name = field_name.trim();
    if field_name.is_empty() {
        return;
    }

    let field = fields.entry(field_name.to_string()).or_default();
    if let Some(label) = hint.as_str().map(str::trim).filter(|label| !label.is_empty()) {
        field.label = Some(label.to_string());
        return;
    }

    let Some(hint_obj) = hint.as_object() else {
        return;
    };

    if let Some(field_type) = hint_obj
        .get("type")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        field.field_type = Some(field_type.to_string());
    }
    if let Some(label) = hint_obj
        .get("label")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        field.label = Some(label.to_string());
    }
    if let Some(required) = hint_obj.get("required").and_then(|value| value.as_bool()) {
        field.required = required;
    }
    if let Some(aliases) = string_vec_from_json(hint_obj.get("aliases")) {
        field.aliases = aliases;
    }
    if let Some(examples) = string_vec_from_json(hint_obj.get("examples")) {
        field.examples = examples;
    }
    if let Some(enum_values) = string_vec_from_json(hint_obj.get("enum_values")) {
        field.enum_values = enum_values;
    }
}

pub(crate) fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn string_vec_from_json(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    Some(
        value?
            .as_array()?
            .iter()
            .filter_map(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

// ── Internal catalog / commit-prep helpers ───────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct CopilotCaseTypeCatalogEntry {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub config: BusinessCaseAiExtractionConfig,
    pub case_properties: BusinessCaseProperties,
}

pub(crate) struct PreparedCommitAction {
    pub action: super::schemas::AiCopilotApprovedAction,
    pub flight_id: String,
    pub flight_no: String,
    pub description: String,
    pub status: Option<String>,
    pub context: HashMap<String, Value>,
    pub duplicate_policy: CaseDuplicatePolicy,
}
