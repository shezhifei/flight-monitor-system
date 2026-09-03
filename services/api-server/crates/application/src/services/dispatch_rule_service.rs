//! 部门资质与作业类型规则服务。

use chrono::Utc;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::schemas::dispatch_schemas::{
    DepartmentQualificationCatalogCreate, DepartmentQualificationLevelCreate, DepartmentTaskTypeRequirementDraftCreate,
    DepartmentTaskTypeRequirementPublishRequest, DispatchRulePreviewRequest, FlightGenerationRuleCreate,
    GenerationAdjustmentRuleCreate, QualificationGrantCreate, TemporaryTaskTemplateCreate,
};
use crate::services::attribute_validation::collect_attribute_references;
use crate::services::attribute_validation::validate_attributes;
use crate::services::attribute_validation::ObjectReferenceValidator;
use crate::services::qualification_writer::QualificationAttributeTransactionalWriter;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    validate_completion_warning_lead_minutes, DepartmentQualificationCatalog, DepartmentQualificationLevel,
    DepartmentRuleStatus, DepartmentTaskTypeRequirementVersion, DispatchPublicationState, FlightGenerationRule,
    GenerationAdjustmentRule, LegScope, PublishTriggerMode, QualificationGrant, QualificationGrantStatus,
    TaskTypeCrewSlotRequirement, TaskTypeEquipmentRequirement, TemporaryTaskTemplate, TurnaroundConstraintMode,
    TurnaroundContinuityRule, TurnaroundSlotPair,
};
use fms_domain::ports::dispatch_repository::{
    DepartmentQualificationRepository, DepartmentRepository, DepartmentTaskTypeRequirementRepository,
    FlightGenerationRuleRepository, GenerationAdjustmentRuleRepository, QualificationGrantRepository,
    TemporaryTaskTemplateRepository,
};
use fms_domain::ports::field_overlay_repository::FieldOverlayRepository;

pub struct DispatchRuleService {
    department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
    qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
    qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
    task_type_requirement_repo: Arc<dyn DepartmentTaskTypeRequirementRepository + Send + Sync>,
    generation_rule_repo: Arc<dyn FlightGenerationRuleRepository + Send + Sync>,
    adjustment_rule_repo: Arc<dyn GenerationAdjustmentRuleRepository + Send + Sync>,
    temporary_task_template_repo: Arc<dyn TemporaryTaskTemplateRepository + Send + Sync>,
    field_overlay_repo: Option<Arc<dyn FieldOverlayRepository + Send + Sync>>,
    object_reference_validator: Option<Arc<dyn ObjectReferenceValidator>>,
    qualification_writer: Option<Arc<dyn QualificationAttributeTransactionalWriter>>,
}

impl DispatchRuleService {
    pub fn new(
        department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
        qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
        qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
        task_type_requirement_repo: Arc<dyn DepartmentTaskTypeRequirementRepository + Send + Sync>,
        generation_rule_repo: Arc<dyn FlightGenerationRuleRepository + Send + Sync>,
        adjustment_rule_repo: Arc<dyn GenerationAdjustmentRuleRepository + Send + Sync>,
        temporary_task_template_repo: Arc<dyn TemporaryTaskTemplateRepository + Send + Sync>,
    ) -> Self {
        Self {
            department_repo,
            qualification_repo,
            qualification_grant_repo,
            task_type_requirement_repo,
            generation_rule_repo,
            adjustment_rule_repo,
            temporary_task_template_repo,
            field_overlay_repo: None,
            object_reference_validator: None,
            qualification_writer: None,
        }
    }

    pub fn with_field_overlay_repository(mut self, repo: Arc<dyn FieldOverlayRepository + Send + Sync>) -> Self {
        self.field_overlay_repo = Some(repo);
        self
    }

    pub fn with_object_reference_validator(mut self, validator: Arc<dyn ObjectReferenceValidator>) -> Self {
        self.object_reference_validator = Some(validator);
        self
    }

    pub fn with_qualification_writer(mut self, writer: Arc<dyn QualificationAttributeTransactionalWriter>) -> Self {
        self.qualification_writer = Some(writer);
        self
    }

    pub async fn create_qualification(
        &self,
        department_id: &str,
        payload: DepartmentQualificationCatalogCreate,
    ) -> Result<DepartmentQualificationCatalog, DomainError> {
        self.ensure_department(department_id).await?;
        let attributes =
            validate_attributes("Qualification", payload.attributes, self.field_overlay_repo.as_ref()).await?;
        if let Some(validator) = self.object_reference_validator.as_ref() {
            validator.validate("Qualification", &attributes).await?;
        }
        let item = DepartmentQualificationCatalog {
            id: ulid::Ulid::new().to_string(),
            department_id: department_id.to_string(),
            qualification_code: require_non_empty(&payload.qualification_code, "qualification_code")?,
            qualification_name: require_non_empty(&payload.qualification_name, "qualification_name")?,
            description: normalize_optional_string(payload.description),
            is_active: payload.is_active,
            created_at: None,
            updated_at: None,
            attributes,
        };
        // writer 路径：目录行 + reference index 同一 UnitOfWork 提交；
        // 无 writer 时保持原仓储直写行为（validator 已做引用校验）。
        if let Some(writer) = self.qualification_writer.as_ref() {
            let references = collect_attribute_references(
                "Qualification",
                &item.id,
                &item.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_catalog_with_references(&item, &references).await;
        }
        self.qualification_repo.save_catalog(&item).await
    }

    pub async fn list_qualifications(
        &self,
        department_id: &str,
        include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationCatalog>, DomainError> {
        self.ensure_department(department_id).await?;
        self.qualification_repo
            .list_catalogs(department_id, include_inactive)
            .await
    }

    pub async fn create_level(
        &self,
        department_id: &str,
        payload: DepartmentQualificationLevelCreate,
    ) -> Result<DepartmentQualificationLevel, DomainError> {
        self.ensure_department(department_id).await?;
        let item = DepartmentQualificationLevel {
            id: ulid::Ulid::new().to_string(),
            department_id: department_id.to_string(),
            qualification_code: require_non_empty(&payload.qualification_code, "qualification_code")?,
            level_code: require_non_empty(&payload.level_code, "level_code")?,
            level_name: require_non_empty(&payload.level_name, "level_name")?,
            level_rank: payload.level_rank,
            covered_level_codes: normalize_string_list(payload.covered_level_codes),
            is_active: payload.is_active,
            created_at: None,
            updated_at: None,
        };
        self.qualification_repo.save_level(&item).await
    }

    pub async fn list_levels(
        &self,
        department_id: &str,
        qualification_code: Option<&str>,
        include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationLevel>, DomainError> {
        self.ensure_department(department_id).await?;
        self.qualification_repo
            .list_levels(
                department_id,
                normalize_optional_ref(qualification_code),
                include_inactive,
            )
            .await
    }

    pub async fn create_grant(
        &self,
        department_id: &str,
        payload: QualificationGrantCreate,
    ) -> Result<QualificationGrant, DomainError> {
        self.ensure_department(department_id).await?;
        let item = QualificationGrant {
            id: ulid::Ulid::new().to_string(),
            user_id: require_non_empty(&payload.user_id, "user_id")?,
            department_id: department_id.to_string(),
            qualification_code: require_non_empty(&payload.qualification_code, "qualification_code")?,
            level_code: require_non_empty(&payload.level_code, "level_code")?,
            valid_from: payload.valid_from,
            valid_to: payload.valid_to,
            status: parse_grant_status(&payload.status)?,
            source_team_id: normalize_optional_string(payload.source_team_id),
            metadata: payload.metadata,
            created_at: None,
            updated_at: None,
        };
        self.qualification_grant_repo.save(&item).await
    }

    pub async fn list_grants(
        &self,
        department_id: &str,
        user_ids: &[String],
        include_inactive: bool,
        at_time: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<QualificationGrant>, DomainError> {
        self.ensure_department(department_id).await?;
        self.qualification_grant_repo
            .find_by_department(department_id, at_time, user_ids, include_inactive)
            .await
    }

    pub async fn save_requirement_draft(
        &self,
        department_id: &str,
        payload: DepartmentTaskTypeRequirementDraftCreate,
    ) -> Result<DepartmentTaskTypeRequirementVersion, DomainError> {
        self.ensure_department(department_id).await?;
        let task_type = require_non_empty(&payload.task_type, "task_type")?;
        let crew_requirements = self.normalize_crew_requirements(payload.requirements, payload.crew_requirements)?;
        let equipment_requirements = payload
            .equipment_requirements
            .into_iter()
            .map(|item| TaskTypeEquipmentRequirement {
                slot_code: item.slot_code.trim().to_string(),
                equipment_type_id: normalize_optional_string(item.equipment_type_id),
                equipment_type_code: normalize_optional_string(item.equipment_type_code),
                required_count: item.required_count.max(1),
                must_be_distinct: item.must_be_distinct,
                requires_driver: item.requires_driver,
                driver_qualification_code: normalize_optional_string(item.driver_qualification_code),
                driver_min_level_code: normalize_optional_string(item.driver_min_level_code),
                remarks: normalize_optional_string(item.remarks),
            })
            .filter(|item| !item.slot_code.is_empty())
            .collect::<Vec<_>>();
        let turnaround_continuity_rules = payload
            .turnaround_continuity_rules
            .into_iter()
            .filter(|item| !item.counterpart_task_type.trim().is_empty())
            .map(|item| {
                Ok(TurnaroundContinuityRule {
                    enabled: item.enabled,
                    counterpart_leg_scope: parse_leg_scope(&item.counterpart_leg_scope)?,
                    counterpart_task_type: item.counterpart_task_type.trim().to_string(),
                    slot_pairs: item
                        .slot_pairs
                        .into_iter()
                        .filter(|pair| {
                            !pair.inbound_slot_code.trim().is_empty() && !pair.outbound_slot_code.trim().is_empty()
                        })
                        .map(|pair| TurnaroundSlotPair {
                            inbound_slot_code: pair.inbound_slot_code.trim().to_string(),
                            outbound_slot_code: pair.outbound_slot_code.trim().to_string(),
                        })
                        .collect(),
                    constraint_mode: parse_turnaround_constraint_mode(&item.constraint_mode)?,
                    tight_threshold_minutes: item.tight_threshold_minutes,
                    relax_threshold_minutes: item.relax_threshold_minutes,
                    flight_filters: item.flight_filters,
                    aircraft_type_filters: normalize_string_list(item.aircraft_type_filters),
                    notes: normalize_optional_string(item.notes),
                })
            })
            .collect::<Result<Vec<_>, DomainError>>()?;

        if let Some(mut existing) = self
            .task_type_requirement_repo
            .find_latest_draft(department_id, &task_type)
            .await?
        {
            existing.requirements = crew_requirements.clone();
            existing.crew_requirements = crew_requirements;
            existing.equipment_requirements = equipment_requirements;
            existing.turnaround_continuity_rules = turnaround_continuity_rules;
            existing.notes = normalize_optional_string(payload.notes);
            existing.status = DepartmentRuleStatus::Draft;
            return self.task_type_requirement_repo.save(&existing).await;
        }

        let version = DepartmentTaskTypeRequirementVersion {
            id: ulid::Ulid::new().to_string(),
            department_id: department_id.to_string(),
            task_type: task_type.clone(),
            version_no: self
                .task_type_requirement_repo
                .next_version_no(department_id, &task_type)
                .await?,
            status: DepartmentRuleStatus::Draft,
            requirements: crew_requirements.clone(),
            crew_requirements,
            equipment_requirements,
            turnaround_continuity_rules,
            notes: normalize_optional_string(payload.notes),
            published_at: None,
            created_at: None,
            updated_at: None,
        };
        self.task_type_requirement_repo.save(&version).await
    }

    pub async fn list_requirement_versions(
        &self,
        department_id: &str,
        task_type: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<DepartmentTaskTypeRequirementVersion>, DomainError> {
        self.ensure_department(department_id).await?;
        self.task_type_requirement_repo
            .list_versions(
                department_id,
                normalize_optional_ref(task_type),
                normalize_optional_ref(status),
            )
            .await
    }

    pub async fn publish_requirement(
        &self,
        department_id: &str,
        payload: DepartmentTaskTypeRequirementPublishRequest,
    ) -> Result<DepartmentTaskTypeRequirementVersion, DomainError> {
        self.ensure_department(department_id).await?;
        let task_type = require_non_empty(&payload.task_type, "task_type")?;
        let mut draft = if let Some(draft_id) = normalize_optional_string(payload.draft_id) {
            let Some(version) = self.task_type_requirement_repo.find_by_id(&draft_id).await? else {
                return Err(DomainError::NotFound {
                    entity_type: "task_type_requirement_draft",
                    id: draft_id,
                });
            };
            if version.department_id != department_id || version.task_type != task_type {
                return Err(DomainError::NotFound {
                    entity_type: "task_type_requirement_draft",
                    id: version.id,
                });
            }
            version
        } else {
            self.task_type_requirement_repo
                .find_latest_draft(department_id, &task_type)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "task_type_requirement_draft",
                    id: format!("{department_id}:{task_type}"),
                })?
        };

        self.task_type_requirement_repo
            .archive_published(department_id, &task_type)
            .await?;
        draft.status = DepartmentRuleStatus::Published;
        draft.published_at = Some(Utc::now());
        self.task_type_requirement_repo.save(&draft).await
    }

    pub async fn list_generation_rules(
        &self,
        department_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<FlightGenerationRule>, DomainError> {
        self.ensure_department(department_id).await?;
        self.generation_rule_repo
            .list_rules(department_id, normalize_optional_ref(status))
            .await
    }

    pub async fn save_generation_rule(
        &self,
        department_id: &str,
        payload: FlightGenerationRuleCreate,
    ) -> Result<FlightGenerationRule, DomainError> {
        self.ensure_department(department_id).await?;
        let task_type = require_non_empty(&payload.task_type, "task_type")?;
        let leg_scope = parse_leg_scope(&payload.leg_scope)?;
        let status = parse_department_rule_status(&payload.status)?;
        let requested_rule_id = payload
            .rule_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let existing = if let Some(rule_id) = requested_rule_id.as_deref() {
            self.generation_rule_repo.find_by_id(rule_id).await?
        } else {
            None
        };
        if let Some(existing) = existing.as_ref() {
            if existing.department_id != department_id {
                return Err(DomainError::NotFound {
                    entity_type: "flight_generation_rule",
                    id: existing.id.clone(),
                });
            }
            if existing.status == DepartmentRuleStatus::Archived {
                return Err(DomainError::ValidationError(
                    "已归档规则不可直接修改，请新建规则".to_string(),
                ));
            }
            if existing.status == DepartmentRuleStatus::Published && status != DepartmentRuleStatus::Published {
                return Err(DomainError::ValidationError(
                    "已发布规则的修改必须直接发布为新版本；如需停用请使用删除/归档操作".to_string(),
                ));
            }
        }
        let creates_new_version = existing
            .as_ref()
            .is_some_and(|rule| rule.status != DepartmentRuleStatus::Draft);
        let rule_id = if creates_new_version || requested_rule_id.is_none() {
            ulid::Ulid::new().to_string()
        } else {
            requested_rule_id.clone().expect("existing draft id")
        };
        let validation = self
            .validate_generation_rule(department_id, &payload, requested_rule_id.as_deref())
            .await?;
        if status == DepartmentRuleStatus::Published && !validation["valid"].as_bool().unwrap_or(false) {
            let message = validation["messages"]
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("；"))
                .unwrap_or_else(|| "基础生成规则校验失败".to_string());
            return Err(DomainError::BusinessRuleViolation(message));
        }

        let start_flex_minutes = match payload.start_flex_minutes {
            Some(value) if value < 0 => {
                return Err(DomainError::ValidationError(
                    "start_flex_minutes 不能为负数".to_string(),
                ));
            }
            value => value,
        };
        let duration_by_crew_size = normalize_duration_by_crew_size(payload.duration_by_crew_size)?;
        if let Some(lead_minutes) = payload.completion_warning_lead_minutes {
            validate_completion_warning_lead_minutes(lead_minutes)?;
        }
        let (completion_time_mode, completion_anchor_type, completion_offset_minutes) =
            validate_completion_configuration(
                &payload.completion_time_mode,
                payload.completion_anchor_type.as_deref(),
                payload.completion_offset_minutes,
                payload.duration_minutes,
                duration_by_crew_size.as_ref(),
            )?;
        let item = FlightGenerationRule {
            id: rule_id,
            department_id: department_id.to_string(),
            task_type: task_type.clone(),
            leg_scope,
            version_no: if creates_new_version || existing.is_none() {
                self.generation_rule_repo
                    .next_version_no(department_id, &task_type, leg_scope_value(leg_scope))
                    .await?
            } else {
                existing.as_ref().map(|rule| rule.version_no).unwrap_or(1)
            },
            status,
            rule_name: normalize_optional_string(payload.rule_name),
            conditions: payload.conditions,
            generation_anchor_type: parse_generation_anchor_type(&payload.generation_anchor_type)?,
            start_offset_minutes: payload.start_offset_minutes,
            completion_time_mode,
            completion_anchor_type,
            completion_offset_minutes,
            duration_minutes: payload.duration_minutes,
            start_flex_minutes,
            duration_by_crew_size,
            completion_warning_lead_minutes: payload.completion_warning_lead_minutes,
            publication_state: parse_publication_state(&payload.publication_state)?,
            publish_trigger_mode: parse_publish_trigger_mode(&payload.publish_trigger_mode)?,
            publish_at: None,
            publish_offset_minutes: payload.publish_offset_minutes,
            publish_event_code: normalize_optional_string(payload.publish_event_code),
            notes: normalize_optional_string(payload.notes),
            published_at: (status == DepartmentRuleStatus::Published).then(Utc::now),
            created_at: None,
            updated_at: None,
        };
        if creates_new_version && status == DepartmentRuleStatus::Published {
            let previous_rule_id = existing
                .as_ref()
                .map(|rule| rule.id.as_str())
                .expect("new version must have previous rule");
            self.generation_rule_repo
                .save_replacing_published(&item, previous_rule_id)
                .await
        } else {
            self.generation_rule_repo.save(&item).await
        }
    }

    pub async fn delete_generation_rule(&self, department_id: &str, rule_id: &str) -> Result<Value, DomainError> {
        self.ensure_department(department_id).await?;
        let Some(mut existing) = self.generation_rule_repo.find_by_id(rule_id).await? else {
            return Err(DomainError::NotFound {
                entity_type: "flight_generation_rule",
                id: rule_id.to_string(),
            });
        };

        if existing.department_id != department_id {
            return Err(DomainError::NotFound {
                entity_type: "flight_generation_rule",
                id: rule_id.to_string(),
            });
        }

        existing.status = DepartmentRuleStatus::Archived;
        existing.published_at = None;
        self.generation_rule_repo.save(&existing).await?;

        Ok(json!({ "message": "触发规则已删除" }))
    }

    pub async fn validate_generation_rule(
        &self,
        department_id: &str,
        payload: &FlightGenerationRuleCreate,
        current_rule_id: Option<&str>,
    ) -> Result<Value, DomainError> {
        self.ensure_department(department_id).await?;
        let task_type = require_non_empty(&payload.task_type, "task_type")?;
        let leg_scope = parse_leg_scope(&payload.leg_scope)?;
        parse_generation_anchor_type(&payload.generation_anchor_type)?;
        let duration_by_crew_size = normalize_duration_by_crew_size(payload.duration_by_crew_size.clone())?;
        validate_completion_configuration(
            &payload.completion_time_mode,
            payload.completion_anchor_type.as_deref(),
            payload.completion_offset_minutes,
            payload.duration_minutes,
            duration_by_crew_size.as_ref(),
        )?;
        let rules = self.generation_rule_repo.list_rules(department_id, None).await?;
        let mut conflicts = Vec::new();
        for rule in rules {
            if rule.status == DepartmentRuleStatus::Archived {
                continue;
            }
            if current_rule_id.is_some_and(|id| id == rule.id) {
                continue;
            }
            if rule.task_type != task_type || rule.leg_scope != leg_scope {
                continue;
            }
            if filters_overlap(&rule.conditions, &payload.conditions) {
                conflicts.push(json!({
                    "rule_id": rule.id,
                    "task_type": rule.task_type,
                    "leg_scope": leg_scope_value(rule.leg_scope),
                    "rule_name": rule.rule_name,
                    "status": department_rule_status_value(rule.status),
                }));
            }
        }
        let mut messages = Vec::new();
        if !conflicts.is_empty() {
            messages.push("存在可重叠的基础生成规则".to_string());
        }
        if parse_department_rule_status(&payload.status)? == DepartmentRuleStatus::Published {
            let requirement = self
                .task_type_requirement_repo
                .find_published(department_id, &task_type)
                .await?;
            messages.extend(build_requirement_messages(&task_type, requirement.as_ref()));
        }
        messages = dedupe_strings(messages);
        Ok(json!({
            "valid": conflicts.is_empty() && messages.is_empty(),
            "conflicts": conflicts,
            "messages": messages,
        }))
    }

    pub async fn list_adjustment_rules(
        &self,
        department_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<GenerationAdjustmentRule>, DomainError> {
        self.ensure_department(department_id).await?;
        self.adjustment_rule_repo
            .list_rules(department_id, normalize_optional_ref(status))
            .await
    }

    pub async fn save_adjustment_rule(
        &self,
        department_id: &str,
        payload: GenerationAdjustmentRuleCreate,
    ) -> Result<GenerationAdjustmentRule, DomainError> {
        self.ensure_department(department_id).await?;
        let task_type = require_non_empty(&payload.task_type, "task_type")?;
        let status = parse_department_rule_status(&payload.status)?;
        let requested_rule_id = payload
            .rule_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let existing = if let Some(rule_id) = requested_rule_id.as_deref() {
            self.adjustment_rule_repo.find_by_id(rule_id).await?
        } else {
            None
        };
        if let Some(existing) = existing.as_ref() {
            if existing.department_id != department_id {
                return Err(DomainError::NotFound {
                    entity_type: "generation_adjustment_rule",
                    id: existing.id.clone(),
                });
            }
            if existing.status == DepartmentRuleStatus::Archived {
                return Err(DomainError::ValidationError(
                    "已归档调整规则不可直接修改，请新建规则".to_string(),
                ));
            }
            if existing.status == DepartmentRuleStatus::Published && status != DepartmentRuleStatus::Published {
                return Err(DomainError::ValidationError(
                    "已发布调整规则的修改必须直接发布为新版本".to_string(),
                ));
            }
        }
        let creates_new_version = existing
            .as_ref()
            .is_some_and(|rule| rule.status != DepartmentRuleStatus::Draft);
        let item = GenerationAdjustmentRule {
            id: if creates_new_version {
                ulid::Ulid::new().to_string()
            } else {
                requested_rule_id.unwrap_or_else(|| ulid::Ulid::new().to_string())
            },
            department_id: department_id.to_string(),
            task_type: task_type.clone(),
            version_no: if creates_new_version || existing.is_none() {
                self.adjustment_rule_repo
                    .next_version_no(department_id, &task_type)
                    .await?
            } else {
                existing.as_ref().map(|rule| rule.version_no).unwrap_or(1)
            },
            status,
            rule_name: normalize_optional_string(payload.rule_name),
            conditions: payload.conditions,
            actions: payload.actions,
            notes: normalize_optional_string(payload.notes),
            published_at: (status == DepartmentRuleStatus::Published).then(Utc::now),
            created_at: None,
            updated_at: None,
        };
        if creates_new_version && status == DepartmentRuleStatus::Published {
            let previous_rule_id = existing
                .as_ref()
                .map(|rule| rule.id.as_str())
                .expect("new version must have previous rule");
            self.adjustment_rule_repo
                .save_replacing_published(&item, previous_rule_id)
                .await
        } else {
            self.adjustment_rule_repo.save(&item).await
        }
    }

    pub async fn save_temporary_task_template(
        &self,
        department_id: &str,
        payload: TemporaryTaskTemplateCreate,
    ) -> Result<TemporaryTaskTemplate, DomainError> {
        self.ensure_department(department_id).await?;
        let template = TemporaryTaskTemplate {
            id: ulid::Ulid::new().to_string(),
            department_id: department_id.to_string(),
            template_code: require_non_empty(&payload.template_code, "template_code")?,
            template_name: require_non_empty(&payload.template_name, "template_name")?,
            task_type: require_non_empty(&payload.task_type, "task_type")?,
            crew_requirements: self.normalize_template_crew_requirements(payload.crew_requirements)?,
            equipment_requirements: self.normalize_equipment_requirements(payload.equipment_requirements),
            notes: normalize_optional_string(payload.notes),
            is_active: payload.is_active,
            created_at: None,
            updated_at: None,
        };
        self.temporary_task_template_repo.save(&template).await
    }

    pub async fn list_temporary_task_templates(
        &self,
        department_id: &str,
        include_inactive: bool,
    ) -> Result<Vec<TemporaryTaskTemplate>, DomainError> {
        self.ensure_department(department_id).await?;
        self.temporary_task_template_repo
            .list_templates(department_id, include_inactive)
            .await
    }

    pub async fn preview_dispatch_rules(
        &self,
        department_id: &str,
        payload: DispatchRulePreviewRequest,
    ) -> Result<Value, DomainError> {
        self.ensure_department(department_id).await?;
        let sample_flight = payload.sample_flight;
        let leg_scope = sample_flight.get("leg_scope").and_then(Value::as_str).unwrap_or("none");
        let generation_rules = self
            .generation_rule_repo
            .list_rules(department_id, Some("published"))
            .await?;
        let adjustment_rules = self
            .adjustment_rule_repo
            .list_rules(department_id, Some("published"))
            .await?;

        let mut generated_orders = Vec::new();
        let mut applied_adjustments = Vec::new();
        let mut turnaround_constraints = Vec::new();
        let mut conflicts = Vec::new();
        let mut blocking_errors = Vec::new();

        for rule in generation_rules {
            if leg_scope_value(rule.leg_scope) != leg_scope {
                continue;
            }
            if !conditions_match_context(&rule.conditions, &sample_flight) {
                continue;
            }

            let requirement = self
                .task_type_requirement_repo
                .find_published(department_id, &rule.task_type)
                .await?;
            let Some(requirement) = requirement else {
                blocking_errors.push(format!("作业类型 {} 缺少已发布作业类型规则", rule.task_type));
                continue;
            };

            let requirement_messages = build_requirement_messages(&rule.task_type, Some(&requirement));
            if !requirement_messages.is_empty() {
                blocking_errors.extend(requirement_messages);
                continue;
            }

            let mut order_payload = json!({
                "task_type": rule.task_type,
                "leg_scope": leg_scope_value(rule.leg_scope),
                "generation_rule_id": rule.id,
                "generation_rule_version": rule.version_no,
                "generation_anchor_type": rule.generation_anchor_type,
                "start_offset_minutes": rule.start_offset_minutes,
                "duration_minutes": rule.duration_minutes,
                "publication_state": publication_state_value(rule.publication_state),
                "publish_trigger_mode": publish_trigger_mode_value(rule.publish_trigger_mode),
                "publish_offset_minutes": rule.publish_offset_minutes,
                "crew_requirement_snapshot": crew_requirements_to_json(&requirement.crew_requirements),
                "equipment_requirement_snapshot": equipment_requirements_to_json(&requirement.equipment_requirements),
            });

            let mut matched_adjustment_rule_ids = Vec::new();
            for adjustment in &adjustment_rules {
                if adjustment.task_type != order_payload["task_type"].as_str().unwrap_or_default() {
                    continue;
                }
                if !conditions_match_context(&adjustment.conditions, &sample_flight) {
                    continue;
                }
                apply_adjustments_to_preview_order(&mut order_payload, &adjustment.actions);
                matched_adjustment_rule_ids.push(Value::String(adjustment.id.clone()));
                applied_adjustments.push(json!({
                    "task_type": adjustment.task_type,
                    "rule_id": adjustment.id,
                    "actions": adjustment.actions,
                }));
            }
            order_payload["matched_adjustment_rule_ids"] = Value::Array(matched_adjustment_rule_ids);
            generated_orders.push(order_payload);

            for turnaround_rule in &requirement.turnaround_continuity_rules {
                if matches_turnaround_rule_preview(&sample_flight, turnaround_rule) {
                    turnaround_constraints.push(build_turnaround_preview_entry(
                        rule.task_type.as_str(),
                        leg_scope_value(rule.leg_scope),
                        turnaround_rule,
                        &sample_flight,
                    ));
                }
            }
        }

        blocking_errors = dedupe_strings(blocking_errors);
        conflicts = dedupe_values(conflicts);
        Ok(json!({
            "generated_orders": generated_orders,
            "applied_adjustments": applied_adjustments,
            "turnaround_constraints": turnaround_constraints,
            "conflicts": conflicts,
            "blocking_errors": blocking_errors,
        }))
    }

    async fn ensure_department(&self, department_id: &str) -> Result<(), DomainError> {
        let department = self.department_repo.find_by_id(department_id).await?;
        if department.is_none() {
            return Err(DomainError::NotFound {
                entity_type: "department",
                id: department_id.to_string(),
            });
        }
        Ok(())
    }

    fn normalize_crew_requirements(
        &self,
        requirements: Vec<crate::schemas::dispatch_schemas::TaskTypeCrewSlotRequirementSchema>,
        crew_requirements: Vec<crate::schemas::dispatch_schemas::TaskTypeCrewSlotRequirementSchema>,
    ) -> Result<Vec<TaskTypeCrewSlotRequirement>, DomainError> {
        let source = if crew_requirements.is_empty() {
            requirements
        } else {
            crew_requirements
        };
        source
            .into_iter()
            .map(|item| {
                Ok(TaskTypeCrewSlotRequirement {
                    slot_code: require_non_empty(&item.slot_code, "requirements[].slot_code")?,
                    qualification_code: require_non_empty(
                        &item.qualification_code,
                        "requirements[].qualification_code",
                    )?,
                    min_level_code: normalize_optional_string(item.min_level_code),
                    required_count: item.required_count.max(1),
                    must_be_distinct: item.must_be_distinct,
                    exclusive_group: normalize_optional_string(item.exclusive_group),
                    remarks: normalize_optional_string(item.remarks),
                })
            })
            .collect()
    }

    fn normalize_template_crew_requirements(
        &self,
        crew_requirements: Vec<crate::schemas::dispatch_schemas::TaskTypeCrewSlotRequirementSchema>,
    ) -> Result<Vec<TaskTypeCrewSlotRequirement>, DomainError> {
        crew_requirements
            .into_iter()
            .map(|item| {
                Ok(TaskTypeCrewSlotRequirement {
                    slot_code: require_non_empty(&item.slot_code, "crew_requirements[].slot_code")?,
                    qualification_code: require_non_empty(
                        &item.qualification_code,
                        "crew_requirements[].qualification_code",
                    )?,
                    min_level_code: normalize_optional_string(item.min_level_code),
                    required_count: item.required_count.max(1),
                    must_be_distinct: item.must_be_distinct,
                    exclusive_group: normalize_optional_string(item.exclusive_group),
                    remarks: normalize_optional_string(item.remarks),
                })
            })
            .collect()
    }

    fn normalize_equipment_requirements(
        &self,
        equipment_requirements: Vec<crate::schemas::dispatch_schemas::TaskTypeEquipmentRequirementSchema>,
    ) -> Vec<TaskTypeEquipmentRequirement> {
        equipment_requirements
            .into_iter()
            .map(|item| TaskTypeEquipmentRequirement {
                slot_code: item.slot_code.trim().to_string(),
                equipment_type_id: normalize_optional_string(item.equipment_type_id),
                equipment_type_code: normalize_optional_string(item.equipment_type_code),
                required_count: item.required_count.max(1),
                must_be_distinct: item.must_be_distinct,
                requires_driver: item.requires_driver,
                driver_qualification_code: normalize_optional_string(item.driver_qualification_code),
                driver_min_level_code: normalize_optional_string(item.driver_min_level_code),
                remarks: normalize_optional_string(item.remarks),
            })
            .filter(|item| !item.slot_code.is_empty())
            .collect()
    }
}

fn require_non_empty(value: &str, field: &str) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::ValidationError(format!("{field} 不能为空")));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

const GENERATION_ANCHOR_TYPES: [&str; 7] = [
    "scheduled_time",
    "actual_arrival",
    "estimated_arrival",
    "scheduled_arrival",
    "actual_departure",
    "estimated_departure",
    "scheduled_departure",
];

const COMPLETION_TIME_MODES: [&str; 2] = ["start_plus_duration", "completion_anchor_offset"];

fn parse_generation_anchor_type(value: &str) -> Result<String, DomainError> {
    let normalized = value.trim().to_ascii_lowercase();
    if GENERATION_ANCHOR_TYPES.contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(DomainError::ValidationError(format!(
        "generation_anchor_type 不支持 {value:?}，允许值：{}",
        GENERATION_ANCHOR_TYPES.join(", ")
    )))
}

fn parse_completion_time_mode(value: &str) -> Result<String, DomainError> {
    let normalized = value.trim().to_ascii_lowercase();
    if COMPLETION_TIME_MODES.contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(DomainError::ValidationError(format!(
        "completion_time_mode 不支持 {value:?}，允许值：{}",
        COMPLETION_TIME_MODES.join(", ")
    )))
}

fn parse_completion_anchor_type(value: &str) -> Result<String, DomainError> {
    let normalized = value.trim().to_ascii_lowercase();
    if GENERATION_ANCHOR_TYPES.contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(DomainError::ValidationError(format!(
        "completion_anchor_type 不支持 {value:?}，允许值：{}",
        GENERATION_ANCHOR_TYPES.join(", ")
    )))
}

fn validate_completion_configuration(
    mode: &str,
    completion_anchor_type: Option<&str>,
    completion_offset_minutes: Option<i32>,
    duration_minutes: Option<i32>,
    duration_by_crew_size: Option<&Value>,
) -> Result<(String, Option<String>, Option<i32>), DomainError> {
    let mode = parse_completion_time_mode(mode)?;
    match mode.as_str() {
        "start_plus_duration" => {
            if completion_anchor_type.is_some_and(|value| !value.trim().is_empty())
                || completion_offset_minutes.is_some()
            {
                return Err(DomainError::ValidationError(
                    "start_plus_duration 模式不能配置 completion_anchor_type 或 completion_offset_minutes".to_string(),
                ));
            }
            if duration_minutes.is_some_and(|minutes| minutes <= 0) {
                return Err(DomainError::ValidationError(
                    "duration_minutes 必须是正整数分钟".to_string(),
                ));
            }
            Ok((mode, None, None))
        }
        "completion_anchor_offset" => {
            if duration_minutes.is_some() || duration_by_crew_size.is_some() {
                return Err(DomainError::ValidationError(
                    "completion_anchor_offset 模式不能配置 duration_minutes 或 duration_by_crew_size".to_string(),
                ));
            }
            let anchor = completion_anchor_type
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    DomainError::ValidationError(
                        "completion_anchor_offset 模式必须配置 completion_anchor_type".to_string(),
                    )
                })?;
            let anchor = parse_completion_anchor_type(anchor)?;
            let offset = completion_offset_minutes.ok_or_else(|| {
                DomainError::ValidationError(
                    "completion_anchor_offset 模式必须配置 completion_offset_minutes".to_string(),
                )
            })?;
            Ok((mode, Some(anchor), Some(offset)))
        }
        _ => unreachable!("completion mode was validated above"),
    }
}

/// Validates and canonicalizes a `crew size -> minutes` object.
/// Invalid API input is rejected instead of being silently rewritten to NULL;
/// the read side remains defensive for historical rows.
fn normalize_duration_by_crew_size(value: Option<Value>) -> Result<Option<Value>, DomainError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Value::Object(entries) = value else {
        return Err(DomainError::ValidationError(
            "duration_by_crew_size 必须是人数到分钟数的 JSON 对象".to_string(),
        ));
    };
    let mut normalized = serde_json::Map::new();
    for (crew_size, minutes) in entries {
        let crew_size: u32 = crew_size
            .trim()
            .parse()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                DomainError::ValidationError(format!("duration_by_crew_size 的人数 {crew_size:?} 必须是正整数"))
            })?;
        let minutes = match &minutes {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.trim().parse::<i64>().ok(),
            _ => None,
        }
        .filter(|value| *value > 0 && *value <= i64::from(i32::MAX))
        .ok_or_else(|| {
            DomainError::ValidationError(format!(
                "duration_by_crew_size 中人数 {crew_size} 的时长必须是正整数分钟"
            ))
        })?;
        if normalized.insert(crew_size.to_string(), Value::from(minutes)).is_some() {
            return Err(DomainError::ValidationError(format!(
                "duration_by_crew_size 中人数 {crew_size} 被重复配置"
            )));
        }
    }
    if normalized.is_empty() {
        return Ok(None);
    }
    Ok(Some(Value::Object(normalized)))
}

fn normalize_optional_ref(value: Option<&str>) -> Option<&str> {
    value.filter(|&item| !item.trim().is_empty())
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn parse_grant_status(value: &str) -> Result<QualificationGrantStatus, DomainError> {
    match value.trim() {
        "active" | "" => Ok(QualificationGrantStatus::Active),
        "expired" => Ok(QualificationGrantStatus::Expired),
        "suspended" => Ok(QualificationGrantStatus::Suspended),
        other => Err(DomainError::ValidationError(format!("未知资质状态: {other}"))),
    }
}

fn parse_department_rule_status(value: &str) -> Result<DepartmentRuleStatus, DomainError> {
    match value.trim() {
        "draft" | "" => Ok(DepartmentRuleStatus::Draft),
        "published" => Ok(DepartmentRuleStatus::Published),
        "archived" => Ok(DepartmentRuleStatus::Archived),
        other => Err(DomainError::ValidationError(format!("未知规则状态: {other}"))),
    }
}

fn parse_leg_scope(value: &str) -> Result<LegScope, DomainError> {
    match value.trim() {
        "inbound" => Ok(LegScope::Inbound),
        "outbound" => Ok(LegScope::Outbound),
        "none" | "" => Ok(LegScope::None),
        other => Err(DomainError::ValidationError(format!("未知航段范围: {other}"))),
    }
}

fn parse_publication_state(value: &str) -> Result<DispatchPublicationState, DomainError> {
    match value.trim() {
        "prepublished" | "" => Ok(DispatchPublicationState::Prepublished),
        "published" => Ok(DispatchPublicationState::Published),
        "cancelled" => Ok(DispatchPublicationState::Cancelled),
        other => Err(DomainError::ValidationError(format!("未知发布状态: {other}"))),
    }
}

fn parse_publish_trigger_mode(value: &str) -> Result<PublishTriggerMode, DomainError> {
    match value.trim() {
        "time" | "" => Ok(PublishTriggerMode::Time),
        "event" => Ok(PublishTriggerMode::Event),
        "either" => Ok(PublishTriggerMode::Either),
        "both_required" => Ok(PublishTriggerMode::BothRequired),
        other => Err(DomainError::ValidationError(format!("未知触发模式: {other}"))),
    }
}

fn parse_turnaround_constraint_mode(value: &str) -> Result<TurnaroundConstraintMode, DomainError> {
    match value.trim() {
        "same_person" => Ok(TurnaroundConstraintMode::SamePerson),
        "soft_prefer_same_person" => Ok(TurnaroundConstraintMode::SoftPreferSamePerson),
        "handover_required" => Ok(TurnaroundConstraintMode::HandoverRequired),
        "disabled" | "" => Ok(TurnaroundConstraintMode::Disabled),
        other => Err(DomainError::ValidationError(format!("未知过站约束模式: {other}"))),
    }
}

pub(crate) fn filters_overlap(left: &HashMap<String, Value>, right: &HashMap<String, Value>) -> bool {
    if left.is_empty() || right.is_empty() {
        return true;
    }
    let left_branches = condition_branches(&normalize_conditions(left));
    let right_branches = condition_branches(&normalize_conditions(right));
    if left_branches.is_empty() || right_branches.is_empty() {
        return true;
    }
    left_branches.iter().any(|left_branch| {
        right_branches
            .iter()
            .any(|right_branch| branches_can_overlap(left_branch, right_branch))
    })
}

fn conditions_match_context(conditions: &HashMap<String, Value>, context: &HashMap<String, Value>) -> bool {
    evaluate_condition_tree(&normalize_conditions(conditions), context)
}

#[derive(Clone, Debug)]
struct ConditionLeaf {
    field: String,
    op: String,
    value: Value,
}

/// Converts a condition tree into disjunctive branches. Unsupported or malformed
/// nodes intentionally become an unconstrained branch: conflict validation must
/// be conservative and may reject an ambiguous pair, but must never publish two
/// rules merely because it failed to understand their predicates.
fn condition_branches(tree: &Value) -> Vec<Vec<ConditionLeaf>> {
    let Some(object) = tree.as_object() else {
        return vec![Vec::new()];
    };
    if object.contains_key("field") {
        let Some(field) = object.get("field").and_then(Value::as_str).map(str::trim) else {
            return vec![Vec::new()];
        };
        if field.is_empty() {
            return vec![Vec::new()];
        }
        return vec![vec![ConditionLeaf {
            field: field.to_string(),
            op: object
                .get("op")
                .and_then(Value::as_str)
                .unwrap_or("eq")
                .trim()
                .to_ascii_lowercase(),
            value: object.get("value").cloned().unwrap_or(Value::Null),
        }]];
    }
    let children = object
        .get("children")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if children.is_empty() {
        return vec![Vec::new()];
    }
    let operator = object
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or("AND")
        .trim()
        .to_ascii_uppercase();
    if operator == "OR" {
        return children.iter().flat_map(condition_branches).collect();
    }
    let mut branches = vec![Vec::new()];
    for child in &children {
        let child_branches = condition_branches(child);
        let mut combined = Vec::new();
        for branch in &branches {
            for child_branch in &child_branches {
                let mut next = branch.clone();
                next.extend(child_branch.clone());
                combined.push(next);
            }
        }
        branches = combined;
    }
    branches
}

fn branches_can_overlap(left: &[ConditionLeaf], right: &[ConditionLeaf]) -> bool {
    !left.iter().any(|left_leaf| {
        right
            .iter()
            .any(|right_leaf| leaves_are_disjoint(left_leaf, right_leaf))
    })
}

fn leaves_are_disjoint(left: &ConditionLeaf, right: &ConditionLeaf) -> bool {
    if left.field != right.field {
        return false;
    }
    let left_values = exact_condition_values(left);
    let right_values = exact_condition_values(right);
    if let (Some(left_values), Some(right_values)) = (left_values, right_values) {
        return left_values.is_disjoint(&right_values);
    }
    let left_range = numeric_condition_range(left);
    let right_range = numeric_condition_range(right);
    match (left_range, right_range) {
        (Some((left_min, left_max)), Some((right_min, right_max))) => left_max < right_min || right_max < left_min,
        _ => false,
    }
}

fn exact_condition_values(leaf: &ConditionLeaf) -> Option<HashSet<String>> {
    match leaf.op.as_str() {
        "eq" | "in" => Some(value_to_filter_set(&leaf.value)),
        _ => None,
    }
}

fn numeric_condition_range(leaf: &ConditionLeaf) -> Option<(f64, f64)> {
    let value = parse_numeric(&leaf.value)?;
    match leaf.op.as_str() {
        "eq" => Some((value, value)),
        "gt" => Some((value + f64::EPSILON, f64::INFINITY)),
        "gte" => Some((value, f64::INFINITY)),
        "lt" => Some((f64::NEG_INFINITY, value - f64::EPSILON)),
        "lte" => Some((f64::NEG_INFINITY, value)),
        _ => None,
    }
}

/// Convert legacy flat-dict conditions into a standard condition tree.
///
/// Legacy format (implicit AND of field checks):
///   `{"is_vip": true, "flight_nature": "domestic"}`
/// Normalized tree:
///   `{"operator":"AND","children":[{"field":"is_vip","op":"eq","value":true}, ...]}`
///
/// If *raw* is already a tree it is returned unchanged.
fn normalize_conditions(raw: &HashMap<String, Value>) -> Value {
    if raw.is_empty() {
        return json!({"operator": "AND", "children": []});
    }
    if is_condition_tree(raw) {
        return Value::Object(raw.clone().into_iter().collect());
    }
    // Legacy flat dict → convert each key-value pair into a leaf node.
    let mut children: Vec<Value> = Vec::new();
    for (key, value) in raw {
        if is_empty_filter_value(value) {
            continue;
        }
        if value.is_array() {
            children.push(json!({"field": key, "op": "in", "value": value}));
        } else {
            children.push(json!({"field": key, "op": "eq", "value": value}));
        }
    }
    json!({"operator": "AND", "children": children})
}

/// Return true when the map uses the tree format (has both "operator" and "children" keys).
fn is_condition_tree(map: &HashMap<String, Value>) -> bool {
    map.contains_key("operator") && map.contains_key("children")
}

/// Recursively evaluate a condition tree against a flight context dict.
///
/// A group node has `operator` (AND | OR) and `children` (list).
/// A leaf node has `field`, `op`, and `value`.
fn evaluate_condition_tree(tree: &Value, context: &HashMap<String, Value>) -> bool {
    // Empty/null tree → match everything
    if tree.is_null() || tree.as_object().is_some_and(|o| o.is_empty()) {
        return true;
    }
    let obj = match tree.as_object() {
        Some(o) => o,
        None => return true,
    };
    // Leaf node
    if obj.contains_key("field") {
        return evaluate_leaf(tree, context);
    }
    // Group node
    let operator = obj
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or("AND")
        .to_ascii_uppercase();
    let children = match obj.get("children").and_then(Value::as_array) {
        Some(arr) => arr,
        None => return true,
    };
    if children.is_empty() {
        return true;
    }
    if operator == "OR" {
        children.iter().any(|child| evaluate_condition_tree(child, context))
    } else {
        children.iter().all(|child| evaluate_condition_tree(child, context))
    }
}

/// Evaluate a single condition leaf against context.
fn evaluate_leaf(leaf: &Value, context: &HashMap<String, Value>) -> bool {
    let obj = match leaf.as_object() {
        Some(o) => o,
        None => return true,
    };
    let field = obj.get("field").and_then(Value::as_str).unwrap_or("");
    let op = obj
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or("eq")
        .to_ascii_lowercase();
    let expected = obj.get("value").unwrap_or(&Value::Null);
    let actual = context.get(field).unwrap_or(&Value::Null);

    // Missing / empty actual → condition not met (unless op checks for absence)
    if is_empty_filter_value(actual) {
        if op == "eq" && (expected.is_null() || expected.as_str() == Some("") || expected.as_bool() == Some(false)) {
            return true;
        }
        if op == "neq" {
            return true;
        }
        return false;
    }

    match op.as_str() {
        "eq" => {
            if let Some(b) = expected.as_bool() {
                return actual.as_bool() == Some(b);
            }
            normalized_scalar(actual) == normalized_scalar(expected)
        }
        "neq" => {
            if let Some(b) = expected.as_bool() {
                return actual.as_bool() != Some(b);
            }
            normalized_scalar(actual) != normalized_scalar(expected)
        }
        "in" => {
            let expected_set = value_to_filter_set(expected);
            let actual_set = value_to_filter_set(actual);
            !expected_set.is_disjoint(&actual_set)
        }
        "nin" => {
            let expected_set = value_to_filter_set(expected);
            let actual_set = value_to_filter_set(actual);
            expected_set.is_disjoint(&actual_set)
        }
        "contains" => {
            let hay = to_normalized_str(actual);
            let needle = to_normalized_str(expected);
            !needle.is_empty() && hay.contains(&needle)
        }
        "gt" | "gte" | "lt" | "lte" => {
            let actual_num = match parse_numeric(actual) {
                Some(n) => n,
                None => return false,
            };
            let expected_num = match parse_numeric(expected) {
                Some(n) => n,
                None => return false,
            };
            match op.as_str() {
                "gt" => actual_num > expected_num,
                "gte" => actual_num >= expected_num,
                "lt" => actual_num < expected_num,
                "lte" => actual_num <= expected_num,
                _ => false,
            }
        }
        _ => false,
    }
}

fn to_normalized_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.trim().to_ascii_lowercase(),
        Value::Null => String::new(),
        other => other.to_string().trim().to_ascii_lowercase(),
    }
}

fn parse_numeric(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn is_empty_filter_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

fn value_to_filter_set(value: &Value) -> HashSet<String> {
    match value {
        Value::Array(items) => items.iter().filter_map(normalized_scalar).collect(),
        _ => normalized_scalar(value).into_iter().collect(),
    }
}

fn normalized_scalar(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => {
            let trimmed = text.trim().to_ascii_lowercase();
            (!trimmed.is_empty()).then_some(trimmed)
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => Some(value.to_string().to_ascii_lowercase()),
    }
}

fn build_requirement_messages(
    task_type: &str,
    requirement_version: Option<&DepartmentTaskTypeRequirementVersion>,
) -> Vec<String> {
    let Some(requirement_version) = requirement_version else {
        return vec![format!("作业类型 {task_type} 缺少已发布作业类型规则")];
    };
    let crew_requirements = if requirement_version.crew_requirements.is_empty() {
        &requirement_version.requirements
    } else {
        &requirement_version.crew_requirements
    };
    let mut messages = Vec::new();
    if crew_requirements.is_empty() {
        messages.push(format!("作业类型 {task_type} 缺少人员资质要求"));
    }
    if requirement_version.equipment_requirements.is_empty() {
        messages.push(format!("作业类型 {task_type} 缺少设备类型要求"));
    }
    messages
}

fn crew_requirements_to_json(items: &[TaskTypeCrewSlotRequirement]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| {
                json!({
                    "slot_code": item.slot_code,
                    "qualification_code": item.qualification_code,
                    "min_level_code": item.min_level_code,
                    "required_count": item.required_count,
                })
            })
            .collect(),
    )
}

fn equipment_requirements_to_json(items: &[TaskTypeEquipmentRequirement]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| {
                json!({
                    "slot_code": item.slot_code,
                    "equipment_type_id": item.equipment_type_id,
                    "equipment_type_code": item.equipment_type_code,
                    "required_count": item.required_count,
                    "requires_driver": item.requires_driver,
                    "driver_qualification_code": item.driver_qualification_code,
                    "driver_min_level_code": item.driver_min_level_code,
                })
            })
            .collect(),
    )
}

pub(crate) fn apply_adjustments_to_preview_order(order_payload: &mut Value, actions: &[Value]) {
    let mut crew_requirements = order_payload["crew_requirement_snapshot"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut equipment_requirements = order_payload["equipment_requirement_snapshot"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    for action in actions {
        let Some(action_type) = action.get("action_type").and_then(Value::as_str) else {
            continue;
        };
        let slot_code = action.get("slot_code").and_then(Value::as_str).unwrap_or_default();
        match action_type {
            "increase_slot_count" => {
                for item in &mut crew_requirements {
                    if item.get("slot_code").and_then(Value::as_str) == Some(slot_code) {
                        let current = item.get("required_count").and_then(Value::as_i64).unwrap_or(1);
                        let delta = action.get("delta").and_then(Value::as_i64).unwrap_or(1);
                        item["required_count"] = Value::from(current + delta);
                    }
                }
            }
            "add_slot" => {
                if let Some(slot) = action.get("slot").cloned() {
                    crew_requirements.push(slot);
                }
            }
            "upgrade_min_level" => {
                for item in &mut crew_requirements {
                    if item.get("slot_code").and_then(Value::as_str) == Some(slot_code) {
                        item["min_level_code"] = action.get("min_level_code").cloned().unwrap_or(Value::Null);
                    }
                }
            }
            "extend_duration" => {
                let current = order_payload["duration_minutes"].as_i64().unwrap_or(0);
                let delta = action.get("delta_minutes").and_then(Value::as_i64).unwrap_or(0);
                order_payload["duration_minutes"] = Value::from(current + delta);
            }
            "advance_publish_offset" => {
                let current = order_payload["publish_offset_minutes"].as_i64().unwrap_or(0);
                let delta = action.get("delta_minutes").and_then(Value::as_i64).unwrap_or(0);
                order_payload["publish_offset_minutes"] = Value::from(current - delta);
            }
            "delay_publish_offset" => {
                let current = order_payload["publish_offset_minutes"].as_i64().unwrap_or(0);
                let delta = action.get("delta_minutes").and_then(Value::as_i64).unwrap_or(0);
                order_payload["publish_offset_minutes"] = Value::from(current + delta);
            }
            "increase_equipment_count" => {
                for item in &mut equipment_requirements {
                    if item.get("slot_code").and_then(Value::as_str) == Some(slot_code) {
                        let current = item.get("required_count").and_then(Value::as_i64).unwrap_or(1);
                        let delta = action.get("delta").and_then(Value::as_i64).unwrap_or(1);
                        item["required_count"] = Value::from(current + delta);
                    }
                }
            }
            "add_equipment_type_requirement" => {
                if let Some(slot) = action.get("equipment_slot").cloned() {
                    equipment_requirements.push(slot);
                }
            }
            "require_driver_for_equipment" => {
                for item in &mut equipment_requirements {
                    if item.get("slot_code").and_then(Value::as_str) == Some(slot_code) {
                        item["requires_driver"] = Value::Bool(true);
                        if let Some(value) = action.get("driver_qualification_code") {
                            item["driver_qualification_code"] = value.clone();
                        }
                        if let Some(value) = action.get("driver_min_level_code") {
                            item["driver_min_level_code"] = value.clone();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    order_payload["crew_requirement_snapshot"] = Value::Array(crew_requirements);
    order_payload["equipment_requirement_snapshot"] = Value::Array(equipment_requirements);
}

fn matches_turnaround_rule_preview(
    sample_flight: &HashMap<String, Value>,
    turnaround_rule: &TurnaroundContinuityRule,
) -> bool {
    if !turnaround_rule.enabled {
        return false;
    }
    if !sample_flight
        .get("is_turnaround")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    if !conditions_match_context(&turnaround_rule.flight_filters, sample_flight) {
        return false;
    }
    let aircraft_filters = turnaround_rule
        .aircraft_type_filters
        .iter()
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect::<HashSet<_>>();
    let aircraft_type = sample_flight
        .get("aircraft_type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    aircraft_filters.is_empty() || aircraft_filters.contains(&aircraft_type)
}

fn build_turnaround_preview_entry(
    task_type: &str,
    leg_scope: &str,
    turnaround_rule: &TurnaroundContinuityRule,
    sample_flight: &HashMap<String, Value>,
) -> Value {
    let delta_t_minutes = sample_flight
        .get("delta_t_minutes")
        .and_then(Value::as_i64)
        .or_else(|| {
            sample_flight
                .get("delta_t_minutes")
                .and_then(Value::as_str)?
                .parse::<i64>()
                .ok()
        });
    let minimum_turnaround_minutes = sample_flight
        .get("minimum_turnaround_minutes")
        .or_else(|| sample_flight.get("mt_minutes"))
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse::<i64>().ok()));
    let slack_minutes = match (delta_t_minutes, minimum_turnaround_minutes) {
        (Some(delta), Some(minimum)) => Some((delta - minimum).max(0)),
        _ => None,
    };
    json!({
        "pair_key": sample_flight
            .get("turnaround_pair_key")
            .or_else(|| sample_flight.get("flight_id"))
            .unwrap_or(&Value::Null),
        "task_type": task_type,
        "leg_scope": leg_scope,
        "counterpart_leg_scope": leg_scope_value(turnaround_rule.counterpart_leg_scope),
        "counterpart_task_type": turnaround_rule.counterpart_task_type,
        "constraint_mode": turnaround_constraint_mode_value(turnaround_rule.constraint_mode),
        "slot_pairs": turnaround_rule.slot_pairs.iter().map(|pair| json!({
            "inbound_slot_code": pair.inbound_slot_code,
            "outbound_slot_code": pair.outbound_slot_code,
        })).collect::<Vec<_>>(),
        "delta_t_minutes": delta_t_minutes,
        "minimum_turnaround_minutes": minimum_turnaround_minutes,
        "slack_minutes": slack_minutes,
        "tight_threshold_minutes": turnaround_rule.tight_threshold_minutes,
        "relax_threshold_minutes": turnaround_rule.relax_threshold_minutes,
    })
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values.into_iter().filter(|item| seen.insert(item.clone())).collect()
}

fn dedupe_values(values: Vec<Value>) -> Vec<Value> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|item| seen.insert(item.to_string()))
        .collect()
}

fn department_rule_status_value(status: DepartmentRuleStatus) -> &'static str {
    match status {
        DepartmentRuleStatus::Draft => "draft",
        DepartmentRuleStatus::Published => "published",
        DepartmentRuleStatus::Archived => "archived",
    }
}

fn leg_scope_value(value: LegScope) -> &'static str {
    match value {
        LegScope::Inbound => "inbound",
        LegScope::Outbound => "outbound",
        LegScope::None => "none",
    }
}

fn publication_state_value(value: DispatchPublicationState) -> &'static str {
    match value {
        DispatchPublicationState::Prepublished => "prepublished",
        DispatchPublicationState::Published => "published",
        DispatchPublicationState::Cancelled => "cancelled",
    }
}

fn publish_trigger_mode_value(value: PublishTriggerMode) -> &'static str {
    match value {
        PublishTriggerMode::Time => "time",
        PublishTriggerMode::Event => "event",
        PublishTriggerMode::Either => "either",
        PublishTriggerMode::BothRequired => "both_required",
    }
}

fn turnaround_constraint_mode_value(value: TurnaroundConstraintMode) -> &'static str {
    match value {
        TurnaroundConstraintMode::SamePerson => "same_person",
        TurnaroundConstraintMode::SoftPreferSamePerson => "soft_prefer_same_person",
        TurnaroundConstraintMode::HandoverRequired => "handover_required",
        TurnaroundConstraintMode::Disabled => "disabled",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        filters_overlap, normalize_duration_by_crew_size, parse_completion_time_mode, parse_generation_anchor_type,
        validate_completion_configuration,
    };
    use fms_domain::error::DomainError;
    use serde_json::json;

    #[test]
    fn duration_by_crew_size_is_canonicalized_without_silently_dropping_entries() {
        let normalized = normalize_duration_by_crew_size(Some(json!({
            "1": "45",
            "2": 30,
        })))
        .expect("valid table")
        .expect("configured table");

        assert_eq!(normalized, json!({ "1": 45, "2": 30 }));
    }

    #[test]
    fn duration_by_crew_size_rejects_invalid_rows() {
        let error =
            normalize_duration_by_crew_size(Some(json!({ "2": 0 }))).expect_err("zero duration must be rejected");

        assert!(matches!(error, DomainError::ValidationError(message) if message.contains("正整数分钟")));
    }

    #[test]
    fn duration_by_crew_size_rejects_duplicate_canonical_crew_sizes() {
        let error = normalize_duration_by_crew_size(Some(json!({
            "01": 45,
            "1": 30,
        })))
        .expect_err("equivalent crew sizes must not overwrite each other");

        assert!(matches!(error, DomainError::ValidationError(message) if message.contains("重复配置")));
    }

    #[test]
    fn generation_anchor_type_accepts_only_the_explicit_vocabulary() {
        assert_eq!(
            parse_generation_anchor_type(" Estimated_Departure ").expect("known anchor"),
            "estimated_departure"
        );
        assert_eq!(
            parse_generation_anchor_type("scheduled_time").expect("generic scheduled anchor"),
            "scheduled_time"
        );
    }

    #[test]
    fn generation_anchor_type_rejects_unknown_values() {
        for value in ["estimated_time", "event", "unknown", ""] {
            let error = parse_generation_anchor_type(value).expect_err("unknown anchor must be rejected");
            assert!(
                matches!(error, DomainError::ValidationError(message) if message.contains("generation_anchor_type")),
                "unexpected error for {value:?}"
            );
        }
    }

    #[test]
    fn completion_time_mode_accepts_only_explicit_values() {
        assert_eq!(
            parse_completion_time_mode(" START_PLUS_DURATION ").expect("known mode"),
            "start_plus_duration"
        );
        assert_eq!(
            parse_completion_time_mode("completion_anchor_offset").expect("known mode"),
            "completion_anchor_offset"
        );
        assert!(parse_completion_time_mode("sla").is_err());
    }

    #[test]
    fn duration_mode_rejects_completion_anchor_fields() {
        let error = validate_completion_configuration(
            "start_plus_duration",
            Some("scheduled_departure"),
            Some(-10),
            Some(30),
            None,
        )
        .expect_err("modes must not be mixed");

        assert!(matches!(error, DomainError::ValidationError(message) if message.contains("不能配置")));
    }

    #[test]
    fn completion_anchor_mode_requires_anchor_and_offset() {
        assert!(validate_completion_configuration("completion_anchor_offset", None, Some(-10), None, None,).is_err());
        assert!(validate_completion_configuration(
            "completion_anchor_offset",
            Some("scheduled_departure"),
            None,
            None,
            None,
        )
        .is_err());
    }

    #[test]
    fn completion_anchor_mode_rejects_duration_sources() {
        let error = validate_completion_configuration(
            "completion_anchor_offset",
            Some("scheduled_departure"),
            Some(-10),
            Some(30),
            None,
        )
        .expect_err("duration is a conflicting completion source");

        assert!(matches!(error, DomainError::ValidationError(message) if message.contains("duration_minutes")));
    }

    #[test]
    fn completion_anchor_mode_accepts_negative_offset() {
        let normalized = validate_completion_configuration(
            "completion_anchor_offset",
            Some(" Estimated_Departure "),
            Some(-10),
            None,
            None,
        )
        .expect("valid completion anchor mode");

        assert_eq!(
            normalized,
            (
                "completion_anchor_offset".to_string(),
                Some("estimated_departure".to_string()),
                Some(-10),
            )
        );
    }

    #[test]
    fn identical_condition_trees_overlap() {
        let tree = json!({
            "operator": "AND",
            "children": [{"field": "is_vip", "op": "eq", "value": true}]
        });
        let map = serde_json::from_value(tree).expect("condition map");

        assert!(filters_overlap(&map, &map));
    }

    #[test]
    fn mutually_exclusive_condition_trees_do_not_overlap() {
        let vip = serde_json::from_value(json!({
            "operator": "AND",
            "children": [{"field": "is_vip", "op": "eq", "value": true}]
        }))
        .expect("vip map");
        let regular = serde_json::from_value(json!({
            "operator": "AND",
            "children": [{"field": "is_vip", "op": "eq", "value": false}]
        }))
        .expect("regular map");

        assert!(!filters_overlap(&vip, &regular));
    }
}
