use fms_domain::error::DomainError;
use fms_domain::models::field_overlay::{FieldOverlay, OntologyFieldType};
use fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema;
use fms_domain::ports::field_overlay_repository::FieldOverlayRepository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldOverlayWrite {
    pub object_name: String,
    pub field_name: String,
    pub field_type: String,
    pub catalog_code: Option<String>,
    pub object_name_target: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub list_visible: bool,
    #[serde(default)]
    pub filterable: bool,
    pub widget: Option<String>,
    pub description: Option<String>,
    pub visible_when: Option<Value>,
    pub max_length: Option<i32>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

pub struct FieldOverlayService<R: FieldOverlayRepository + ?Sized> {
    repo: Arc<R>,
}

impl<R: FieldOverlayRepository + ?Sized> FieldOverlayService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    pub async fn list(
        &self,
        object_name: Option<&str>,
        include_inactive: bool,
    ) -> Result<Vec<FieldOverlay>, DomainError> {
        self.repo.list(object_name, include_inactive).await
    }

    pub async fn save(&self, payload: FieldOverlayWrite) -> Result<FieldOverlay, DomainError> {
        let object_name = payload.object_name.trim().to_string();
        let field_name = payload.field_name.trim().to_string();
        if object_name.is_empty() || field_name.is_empty() {
            return Err(DomainError::ValidationError("对象名和字段名不能为空".into()));
        }
        let ty = OntologyFieldType::parse(payload.field_type.trim())
            .ok_or_else(|| DomainError::ValidationError("不支持的字段类型".into()))?;
        let schema = build_flight_ops_v1_schema();
        let object = schema
            .objects
            .get(&object_name)
            .ok_or_else(|| DomainError::ValidationError(format!("未知本体对象: {object_name}")))?;
        if let Some(visible_when) = payload.visible_when.as_ref() {
            validate_visible_when(
                visible_when,
                &field_name,
                object,
                &self.repo.list(Some(&object_name), false).await?,
            )?;
        }
        if let Some(target) = &payload.object_name_target {
            if !schema.objects.contains_key(target) {
                return Err(DomainError::ValidationError(format!("未知对象引用目标: {target}")));
            }
        }
        if let Some(core) = object.fields.get(&field_name) {
            if !core.field_type.eq_ignore_ascii_case(&payload.field_type) {
                return Err(DomainError::Conflict("核心字段不能修改类型".into()));
            }
        }
        if ty.is_catalog() == payload.catalog_code.is_none() {
            return Err(DomainError::ValidationError(
                "catalog_ref 必须且只能指定 catalog_code".into(),
            ));
        }
        if ty.is_object() == payload.object_name_target.is_none() {
            return Err(DomainError::ValidationError(
                "object_ref 必须且只能指定 object_name_target".into(),
            ));
        }
        if payload.max_length.is_some() && !matches!(ty, OntologyFieldType::String) {
            return Err(DomainError::ValidationError("max_length 仅适用于 string".into()));
        }
        let existing = self.repo.find(&object_name, &field_name).await?;
        if let Some(existing) = &existing {
            if existing.field_type != ty.as_str() {
                return Err(DomainError::Conflict(
                    "字段类型不可修改，请停用旧字段后新增新类型字段".into(),
                ));
            }
        }
        let overlay = FieldOverlay {
            object_name,
            field_name,
            field_type: ty.as_str().into(),
            catalog_code: payload.catalog_code,
            object_name_target: payload.object_name_target,
            required: payload.required,
            list_visible: payload.list_visible,
            filterable: payload.filterable,
            widget: payload.widget,
            description: payload.description,
            visible_when: payload.visible_when,
            max_length: payload.max_length,
            min: payload.min,
            max: payload.max,
            is_active: existing.as_ref().map(|x| x.is_active).unwrap_or(true),
            created_at: existing.as_ref().and_then(|x| x.created_at),
            updated_at: None,
        };
        self.repo.save(&overlay).await
    }

    pub async fn set_active(
        &self,
        object_name: &str,
        field_name: &str,
        is_active: bool,
    ) -> Result<FieldOverlay, DomainError> {
        self.repo
            .set_active(object_name, field_name, is_active)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "ontology_field_overlay",
                id: format!("{object_name}.{field_name}"),
            })
    }
}

fn validate_visible_when(
    value: &Value,
    field_name: &str,
    object: &fms_domain::models::ai_ontology::OntologyObjectDef,
    overlays: &[FieldOverlay],
) -> Result<(), DomainError> {
    let Some(map) = value.as_object() else {
        return Err(DomainError::ValidationError(
            "visible_when 必须是 { field, op, value } 对象".into(),
        ));
    };
    let dependency = map
        .get("field")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let dependency =
        dependency.ok_or_else(|| DomainError::ValidationError("visible_when.field 必须是非空字符串".into()))?;
    if dependency == field_name {
        return Err(DomainError::ValidationError("visible_when.field 不能引用自身".into()));
    }
    let known = object.fields.contains_key(dependency) || overlays.iter().any(|item| item.field_name == dependency);
    if !known {
        return Err(DomainError::ValidationError(format!(
            "visible_when 引用了未知字段: {dependency}"
        )));
    }
    let op = map.get("op").and_then(Value::as_str).map(str::trim).unwrap_or("eq");
    if !matches!(op, "eq" | "neq" | "in" | "not_in" | "gt" | "gte" | "lt" | "lte") {
        return Err(DomainError::ValidationError(format!("visible_when.op 不支持: {op}")));
    }
    if !map.contains_key("value") {
        return Err(DomainError::ValidationError("visible_when.value 不能为空".into()));
    }
    Ok(())
}
