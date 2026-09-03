//! Ontology 治理加载：`load_governed_schema` 是唯一能把代码 schema 与
//! DB overlay 合并成 `OntologySchema` 的地方。
//!
//! `AiOntologyRepository` 只能交回 `Vec<ActionOverlay>`（装不下对象/字段/动作清单），
//! 因此「DB 整份替换 schema」在此从「不推荐」变成「不可拼写」。所有读 schema 的
//! 入口（HTTP / validator / generate / ingest / execute 校验）都必须经过这里。

use crate::models::ai_ontology::OntologySchema;
use crate::models::ai_proposal::RiskLevel;
use crate::models::field_overlay::FieldOverlay;
use crate::ontology::flight_ops_v1::build_flight_ops_v1_schema;

/// 覆盖已知 `(object, action)` 键的启用 / 风险 / 审批。只能覆盖代码 schema 里
/// 已经存在的动作键，无法新增对象、字段或动作清单。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActionOverlay {
    /// 对象类型，例如 `Flight`。
    pub object: String,
    /// 动作名，例如 `change_delay`。
    pub action: String,
    /// 启用状态。`Some(false)` 表示停用该动作（从 governed schema 移除）；
    /// `None` 表示不改变当前启用状态。
    pub is_active: Option<bool>,
    /// 风险等级覆盖。
    pub risk: Option<RiskLevel>,
    /// 审批覆盖（`Some(true)` = 必须审批，`Some(false)` = 自动执行）。
    pub requires_approval: Option<bool>,
}

/// 唯一能得到 `OntologySchema` 的地方：以代码 schema 为真相源，把传入的
/// overlay 覆盖回已知动作键上。
pub fn load_governed_schema(overlays: &[ActionOverlay]) -> OntologySchema {
    let mut schema = build_flight_ops_v1_schema();

    for overlay in overlays {
        let Some(object) = schema.objects.get_mut(&overlay.object) else {
            continue;
        };
        let Some(action) = object.actions.get_mut(&overlay.action) else {
            continue;
        };

        if overlay.is_active == Some(false) {
            // overlay 显式停用：动作不再出现在 governed schema。
            object.actions.remove(&overlay.action);
            continue;
        }

        if let Some(risk) = overlay.risk {
            action.risk_level = risk.label().to_string();
        }
        if let Some(requires_approval) = overlay.requires_approval {
            let policy = if requires_approval {
                "require_approval"
            } else {
                "auto_execute"
            };
            action.approval_strategy = policy.to_string();
            action.approval_policy = policy.to_string();
        }
    }

    schema
}

/// 合并字段 overlay。对象与字段必须先存在于代码合同；overlay 只能补充元数据，
/// 不能新增对象、改核心字段类型或覆盖核心字段名称。
pub fn load_governed_schema_with_fields(
    action_overlays: &[ActionOverlay],
    field_overlays: &[FieldOverlay],
) -> OntologySchema {
    let mut schema = load_governed_schema(action_overlays);
    for overlay in field_overlays.iter().filter(|item| item.is_active) {
        let Some(object) = schema.objects.get_mut(&overlay.object_name) else {
            continue;
        };
        let field = object.fields.entry(overlay.field_name.clone()).or_insert_with(|| {
            crate::models::ai_ontology::OntologyFieldDef {
                name: overlay.field_name.clone(),
                field_type: overlay.field_type.clone(),
                description: overlay.description.clone().unwrap_or_default(),
                required: overlay.required,
                ..Default::default()
            }
        });
        // 代码核心字段不允许被 overlay 改类型；扩展字段以 overlay 的类型创建。
        if field.field_type != overlay.field_type && !field.field_type.is_empty() {
            continue;
        }
        field.catalog_code = overlay.catalog_code.clone();
        field.object_name_target = overlay.object_name_target.clone();
        field.required = overlay.required;
        field.list_visible = Some(overlay.list_visible);
        field.filterable = Some(overlay.filterable);
        field.widget = overlay.widget.clone();
        if let Some(description) = &overlay.description {
            field.description = description.clone();
        }
        field.visible_when = overlay.visible_when.clone();
        field.max_length = overlay.max_length;
        field.min = overlay.min;
        field.max = overlay.max;
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::flight_ops_v1::build_flight_ops_v1_schema;

    fn overlay(object: &str, action: &str) -> ActionOverlay {
        ActionOverlay {
            object: object.to_string(),
            action: action.to_string(),
            is_active: None,
            risk: None,
            requires_approval: None,
        }
    }

    #[test]
    fn no_overlays_yields_base_schema() {
        let schema = load_governed_schema(&[]);
        assert!(!schema.objects.is_empty());
        assert!(schema.objects.contains_key("Flight"));
    }

    #[test]
    fn unknown_object_or_action_is_ignored() {
        let overlays = vec![overlay("DoesNotExist", "create"), overlay("Flight", "does_not_exist")];
        let schema = load_governed_schema(&overlays);
        assert!(schema.objects.contains_key("Flight"));
    }

    #[test]
    fn risk_override_writes_risk_level() {
        let overlays = vec![ActionOverlay {
            object: "BusinessCase".to_string(),
            action: "create".to_string(),
            is_active: None,
            risk: Some(RiskLevel::Critical),
            requires_approval: None,
        }];
        let schema = load_governed_schema(&overlays);
        assert_eq!(schema.objects["BusinessCase"].actions["create"].risk_level, "critical");
    }

    #[test]
    fn requires_approval_override_writes_approval() {
        let overlays = vec![ActionOverlay {
            object: "BusinessCase".to_string(),
            action: "create".to_string(),
            is_active: None,
            risk: None,
            requires_approval: Some(false),
        }];
        let schema = load_governed_schema(&overlays);
        let action = &schema.objects["BusinessCase"].actions["create"];
        assert_eq!(action.approval_policy, "auto_execute");
    }

    #[test]
    fn is_active_false_removes_action() {
        let overlays = vec![ActionOverlay {
            object: "BusinessCase".to_string(),
            action: "create".to_string(),
            is_active: Some(false),
            risk: None,
            requires_approval: None,
        }];
        let schema = load_governed_schema(&overlays);
        assert!(!schema.objects["BusinessCase"].actions.contains_key("create"));
    }

    #[test]
    fn governed_schema_is_deterministic_superset_of_base_keys() {
        let base = build_flight_ops_v1_schema();
        let overlays = vec![ActionOverlay {
            object: "BusinessCase".to_string(),
            action: "create".to_string(),
            is_active: None,
            risk: Some(RiskLevel::High),
            requires_approval: Some(true),
        }];
        let governed = load_governed_schema(&overlays);
        assert!(governed.objects.len() == base.objects.len());
        assert!(governed.objects["BusinessCase"].actions.contains_key("create"));
    }

    #[test]
    fn field_overlay_adds_extension_without_changing_action_contract() {
        let schema = load_governed_schema_with_fields(
            &[],
            &[FieldOverlay {
                object_name: "Stand".into(),
                field_name: "max_size_category".into(),
                field_type: "catalog_ref".into(),
                catalog_code: Some("icao_size".into()),
                object_name_target: None,
                required: false,
                list_visible: true,
                filterable: true,
                widget: Some("select".into()),
                description: Some("最大机型等级".into()),
                visible_when: None,
                max_length: None,
                min: None,
                max: None,
                is_active: true,
                created_at: None,
                updated_at: None,
            }],
        );
        let field = &schema.objects["Stand"].fields["max_size_category"];
        assert_eq!(field.catalog_code.as_deref(), Some("icao_size"));
        assert_eq!(field.widget.as_deref(), Some("select"));
        assert!(schema.objects["Stand"].actions.contains_key("get_context"));
    }
}
