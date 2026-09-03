use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::field_overlay::OntologyFieldType;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::{
    DepartmentQualificationRepository, DepartmentRepository, EquipmentRepository, EquipmentTypeRepository,
    StandRepository, TaskTypeRepository, TeamRepository, TeamTypeRepository, TerminalRepository,
};
use fms_domain::ports::field_overlay_repository::FieldOverlayRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceRepository;
use fms_domain::ports::user_repository::UserRepository;
use serde_json::{Map, Value};

#[async_trait]
pub trait ObjectReferenceValidator: Send + Sync {
    async fn validate(&self, object_name: &str, attributes: &Value) -> Result<(), DomainError>;
}

/// Repository-backed resolver for dynamic `object_ref` overlays. Keeping the
/// target lookup behind one application port lets order/rule services enforce
/// the same active-object contract without duplicating repository matching.
pub struct RepositoryObjectReferenceValidator {
    field_overlay_repo: Arc<dyn FieldOverlayRepository + Send + Sync>,
    department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
    team_repo: Arc<dyn TeamRepository + Send + Sync>,
    team_type_repo: Arc<dyn TeamTypeRepository + Send + Sync>,
    equipment_type_repo: Arc<dyn EquipmentTypeRepository + Send + Sync>,
    equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
    stand_repo: Arc<dyn StandRepository + Send + Sync>,
    task_type_repo: Arc<dyn TaskTypeRepository + Send + Sync>,
    user_repo: Arc<dyn UserRepository + Send + Sync>,
    terminal_repo: Arc<dyn TerminalRepository + Send + Sync>,
    qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
}

impl RepositoryObjectReferenceValidator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        field_overlay_repo: Arc<dyn FieldOverlayRepository + Send + Sync>,
        department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        team_type_repo: Arc<dyn TeamTypeRepository + Send + Sync>,
        equipment_type_repo: Arc<dyn EquipmentTypeRepository + Send + Sync>,
        equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
        stand_repo: Arc<dyn StandRepository + Send + Sync>,
        task_type_repo: Arc<dyn TaskTypeRepository + Send + Sync>,
        user_repo: Arc<dyn UserRepository + Send + Sync>,
        terminal_repo: Arc<dyn TerminalRepository + Send + Sync>,
        qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
    ) -> Self {
        Self {
            field_overlay_repo,
            department_repo,
            team_repo,
            team_type_repo,
            equipment_type_repo,
            equipment_repo,
            stand_repo,
            task_type_repo,
            user_repo,
            terminal_repo,
            qualification_repo,
        }
    }
}

#[async_trait]
impl ObjectReferenceValidator for RepositoryObjectReferenceValidator {
    async fn validate(&self, object_name: &str, attributes: &Value) -> Result<(), DomainError> {
        let Some(map) = attributes.as_object() else {
            return Ok(());
        };
        let overlays = self.field_overlay_repo.list(Some(object_name), false).await?;
        for field in overlays.iter().filter(|item| item.is_active) {
            let Some(field_type) = OntologyFieldType::parse(&field.field_type) else {
                continue;
            };
            if !field_type.is_object() {
                continue;
            }
            let Some(target) = field.object_name_target.as_deref() else {
                continue;
            };
            let Some(raw) = map.get(&field.field_name) else {
                continue;
            };
            let keys: Vec<&str> = match field_type {
                OntologyFieldType::ObjectRef => raw.as_str().into_iter().collect(),
                OntologyFieldType::ObjectRefArray => raw
                    .as_array()
                    .map(|values| values.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            for key in keys {
                // `Some(true)` = 目标存在且启用；`Some(false)`/`None` = 不存在或已停用。
                // 未知 target 名是字段定义配置错误（ValidationError），不能伪装成
                // 「目标不存在」的 409。
                let active = match target {
                    "Department" => self.department_repo.find_by_id(key).await?.map(|item| item.is_active),
                    "Team" => self.team_repo.find_by_id(key, false).await?.map(|item| item.is_active),
                    "TeamType" => self.team_type_repo.find_by_id(key).await?.map(|item| item.is_active),
                    "EquipmentType" => self
                        .equipment_type_repo
                        .find_by_id(key)
                        .await?
                        .map(|item| item.is_active),
                    "Equipment" => self.equipment_repo.find_by_id(key).await?.map(|item| item.is_active),
                    "Stand" => self
                        .stand_repo
                        .find_by_id(key)
                        .await?
                        .or(self.stand_repo.find_by_code(key).await?)
                        .map(|item| item.is_active),
                    "TaskType" => self
                        .task_type_repo
                        .find_by_id(key)
                        .await?
                        .or(self.task_type_repo.find_by_code(key).await?)
                        .map(|item| item.is_active),
                    "Personnel" => self.user_repo.find_by_id(key).await?.map(|item| item.is_active),
                    "Gate" => self
                        .terminal_repo
                        .find_gate_by_id(key)
                        .await?
                        .or(self.terminal_repo.find_gate_by_code(key).await?)
                        .map(|item| item.is_active),
                    "Terminal" => self
                        .terminal_repo
                        .find_terminal_by_id(key)
                        .await?
                        .or(self.terminal_repo.find_terminal_by_code(key).await?)
                        .map(|item| item.is_active),
                    "BaggageCarousel" => self
                        .terminal_repo
                        .find_carousel_by_id(key)
                        .await?
                        .or(self.terminal_repo.find_carousel_by_code(key).await?)
                        .map(|item| item.is_active),
                    "Qualification" => self
                        .qualification_repo
                        .find_catalog_by_key(key)
                        .await?
                        .map(|item| item.is_active),
                    unknown_target => {
                        return Err(DomainError::ValidationError(format!(
                            "扩展字段 {object_name}.{} 配置了不支持的对象引用目标: {unknown_target}",
                            field.field_name
                        )));
                    }
                };
                if active != Some(true) {
                    return Err(DomainError::Conflict(format!(
                        "扩展字段 {object_name}.{} 引用了不存在或已停用的 {target}: {key}",
                        field.field_name
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Validate an extensible attribute object against the active field overlays.
///
/// The repository is optional so unit-test doubles and deployments that have
/// not enabled ontology metadata retain the historical pass-through behavior.
/// When overlays are available, unknown/inactive fields and type/range
/// violations are rejected at the application boundary. Object-reference
/// arrays are normalized to a stable, duplicate-free list.
pub async fn validate_attributes(
    object_name: &str,
    value: Value,
    field_overlay_repo: Option<&Arc<dyn FieldOverlayRepository + Send + Sync>>,
) -> Result<Value, DomainError> {
    let value = if value.is_null() {
        Value::Object(Map::new())
    } else if value.is_object() {
        value
    } else {
        return Err(DomainError::ValidationError("attributes 必须是 JSON object".into()));
    };

    let Some(repo) = field_overlay_repo else {
        return Ok(value);
    };
    let overlays = repo.list(Some(object_name), false).await?;
    let object = value.as_object().expect("attributes normalization guarantees object");
    for field in overlays.iter().filter(|item| item.is_active && item.required) {
        if !field_is_visible(field.visible_when.as_ref(), object) {
            continue;
        }
        if !object.contains_key(&field.field_name) {
            return Err(DomainError::ValidationError(format!(
                "缺少必填扩展字段 {object_name}.{}",
                field.field_name
            )));
        }
    }
    let mut normalized = Map::with_capacity(object.len());

    for (key, raw) in object {
        let field = overlays
            .iter()
            .find(|item| item.is_active && item.field_name == *key)
            .ok_or_else(|| DomainError::ValidationError(format!("未知或已停用的 {object_name} 扩展字段: {key}")))?;
        // Server-side counterpart to the form's conditional visibility: values
        // for fields whose predicate is false are ignored rather than persisted.
        if !field_is_visible(field.visible_when.as_ref(), object) {
            continue;
        }
        let field_type = OntologyFieldType::parse(&field.field_type)
            .ok_or_else(|| DomainError::ValidationError(format!("扩展字段 {object_name}.{key} 类型无效")))?;

        let valid = match field_type {
            OntologyFieldType::String
            | OntologyFieldType::Datetime
            | OntologyFieldType::CatalogRef
            | OntologyFieldType::ObjectRef => raw.is_string(),
            OntologyFieldType::Number => raw.is_number(),
            OntologyFieldType::Boolean => raw.is_boolean(),
            OntologyFieldType::CatalogRefArray | OntologyFieldType::ObjectRefArray => raw
                .as_array()
                .map(|items| items.iter().all(Value::is_string))
                .unwrap_or(false),
        };
        if !valid {
            return Err(DomainError::ValidationError(format!(
                "扩展字段 {object_name}.{key} 类型不匹配"
            )));
        }

        if field_type.is_catalog() {
            let catalog_code = field.catalog_code.as_deref().ok_or_else(|| {
                DomainError::ValidationError(format!("扩展字段 {object_name}.{key} 未配置 catalog_code"))
            })?;
            let refs = match field_type {
                OntologyFieldType::CatalogRef => vec![raw.as_str().expect("catalog_ref type check")],
                OntologyFieldType::CatalogRefArray => raw
                    .as_array()
                    .expect("catalog_ref[] type check")
                    .iter()
                    .map(|item| item.as_str().expect("catalog_ref[] element type check"))
                    .collect(),
                _ => unreachable!("is_catalog only matches catalog types"),
            };
            for entry_code in refs {
                if !repo.catalog_entry_is_active(catalog_code, entry_code).await? {
                    return Err(DomainError::ValidationError(format!(
                        "扩展字段 {object_name}.{key} 引用了不存在或已停用的码表项: {entry_code}"
                    )));
                }
            }
        }

        if let Some(max_length) = field.max_length {
            let over_limit = raw
                .as_str()
                .map(|text| text.chars().count() > max_length as usize)
                .unwrap_or(false);
            if over_limit {
                return Err(DomainError::ValidationError(format!(
                    "扩展字段 {object_name}.{key} 超过最大长度"
                )));
            }
        }
        if let Some(number) = raw.as_f64() {
            if field.min.map(|min| number < min).unwrap_or(false) || field.max.map(|max| number > max).unwrap_or(false)
            {
                return Err(DomainError::ValidationError(format!(
                    "扩展字段 {object_name}.{key} 超出数值范围"
                )));
            }
        }

        let normalized_value = if field_type == OntologyFieldType::ObjectRefArray {
            let mut seen = HashSet::new();
            let values = raw
                .as_array()
                .expect("object_ref[] type check guarantees array")
                .iter()
                .filter(|item| seen.insert(item.as_str().expect("string type check")))
                .cloned()
                .collect();
            Value::Array(values)
        } else {
            raw.clone()
        };
        normalized.insert(key.clone(), normalized_value);
    }

    Ok(Value::Object(normalized))
}

fn field_is_visible(condition: Option<&Value>, object: &Map<String, Value>) -> bool {
    let Some(condition) = condition.and_then(Value::as_object) else {
        return true;
    };
    let Some(field) = condition.get("field").and_then(Value::as_str) else {
        return true;
    };
    let Some(actual) = object.get(field) else {
        // Core-field predicates are evaluated by the resource-specific layer;
        // the generic validator must not discard a value when that context is
        // unavailable in attributes.
        return true;
    };
    let expected = condition.get("value").unwrap_or(&Value::Null);
    match condition.get("op").and_then(Value::as_str).unwrap_or("eq") {
        "eq" => actual == expected,
        "neq" => actual != expected,
        "in" => expected
            .as_array()
            .map(|values| values.iter().any(|v| v == actual))
            .unwrap_or(false),
        "not_in" => expected
            .as_array()
            .map(|values| values.iter().all(|v| v != actual))
            .unwrap_or(true),
        "gt" => compare_json_numbers(actual, expected, |a, b| a > b),
        "gte" => compare_json_numbers(actual, expected, |a, b| a >= b),
        "lt" => compare_json_numbers(actual, expected, |a, b| a < b),
        "lte" => compare_json_numbers(actual, expected, |a, b| a <= b),
        _ => true,
    }
}

fn compare_json_numbers(actual: &Value, expected: &Value, predicate: impl FnOnce(f64, f64) -> bool) -> bool {
    match (actual.as_f64(), expected.as_f64()) {
        (Some(actual), Some(expected)) => predicate(actual, expected),
        _ => false,
    }
}

/// Rebuild the application-managed object-reference index for one owner.
/// Callers invoke this after the owner has been persisted; the operation is
/// idempotent and removes stale references from previous attribute values.
pub async fn collect_attribute_references(
    owner_object_name: &str,
    owner_object_id: &str,
    attributes: &Value,
    field_overlay_repo: Option<&Arc<dyn FieldOverlayRepository + Send + Sync>>,
) -> Result<Vec<OntologyAttributeReference>, DomainError> {
    let Some(field_repo) = field_overlay_repo else {
        return Ok(Vec::new());
    };
    let overlays = field_repo.list(Some(owner_object_name), false).await?;
    let object = attributes
        .as_object()
        .ok_or_else(|| DomainError::ValidationError("attributes 必须是 JSON object".into()))?;
    let mut references = Vec::new();
    for field in overlays.iter().filter(|item| item.is_active) {
        let Some(raw) = object.get(&field.field_name) else {
            continue;
        };
        let Some(field_type) = OntologyFieldType::parse(&field.field_type) else {
            continue;
        };
        if !field_type.is_object() {
            continue;
        }
        let target_name = field.object_name_target.as_deref().ok_or_else(|| {
            DomainError::ValidationError(format!(
                "扩展字段 {owner_object_name}.{} 未配置 object_name_target",
                field.field_name
            ))
        })?;
        let keys: Vec<&str> = match field_type {
            OntologyFieldType::ObjectRef => vec![raw.as_str().ok_or_else(|| {
                DomainError::ValidationError(format!("扩展字段 {owner_object_name}.{} 类型不匹配", field.field_name))
            })?],
            OntologyFieldType::ObjectRefArray => raw
                .as_array()
                .ok_or_else(|| {
                    DomainError::ValidationError(format!(
                        "扩展字段 {owner_object_name}.{} 类型不匹配",
                        field.field_name
                    ))
                })?
                .iter()
                .filter_map(Value::as_str)
                .collect(),
            _ => Vec::new(),
        };
        references.extend(keys.into_iter().map(|target_key| OntologyAttributeReference {
            id: None,
            owner_object_name: owner_object_name.to_string(),
            owner_object_id: owner_object_id.to_string(),
            field_name: field.field_name.clone(),
            target_object_name: target_name.to_string(),
            target_key: target_key.to_string(),
            created_at: None,
        }));
    }
    Ok(references)
}

pub async fn sync_attribute_references(
    owner_object_name: &str,
    owner_object_id: &str,
    attributes: &Value,
    field_overlay_repo: Option<&Arc<dyn FieldOverlayRepository + Send + Sync>>,
    reference_repo: Option<&Arc<dyn OntologyAttributeReferenceRepository + Send + Sync>>,
) -> Result<(), DomainError> {
    let (Some(field_repo), Some(reference_repo)) = (field_overlay_repo, reference_repo) else {
        return Ok(());
    };
    let references =
        collect_attribute_references(owner_object_name, owner_object_id, attributes, Some(field_repo)).await?;
    reference_repo
        .replace_owner_references(owner_object_name, owner_object_id, &references)
        .await
}

#[cfg(test)]
mod tests {
    use super::validate_attributes;
    use async_trait::async_trait;
    use fms_domain::error::DomainError;
    use fms_domain::models::field_overlay::FieldOverlay;
    use fms_domain::ports::field_overlay_repository::FieldOverlayRepository;
    use std::sync::Arc;

    struct Stub(Vec<FieldOverlay>);

    #[async_trait]
    impl FieldOverlayRepository for Stub {
        async fn list(&self, _: Option<&str>, _: bool) -> Result<Vec<FieldOverlay>, DomainError> {
            Ok(self.0.clone())
        }
        async fn find(&self, _: &str, _: &str) -> Result<Option<FieldOverlay>, DomainError> {
            Ok(None)
        }
        async fn save(&self, value: &FieldOverlay) -> Result<FieldOverlay, DomainError> {
            Ok(value.clone())
        }
        async fn set_active(&self, _: &str, _: &str, _: bool) -> Result<Option<FieldOverlay>, DomainError> {
            Ok(None)
        }
    }

    fn overlay(name: &str, ty: &str) -> FieldOverlay {
        FieldOverlay {
            object_name: "Team".into(),
            field_name: name.into(),
            field_type: ty.into(),
            catalog_code: None,
            object_name_target: None,
            required: false,
            list_visible: true,
            filterable: false,
            widget: None,
            description: None,
            visible_when: None,
            max_length: None,
            min: None,
            max: None,
            is_active: true,
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn enforces_required_overlay_fields() {
        let mut required = overlay("code", "string");
        required.required = true;
        let repo: Arc<dyn FieldOverlayRepository + Send + Sync> = Arc::new(Stub(vec![required]));
        assert!(validate_attributes("Team", serde_json::json!({}), Some(&repo))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn rejects_unknown_and_deduplicates_object_refs() {
        let repo: Arc<dyn FieldOverlayRepository + Send + Sync> = Arc::new(Stub(vec![overlay("refs", "object_ref[]")]));
        let value = validate_attributes("Team", serde_json::json!({"refs": ["a", "a", "b"]}), Some(&repo))
            .await
            .unwrap();
        assert_eq!(value["refs"], serde_json::json!(["a", "b"]));
        assert!(
            validate_attributes("Team", serde_json::json!({"other": 1}), Some(&repo))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn ignores_hidden_values_and_does_not_require_hidden_fields() {
        let mut child = overlay("child", "string");
        child.visible_when = Some(serde_json::json!({"field": "enabled", "op": "eq", "value": true}));
        child.required = true;
        let repo: Arc<dyn FieldOverlayRepository + Send + Sync> =
            Arc::new(Stub(vec![overlay("enabled", "boolean"), child]));
        let value = validate_attributes(
            "Team",
            serde_json::json!({"enabled": false, "child": "must-be-dropped"}),
            Some(&repo),
        )
        .await
        .unwrap();
        assert_eq!(value, serde_json::json!({"enabled": false}));
    }
}
