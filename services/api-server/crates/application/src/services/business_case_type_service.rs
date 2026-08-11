use std::sync::Arc;

use chrono::Utc;

use fms_domain::error::DomainError;
use fms_domain::models::business_case::{BusinessCaseType, VisibilityScope};
use fms_domain::ports::business_case_repository::BusinessCaseTypeRepository;

pub struct BusinessCaseTypeService {
    repo: Arc<dyn BusinessCaseTypeRepository + Send + Sync>,
}

impl BusinessCaseTypeService {
    pub fn new(repo: Arc<dyn BusinessCaseTypeRepository + Send + Sync>) -> Self {
        Self { repo }
    }

    pub async fn list_case_types(&self, active_only: bool) -> Result<Vec<BusinessCaseType>, DomainError> {
        self.repo.find_all_scoped(active_only, None, None, true).await
    }

    pub async fn list_case_types_for_viewer(
        &self,
        active_only: bool,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Vec<BusinessCaseType>, DomainError> {
        self.repo
            .find_all_scoped(active_only, viewer_department_id, viewer_department_name, true)
            .await
    }

    pub async fn find_by_code(&self, code: &str) -> Result<Option<BusinessCaseType>, DomainError> {
        self.repo.find_by_code(code.trim()).await
    }

    pub async fn find_by_code_for_viewer(
        &self,
        code: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        self.repo
            .find_by_code_scoped(code.trim(), viewer_department_id, viewer_department_name, true)
            .await
    }

    pub async fn create_case_type(
        &self,
        code: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<BusinessCaseType, DomainError> {
        let normalized_code = code.trim();
        let normalized_name = name.trim();
        if normalized_code.is_empty() {
            return Err(DomainError::ValidationError("code is required".into()));
        }
        if normalized_name.is_empty() {
            return Err(DomainError::ValidationError("name is required".into()));
        }

        self.repo
            .save(&BusinessCaseType {
                id: ulid::Ulid::new().to_string(),
                code: normalized_code.to_string(),
                name: normalized_name.to_string(),
                bpmn_xml: None,
                description: description
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                is_active: true,
                visibility_scope: VisibilityScope::Common,
                department_id: None,
                department_name_snapshot: None,
                created_at: Some(Utc::now()),
                updated_at: Some(Utc::now()),
                ai_extraction_config: serde_json::json!({}),
                case_properties: serde_json::json!({}),
            })
            .await
    }

    pub async fn create_case_type_for_viewer(
        &self,
        code: &str,
        name: &str,
        description: Option<&str>,
        visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<BusinessCaseType, DomainError> {
        let normalized_code = code.trim();
        let normalized_name = name.trim();
        if normalized_code.is_empty() {
            return Err(DomainError::ValidationError("code is required".into()));
        }
        if normalized_name.is_empty() {
            return Err(DomainError::ValidationError("name is required".into()));
        }

        let department_id = normalize_optional_scope_value(viewer_department_id);
        let department_name_snapshot = normalize_optional_scope_value(viewer_department_name);
        if visibility_scope == VisibilityScope::Department
            && department_id.is_none()
            && department_name_snapshot.is_none()
        {
            return Err(DomainError::ValidationError(
                "当前用户未绑定业务部门，无法创建部门事项类型".into(),
            ));
        }

        self.repo
            .save(&BusinessCaseType {
                id: ulid::Ulid::new().to_string(),
                code: normalized_code.to_string(),
                name: normalized_name.to_string(),
                bpmn_xml: None,
                description: description
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                is_active: true,
                visibility_scope,
                department_id: if visibility_scope == VisibilityScope::Department {
                    department_id
                } else {
                    None
                },
                department_name_snapshot: if visibility_scope == VisibilityScope::Department {
                    department_name_snapshot
                } else {
                    None
                },
                created_at: Some(Utc::now()),
                updated_at: Some(Utc::now()),
                ai_extraction_config: serde_json::json!({}),
                case_properties: serde_json::json!({}),
            })
            .await
    }

    pub async fn save_bpmn_xml(
        &self,
        code: &str,
        bpmn_xml: &str,
        description: Option<&str>,
    ) -> Result<bool, DomainError> {
        self.repo.update_bpmn_xml(code.trim(), bpmn_xml, description).await
    }

    pub async fn save_bpmn_xml_if_accessible(
        &self,
        code: &str,
        bpmn_xml: &str,
        description: Option<&str>,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        can_manage_common: bool,
    ) -> Result<bool, DomainError> {
        let Some(entity) = self
            .repo
            .find_by_code_scoped(code.trim(), viewer_department_id, viewer_department_name, true)
            .await?
        else {
            return Ok(false);
        };

        if !can_edit_case_type(&entity, viewer_department_id, viewer_department_name, can_manage_common) {
            return Err(DomainError::PermissionDenied(
                "无权修改其他部门或通用业务事项类型".into(),
            ));
        }

        self.repo.update_bpmn_xml(code.trim(), bpmn_xml, description).await
    }

    pub async fn update_status(&self, code: &str, is_active: bool) -> Result<bool, DomainError> {
        self.repo.update_status(code.trim(), is_active).await
    }

    pub async fn update_status_if_accessible(
        &self,
        code: &str,
        is_active: bool,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        can_manage_common: bool,
    ) -> Result<bool, DomainError> {
        let Some(entity) = self
            .repo
            .find_by_code_scoped(code.trim(), viewer_department_id, viewer_department_name, true)
            .await?
        else {
            return Ok(false);
        };

        if !can_edit_case_type(&entity, viewer_department_id, viewer_department_name, can_manage_common) {
            return Err(DomainError::PermissionDenied(
                "无权修改其他部门或通用业务事项类型".into(),
            ));
        }

        self.repo.update_status(code.trim(), is_active).await
    }

    pub async fn update_ai_extraction_config_if_accessible(
        &self,
        code: &str,
        config: serde_json::Value,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        can_manage_common: bool,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        validate_ai_extraction_config(&config)?;
        let Some(entity) = self
            .repo
            .find_by_code_scoped(code.trim(), viewer_department_id, viewer_department_name, true)
            .await?
        else {
            return Ok(None);
        };

        if !can_edit_case_type(&entity, viewer_department_id, viewer_department_name, can_manage_common) {
            return Err(DomainError::PermissionDenied(
                "无权修改其他部门或通用业务事项类型".into(),
            ));
        }

        self.repo.update_ai_extraction_config(code.trim(), &config).await
    }

    pub async fn update_case_properties_if_accessible(
        &self,
        code: &str,
        properties: serde_json::Value,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        can_manage_common: bool,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        validate_case_properties(&properties)?;
        let Some(entity) = self
            .repo
            .find_by_code_scoped(code.trim(), viewer_department_id, viewer_department_name, true)
            .await?
        else {
            return Ok(None);
        };

        if !can_edit_case_type(&entity, viewer_department_id, viewer_department_name, can_manage_common) {
            return Err(DomainError::PermissionDenied(
                "无权修改其他部门或通用业务事项类型".into(),
            ));
        }

        self.repo.update_case_properties(code.trim(), &properties).await
    }
}

fn normalize_optional_scope_value(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|item| !item.is_empty()).map(str::to_string)
}

fn can_edit_case_type(
    entity: &BusinessCaseType,
    viewer_department_id: Option<&str>,
    viewer_department_name: Option<&str>,
    can_manage_common: bool,
) -> bool {
    match entity.visibility_scope {
        VisibilityScope::Common => can_manage_common,
        VisibilityScope::Department => {
            let viewer_department_id = normalize_optional_scope_value(viewer_department_id);
            let viewer_department_name = normalize_optional_scope_value(viewer_department_name);

            entity.department_id.is_some() && entity.department_id == viewer_department_id
                || entity.department_name_snapshot.is_some()
                    && entity.department_name_snapshot == viewer_department_name
        }
    }
}

fn validate_ai_extraction_config(config: &serde_json::Value) -> Result<(), DomainError> {
    if !config.is_object() {
        return Err(DomainError::ValidationError(
            "ai_extraction_config must be an object".into(),
        ));
    }
    if let Some(enabled) = config.get("enabled") {
        if !enabled.is_boolean() {
            return Err(DomainError::ValidationError(
                "ai_extraction_config.enabled must be boolean".into(),
            ));
        }
    }

    // fields 校验
    if let Some(fields) = config.get("fields") {
        let Some(fields_obj) = fields.as_object() else {
            return Err(DomainError::ValidationError(
                "ai_extraction_config.fields must be object".into(),
            ));
        };
        for (field_name, field_val) in fields_obj {
            let Some(field_cfg) = field_val.as_object() else {
                return Err(DomainError::ValidationError(format!(
                    "fields.{} must be object",
                    field_name
                )));
            };
            if let Some(ty_val) = field_cfg.get("type") {
                let Some(ty_str) = ty_val.as_str() else {
                    return Err(DomainError::ValidationError(format!(
                        "fields.{}.type must be string",
                        field_name
                    )));
                };
                if ty_str != "string" && ty_str != "number" && ty_str != "boolean" && ty_str != "enum" {
                    return Err(DomainError::ValidationError(format!(
                        "fields.{}.type must be string/number/boolean/enum",
                        field_name
                    )));
                }
                if ty_str == "enum" {
                    let Some(enum_vals) = field_cfg.get("enum_values") else {
                        return Err(DomainError::ValidationError(format!(
                            "fields.{} has enum type but lacks enum_values",
                            field_name
                        )));
                    };
                    let Some(arr) = enum_vals.as_array() else {
                        return Err(DomainError::ValidationError(format!(
                            "fields.{}.enum_values must be array",
                            field_name
                        )));
                    };
                    if arr.is_empty() {
                        return Err(DomainError::ValidationError(format!(
                            "fields.{}.enum_values cannot be empty",
                            field_name
                        )));
                    }
                    for item in arr {
                        if !item.is_string() {
                            return Err(DomainError::ValidationError(format!(
                                "fields.{}.enum_values items must be strings",
                                field_name
                            )));
                        }
                    }
                }
            }
        }
    }

    // leg_binding 校验
    if let Some(leg_binding) = config.get("leg_binding") {
        let Some(leg_binding_obj) = leg_binding.as_object() else {
            return Err(DomainError::ValidationError(
                "ai_extraction_config.leg_binding must be object".into(),
            ));
        };
        if let Some(allowed) = leg_binding_obj.get("allowed") {
            let Some(arr) = allowed.as_array() else {
                return Err(DomainError::ValidationError("leg_binding.allowed must be array".into()));
            };
            for val in arr {
                let Some(s) = val.as_str() else {
                    return Err(DomainError::ValidationError(
                        "leg_binding.allowed values must be string".into(),
                    ));
                };
                if s != "inbound" && s != "outbound" {
                    return Err(DomainError::ValidationError(
                        "leg_binding.allowed values must be inbound/outbound".into(),
                    ));
                }
            }
        }
        if let Some(default) = leg_binding_obj.get("default") {
            if !default.is_null() {
                let Some(s) = default.as_str() else {
                    return Err(DomainError::ValidationError(
                        "leg_binding.default must be string or null".into(),
                    ));
                };
                if s != "inbound" && s != "outbound" && s != "unknown" {
                    return Err(DomainError::ValidationError(
                        "leg_binding.default must be inbound/outbound/unknown".into(),
                    ));
                }
            }
        }
    }

    // flight_matching 校验
    if let Some(flight_matching) = config.get("flight_matching") {
        let Some(fm_obj) = flight_matching.as_object() else {
            return Err(DomainError::ValidationError(
                "ai_extraction_config.flight_matching must be object".into(),
            ));
        };
        if let Some(min_score) = fm_obj.get("min_auto_match_score") {
            if !min_score.is_null() {
                let Some(val) = min_score.as_f64() else {
                    return Err(DomainError::ValidationError(
                        "flight_matching.min_auto_match_score must be number".into(),
                    ));
                };
                if !(0.0..=1.0).contains(&val) {
                    return Err(DomainError::ValidationError(
                        "flight_matching.min_auto_match_score must be between 0 and 1".into(),
                    ));
                }
            }
        }
        if let Some(before) = fm_obj.get("window_hours_before") {
            if !before.is_null() {
                let Some(val) = before.as_i64() else {
                    return Err(DomainError::ValidationError(
                        "flight_matching.window_hours_before must be integer".into(),
                    ));
                };
                if val < 0 {
                    return Err(DomainError::ValidationError(
                        "flight_matching.window_hours_before cannot be negative".into(),
                    ));
                }
            }
        }
        if let Some(after) = fm_obj.get("window_hours_after") {
            if !after.is_null() {
                let Some(val) = after.as_i64() else {
                    return Err(DomainError::ValidationError(
                        "flight_matching.window_hours_after must be integer".into(),
                    ));
                };
                if val < 0 {
                    return Err(DomainError::ValidationError(
                        "flight_matching.window_hours_after cannot be negative".into(),
                    ));
                }
            }
        }
    }

    Ok(())
}

fn validate_case_properties(properties: &serde_json::Value) -> Result<(), DomainError> {
    if !properties.is_object() {
        return Err(DomainError::ValidationError("case_properties must be an object".into()));
    }

    // binding_policy 校验
    if let Some(binding_policy) = properties.get("binding_policy") {
        let Some(bp_obj) = binding_policy.as_object() else {
            return Err(DomainError::ValidationError(
                "case_properties.binding_policy must be object".into(),
            ));
        };
        if let Some(allowed) = bp_obj.get("allowed_leg_types") {
            let Some(arr) = allowed.as_array() else {
                return Err(DomainError::ValidationError(
                    "binding_policy.allowed_leg_types must be array".into(),
                ));
            };
            for val in arr {
                let Some(s) = val.as_str() else {
                    return Err(DomainError::ValidationError(
                        "binding_policy.allowed_leg_types values must be string".into(),
                    ));
                };
                if s != "inbound" && s != "outbound" {
                    return Err(DomainError::ValidationError(
                        "binding_policy.allowed_leg_types values must be inbound/outbound".into(),
                    ));
                }
            }
        }
        if let Some(default) = bp_obj.get("default_leg_type") {
            if !default.is_null() {
                let Some(s) = default.as_str() else {
                    return Err(DomainError::ValidationError(
                        "binding_policy.default_leg_type must be string or null".into(),
                    ));
                };
                if s != "inbound" && s != "outbound" {
                    return Err(DomainError::ValidationError(
                        "binding_policy.default_leg_type must be inbound/outbound".into(),
                    ));
                }
            }
        }
    }

    // extra_info_schema 校验
    if let Some(extra_info) = properties.get("extra_info_schema") {
        let Some(ei_obj) = extra_info.as_object() else {
            return Err(DomainError::ValidationError(
                "case_properties.extra_info_schema must be object".into(),
            ));
        };
        if let Some(fields) = ei_obj.get("fields") {
            let Some(fields_obj) = fields.as_object() else {
                return Err(DomainError::ValidationError(
                    "extra_info_schema.fields must be object".into(),
                ));
            };
            for (field_name, field_val) in fields_obj {
                let Some(field_cfg) = field_val.as_object() else {
                    return Err(DomainError::ValidationError(format!(
                        "extra_info_schema.fields.{} must be object",
                        field_name
                    )));
                };
                if let Some(ty_val) = field_cfg.get("type") {
                    let Some(ty_str) = ty_val.as_str() else {
                        return Err(DomainError::ValidationError(format!(
                            "extra_info_schema.fields.{}.type must be string",
                            field_name
                        )));
                    };
                    if ty_str != "string"
                        && ty_str != "number"
                        && ty_str != "boolean"
                        && ty_str != "enum"
                        && ty_str != "date"
                        && ty_str != "datetime"
                    {
                        return Err(DomainError::ValidationError(format!(
                            "extra_info_schema.fields.{}.type must be string/number/boolean/enum/date/datetime",
                            field_name
                        )));
                    }
                    if ty_str == "enum" {
                        let Some(enum_vals) = field_cfg.get("enum_values") else {
                            return Err(DomainError::ValidationError(format!(
                                "extra_info_schema.fields.{} has enum type but lacks enum_values",
                                field_name
                            )));
                        };
                        let Some(arr) = enum_vals.as_array() else {
                            return Err(DomainError::ValidationError(format!(
                                "extra_info_schema.fields.{}.enum_values must be array",
                                field_name
                            )));
                        };
                        if arr.is_empty() {
                            return Err(DomainError::ValidationError(format!(
                                "extra_info_schema.fields.{}.enum_values cannot be empty",
                                field_name
                            )));
                        }
                    }
                }
            }
        }
    }

    // workflow_policy 校验
    if let Some(workflow_policy) = properties.get("workflow_policy") {
        let Some(wp_obj) = workflow_policy.as_object() else {
            return Err(DomainError::ValidationError(
                "case_properties.workflow_policy must be object".into(),
            ));
        };
        if let Some(receipt_mode) = wp_obj.get("batch_receipt_mode") {
            let Some(s) = receipt_mode.as_str() else {
                return Err(DomainError::ValidationError(
                    "workflow_policy.batch_receipt_mode must be string".into(),
                ));
            };
            if s != "shared_group" && s != "per_case" {
                return Err(DomainError::ValidationError(
                    "workflow_policy.batch_receipt_mode must be shared_group/per_case".into(),
                ));
            }
        }
    }

    // duplicate_policy 校验
    if let Some(duplicate_policy) = properties.get("duplicate_policy") {
        let Some(dp_obj) = duplicate_policy.as_object() else {
            return Err(DomainError::ValidationError(
                "case_properties.duplicate_policy must be object".into(),
            ));
        };
        if let Some(fields) = dp_obj.get("fields") {
            let Some(arr) = fields.as_array() else {
                return Err(DomainError::ValidationError(
                    "duplicate_policy.fields must be array".into(),
                ));
            };
            for val in arr {
                let Some(s) = val.as_str() else {
                    return Err(DomainError::ValidationError(
                        "duplicate_policy.fields values must be string".into(),
                    ));
                };
                if s.trim().is_empty() {
                    return Err(DomainError::ValidationError(
                        "duplicate_policy.fields values cannot be empty".into(),
                    ));
                }
            }
        }
        if let Some(active_statuses) = dp_obj.get("active_statuses") {
            let Some(arr) = active_statuses.as_array() else {
                return Err(DomainError::ValidationError(
                    "duplicate_policy.active_statuses must be array".into(),
                ));
            };
            for val in arr {
                let Some(s) = val.as_str() else {
                    return Err(DomainError::ValidationError(
                        "duplicate_policy.active_statuses values must be string".into(),
                    ));
                };
                if s.trim().is_empty() {
                    return Err(DomainError::ValidationError(
                        "duplicate_policy.active_statuses values cannot be empty".into(),
                    ));
                }
            }
        }
    }

    Ok(())
}
