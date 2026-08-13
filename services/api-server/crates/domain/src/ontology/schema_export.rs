//! Ontology V1 schema 导出；现行字段见 `docs/architecture/ONTOLOGY_V1.md`。
//!
//! 版本命名决策：
//! - AI Ontology 的 `ontology_version` 统一为 [`FLIGHT_OPS_ONTOLOGY_VERSION`]
//!   （`flight-ops.v1`），不使用裸 `v1.0`。
//! - 飞机中心资源本体（`/api/v2/ontology`，migration 119）是独立域模型，
//!   不参与 AI `ontology_version` 命名空间，二者不得混用。
//!
//! `correlation_id` / `object_id` / `arguments` 是运行时 proposal 字段
//! （运行时 proposal 字段），不属于 schema 导出；schema 只声明 `arguments_schema`。

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::models::ai_ontology::{OntologyActionDef, OntologyConstraint, OntologyObjectDef, OntologySchema};

/// AI Ontology V1 统一版本号。
pub const FLIGHT_OPS_ONTOLOGY_VERSION: &str = "flight-ops.v1";

/// schema 导出中固化的 `exported_at`（仅用于 fixture 对比）。
pub const FIXTURE_EXPORTED_AT: &str = "2026-05-11T00:00:00Z";

/// 风险等级 → 默认审批策略。
pub fn default_risk_policies() -> BTreeMap<String, String> {
    let mut policies = BTreeMap::new();
    policies.insert("low".to_string(), "auto_execute".to_string());
    policies.insert("medium".to_string(), "require_approval".to_string());
    policies.insert("high".to_string(), "require_approval".to_string());
    policies.insert("critical".to_string(), "require_approval".to_string());
    policies
}

/// schema 中单个动作的导出视图。
#[derive(Debug, Clone, Serialize)]
pub struct ExportedActionRef {
    pub ontology_version: String,
    pub object_type: String,
    pub action_name: String,
    pub category: String,
    /// 输入参数 JSON schema（运行时的 `arguments` 必须符合该 schema）。
    pub arguments_schema: Value,
    pub risk_level: String,
    pub required_permissions: Vec<String>,
    pub approval_policy: String,
    pub execution_mapping: Option<String>,
}

/// schema 中单个约束的导出视图（带对象/动作定位）。
#[derive(Debug, Clone, Serialize)]
pub struct ExportedConstraintRef {
    pub object_type: String,
    pub action_name: String,
    pub constraint: OntologyConstraint,
}

/// `/api/v2/ai/ontology/schema` 的稳定导出结构。
#[derive(Debug, Clone, Serialize)]
pub struct OntologySchemaExport {
    pub ontology_version: String,
    pub description: String,
    pub exported_at: DateTime<Utc>,
    pub objects: std::collections::HashMap<String, OntologyObjectDef>,
    /// 扁平动作表，键为 `{Object}.{action_name}`。
    pub actions: BTreeMap<String, ExportedActionRef>,
    pub risk_policies: BTreeMap<String, String>,
    pub constraints: Vec<ExportedConstraintRef>,
}

/// 将内部 schema 转换为稳定的 schema export 结构。
pub fn build_schema_export(schema: &OntologySchema, exported_at: DateTime<Utc>) -> OntologySchemaExport {
    let mut actions = BTreeMap::new();
    let mut constraints = Vec::new();

    for (object_name, object) in &schema.objects {
        for (action_name, action) in &object.actions {
            let key = format!("{object_name}.{action_name}");
            actions.insert(key.clone(), export_action_ref(schema, object_name, action));
            for constraint in &action.constraints {
                constraints.push(ExportedConstraintRef {
                    object_type: object_name.clone(),
                    action_name: action_name.clone(),
                    constraint: constraint.clone(),
                });
            }
        }
    }
    constraints.sort_by(|a, b| {
        (&a.object_type, &a.action_name, &a.constraint.expression).cmp(&(
            &b.object_type,
            &b.action_name,
            &b.constraint.expression,
        ))
    });

    OntologySchemaExport {
        ontology_version: schema.version.clone(),
        description: schema.description.clone(),
        exported_at,
        objects: schema.objects.clone(),
        actions,
        risk_policies: default_risk_policies(),
        constraints,
    }
}

fn export_action_ref(schema: &OntologySchema, object_type: &str, action: &OntologyActionDef) -> ExportedActionRef {
    ExportedActionRef {
        ontology_version: schema.version.clone(),
        object_type: object_type.to_string(),
        action_name: action.name.clone(),
        category: action.category.clone(),
        arguments_schema: action.parameters_schema.clone(),
        risk_level: action.risk_level.clone(),
        required_permissions: action.required_permissions.clone(),
        approval_policy: action.approval_policy.clone(),
        execution_mapping: action.execution_mapping.clone(),
    }
}

/// 递归按键排序 JSON 对象，保证 fixture 对比确定性（HashMap 顺序不稳定）。
pub fn canonical_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> =
                map.into_iter().map(|(key, item)| (key, canonical_json(item))).collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonical_json).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::flight_ops_v1::build_flight_ops_v1_schema;
    use serde_json::json;

    fn fixture_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../docs/fixtures/flight_ops_v1_ontology_schema.json")
    }

    fn fixture_export() -> OntologySchemaExport {
        let schema = build_flight_ops_v1_schema();
        let exported_at = DateTime::parse_from_rfc3339(FIXTURE_EXPORTED_AT)
            .expect("fixture exported_at parses")
            .with_timezone(&Utc);
        build_schema_export(&schema, exported_at)
    }

    #[test]
    fn export_envelope_uses_contract_field_names() {
        let export = fixture_export();
        let value = serde_json::to_value(&export).expect("serialize export");
        for key in [
            "ontology_version",
            "description",
            "exported_at",
            "objects",
            "actions",
            "risk_policies",
            "constraints",
        ] {
            assert!(value.get(key).is_some(), "export missing contract field {key}");
        }
        assert_eq!(export.ontology_version, FLIGHT_OPS_ONTOLOGY_VERSION);
    }

    #[test]
    fn exported_actions_carry_contract_fields() {
        let export = fixture_export();
        assert!(!export.actions.is_empty());
        for (key, action) in &export.actions {
            assert_eq!(key, &format!("{}.{}", action.object_type, action.action_name));
            assert_eq!(action.ontology_version, FLIGHT_OPS_ONTOLOGY_VERSION);
            assert!(!action.risk_level.is_empty());
            assert!(!action.approval_policy.is_empty());
            assert!(
                !action.required_permissions.is_empty(),
                "{key} must declare permissions"
            );
            assert_eq!(
                action.arguments_schema["type"],
                json!("object"),
                "{key} arguments_schema"
            );
        }
    }

    #[test]
    fn flight_ops_v1_export_matches_fixture() {
        let export = fixture_export();
        let actual = canonical_json(serde_json::to_value(&export).expect("serialize export"));
        let path = fixture_path();

        if std::env::var("UPDATE_ONTOLOGY_FIXTURE").is_ok() {
            let pretty = serde_json::to_string_pretty(&actual).expect("pretty fixture");
            std::fs::write(&path, pretty + "\n").expect("write fixture");
            return;
        }

        let expected_text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!(
                "missing ontology fixture {} (regenerate with UPDATE_ONTOLOGY_FIXTURE=1): {err}",
                path.display()
            )
        });
        let expected = canonical_json(serde_json::from_str(&expected_text).expect("fixture is valid JSON"));
        assert_eq!(actual, expected, "schema export drifted from committed fixture");
    }
}
