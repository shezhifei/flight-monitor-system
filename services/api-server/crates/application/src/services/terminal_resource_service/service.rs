//! 空间目录资源服务：Terminal / Gate / BaggageCarousel 的 CRUD 与只读上下文。
//!
//! 规则（见 `docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md`）：
//! - 航站楼目录 = 楼行 + 成员表（`terminal_stands` / `terminal_gates` / `terminal_carousels`）。
//!   成员关系是构成事实，只从这里 `add_*` / `remove_*`。
//! - 新建口/转盘：`terminal_id` 必填，目录行 + 成员行在同一原子操作里建立。
//! - `Stand` / `Gate` / `BaggageCarousel` 没有可写的 `terminal` 字段。
//! - 停用楼或 `remove_*`：成员有未结束占用/分配 → `DomainError::Conflict`(409)，带占用明细。

use std::sync::Arc;

use serde_json::Value;

use crate::services::attribute_validation::{
    collect_attribute_references, sync_attribute_references, validate_attributes,
};
use crate::services::stand_composition::{
    composed_of_codes, validate_stand_composition as validate_composition_snapshot,
};
use crate::services::terminal_resource_writer::TerminalResourceAttributeTransactionalWriter;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{BaggageCarousel, Gate, Stand, Terminal, TerminalDirectory};
use fms_domain::models::field_overlay::OntologyFieldType;
use fms_domain::ports::dispatch_repository::{StandRepository, TerminalRepository};
use fms_domain::ports::field_overlay_repository::FieldOverlayRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceRepository;

use super::schemas::{
    CarouselCreate, CarouselUpdate, GateCreate, GateUpdate, StandCreate, StandUpdate, TerminalCreate, TerminalUpdate,
};

pub struct TerminalResourceService<TR: TerminalRepository + ?Sized> {
    terminal_repo: Arc<TR>,
    field_overlay_repo: Option<Arc<dyn FieldOverlayRepository + Send + Sync>>,
    reference_repo: Option<Arc<dyn OntologyAttributeReferenceRepository + Send + Sync>>,
    attribute_writer: Option<Arc<dyn TerminalResourceAttributeTransactionalWriter>>,
    /// 组成校验需要全量机位快照；TerminalRepository 只有按 code/id 的单查。
    /// 未注入时降级为浅校验（自引用 / 子不存在 / 子停用 / 一层互指）。
    stand_repo: Option<Arc<dyn StandRepository + Send + Sync>>,
}

fn require_non_empty(value: &str, field: &str) -> Result<String, DomainError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(DomainError::ValidationError(format!("{field} 不能为空")));
    }
    Ok(value)
}

fn conflict_with_details(message: impl Into<String>, details: Value) -> DomainError {
    DomainError::Conflict(format!("{}; 明细: {}", message.into(), details))
}

async fn reject_referenced_target(
    reference_repo: Option<&Arc<dyn OntologyAttributeReferenceRepository + Send + Sync>>,
    target_object_name: &str,
    target_key: &str,
    message: &str,
) -> Result<(), DomainError> {
    let Some(reference_repo) = reference_repo else {
        return Ok(());
    };
    let references = reference_repo.find_by_target(target_object_name, target_key).await?;
    if references.is_empty() {
        return Ok(());
    }
    let details = references
        .iter()
        .map(|reference| {
            serde_json::json!({
                "owner_object_name": reference.owner_object_name,
                "owner_object_id": reference.owner_object_id,
                "field_name": reference.field_name,
            })
        })
        .collect::<Vec<_>>();
    Err(conflict_with_details(message, Value::Array(details)))
}

impl<TR: TerminalRepository + Sync + ?Sized> TerminalResourceService<TR> {
    pub fn new(terminal_repo: Arc<TR>) -> Self {
        Self {
            terminal_repo,
            field_overlay_repo: None,
            reference_repo: None,
            attribute_writer: None,
            stand_repo: None,
        }
    }

    pub fn with_field_overlay_repository(mut self, repo: Arc<dyn FieldOverlayRepository + Send + Sync>) -> Self {
        self.field_overlay_repo = Some(repo);
        self
    }

    /// 注入机位仓储以启用组成不变量的全量校验（成环 DFS / 双父）。
    pub fn with_stand_repository(mut self, repo: Arc<dyn StandRepository + Send + Sync>) -> Self {
        self.stand_repo = Some(repo);
        self
    }

    pub fn with_reference_repository(
        mut self,
        repo: Arc<dyn OntologyAttributeReferenceRepository + Send + Sync>,
    ) -> Self {
        self.reference_repo = Some(repo);
        self
    }

    pub fn with_attribute_writer(mut self, writer: Arc<dyn TerminalResourceAttributeTransactionalWriter>) -> Self {
        self.attribute_writer = Some(writer);
        self
    }

    // ------------------------------------------------------------ Terminal --
    pub async fn list_terminals(&self, include_inactive: bool) -> Result<Vec<Terminal>, DomainError> {
        self.terminal_repo.find_terminals(include_inactive).await
    }

    pub async fn get_terminal(&self, terminal_id: &str) -> Result<Option<Terminal>, DomainError> {
        self.terminal_repo.find_terminal_by_id(terminal_id).await
    }

    pub async fn create_terminal(&self, payload: TerminalCreate) -> Result<Terminal, DomainError> {
        let attributes = self.validate_attributes_for("Terminal", payload.attributes).await?;
        self.validate_object_references("Terminal", &attributes).await?;
        let terminal = Terminal {
            terminal_id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            name: require_non_empty(&payload.name, "name")?,
            is_active: true,
            created_at: None,
            updated_at: None,
            attributes,
        };
        let saved = if let Some(writer) = self.attribute_writer.as_ref() {
            let references = collect_attribute_references(
                "Terminal",
                &terminal.terminal_id,
                &terminal.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_terminal_with_references(&terminal, &references).await?
        } else {
            self.terminal_repo.save_terminal(&terminal).await?
        };
        // writer 路径已在同一 UnitOfWork 内写 owner + reference index；
        // 仅无 writer 的替代路径需要这里的非事务同步兜底。
        if self.attribute_writer.is_none() {
            sync_attribute_references(
                "Terminal",
                &saved.terminal_id,
                &saved.attributes,
                self.field_overlay_repo.as_ref(),
                self.reference_repo.as_ref(),
            )
            .await?;
        }
        Ok(saved)
    }

    pub async fn update_terminal(&self, terminal_id: &str, payload: TerminalUpdate) -> Result<Terminal, DomainError> {
        let mut terminal = self
            .terminal_repo
            .find_terminal_by_id(terminal_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "terminal",
                id: terminal_id.to_string(),
            })?;
        if let Some(name) = payload.name {
            terminal.name = require_non_empty(&name, "name")?;
        }
        if let Some(attributes) = payload.attributes {
            terminal.attributes = self.validate_attributes_for("Terminal", attributes).await?;
            self.validate_object_references("Terminal", &terminal.attributes)
                .await?;
        }
        let saved = if let Some(writer) = self.attribute_writer.as_ref() {
            let references = collect_attribute_references(
                "Terminal",
                &terminal.terminal_id,
                &terminal.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_terminal_with_references(&terminal, &references).await?
        } else {
            self.terminal_repo.save_terminal(&terminal).await?
        };
        if self.attribute_writer.is_none() {
            sync_attribute_references(
                "Terminal",
                &saved.terminal_id,
                &saved.attributes,
                self.field_overlay_repo.as_ref(),
                self.reference_repo.as_ref(),
            )
            .await?;
        }
        Ok(saved)
    }

    /// 停用楼。若有任一成员存在未结束占用/分配 → 409 带明细。
    pub async fn deactivate_terminal(&self, terminal_id: &str) -> Result<Terminal, DomainError> {
        let terminal = self
            .terminal_repo
            .find_terminal_by_id(terminal_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "terminal",
                id: terminal_id.to_string(),
            })?;
        if !terminal.is_active {
            return Ok(terminal);
        }
        reject_referenced_target(
            self.reference_repo.as_ref(),
            "Terminal",
            &terminal.code,
            "停用楼失败：仍被 object_ref 引用",
        )
        .await?;

        let directory = self
            .terminal_repo
            .terminal_directory(terminal_id)
            .await?
            .expect("terminal exists; directory must resolve");

        let mut conflicts: Vec<Value> = Vec::new();
        for stand in &directory.stands {
            let mut occ = self.terminal_repo.active_stand_occupations(&stand.code).await?;
            conflicts.append(&mut occ);
        }
        for gate in &directory.gates {
            let mut occ = self.terminal_repo.active_gate_assignments(&gate.code).await?;
            conflicts.append(&mut occ);
        }
        for carousel in &directory.carousels {
            let mut occ = self.terminal_repo.active_carousel_assignments(&carousel.code).await?;
            conflicts.append(&mut occ);
        }
        if !conflicts.is_empty() {
            return Err(conflict_with_details(
                "停用楼失败：存在未结束占用/分配",
                Value::Array(conflicts),
            ));
        }

        if let Some(writer) = self.attribute_writer.as_ref() {
            let mut terminal = terminal;
            terminal.is_active = false;
            let references = collect_attribute_references(
                "Terminal",
                &terminal.terminal_id,
                &terminal.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_terminal_with_references(&terminal, &references).await;
        }
        self.terminal_repo
            .set_terminal_active(terminal_id, false)
            .await?
            .ok_or_else(|| DomainError::Internal("deactivate terminal returned no row".into()))
    }

    // ---------------------------------------------------------------- Gate --
    pub async fn create_gate(&self, payload: GateCreate) -> Result<Gate, DomainError> {
        let terminal_id = require_non_empty(&payload.terminal_id, "terminal_id")?;
        let terminal = self
            .terminal_repo
            .find_terminal_by_id(&terminal_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "terminal",
                id: terminal_id.clone(),
            })?;
        if !terminal.is_active {
            return Err(DomainError::BusinessRuleViolation(
                "不能向已停用航站楼添加登机口".into(),
            ));
        }

        // 目录行 + 成员行原子建立：同一仓储内先插目录行，再写成员表。
        let gate = Gate {
            gate_id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            name: payload
                .name
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty()),
            is_active: true,
            created_at: None,
            updated_at: None,
            attributes: self.validate_attributes_for("Gate", payload.attributes).await?,
        };
        self.validate_object_references("Gate", &gate.attributes).await?;
        let gate = if let Some(writer) = self.attribute_writer.as_ref() {
            let references = collect_attribute_references(
                "Gate",
                &gate.gate_id,
                &gate.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer
                .save_gate_with_terminal_and_references(&terminal_id, &gate, &references)
                .await?
        } else {
            self.terminal_repo.save_gate(&gate).await?
        };
        // 无 writer 兜底路径：owner 保存后补齐成员关系与 reference index。
        if self.attribute_writer.is_none() {
            sync_attribute_references(
                "Gate",
                &gate.gate_id,
                &gate.attributes,
                self.field_overlay_repo.as_ref(),
                self.reference_repo.as_ref(),
            )
            .await?;
            self.terminal_repo.add_gate(&terminal_id, &gate.gate_id).await?;
        }
        Ok(gate)
    }

    pub async fn update_gate(&self, gate_id: &str, payload: GateUpdate) -> Result<Gate, DomainError> {
        let mut gate = self
            .terminal_repo
            .find_gate_by_id(gate_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "gate",
                id: gate_id.to_string(),
            })?;
        if let Some(name) = payload.name {
            gate.name = Some(require_non_empty(&name, "name")?);
        }
        if let Some(is_active) = payload.is_active {
            // 停用须走与 deactivate_gate 相同的占用/引用检查，避免 update 旁路。
            if !is_active && gate.is_active {
                reject_referenced_target(
                    self.reference_repo.as_ref(),
                    "Gate",
                    &gate.code,
                    "停用登机口失败：仍被 object_ref 引用",
                )
                .await?;
                let conflicts = self.terminal_repo.active_gate_assignments(&gate.code).await?;
                if !conflicts.is_empty() {
                    return Err(conflict_with_details(
                        "停用登机口失败：存在未结束分配",
                        Value::Array(conflicts),
                    ));
                }
            }
            gate.is_active = is_active;
        }
        if let Some(attributes) = payload.attributes {
            gate.attributes = self.validate_attributes_for("Gate", attributes).await?;
            self.validate_object_references("Gate", &gate.attributes).await?;
        }
        let saved = if let Some(writer) = self.attribute_writer.as_ref() {
            let references = collect_attribute_references(
                "Gate",
                &gate.gate_id,
                &gate.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_gate_with_references(&gate, &references).await?
        } else {
            self.terminal_repo.save_gate(&gate).await?
        };
        if self.attribute_writer.is_none() {
            sync_attribute_references(
                "Gate",
                &saved.gate_id,
                &saved.attributes,
                self.field_overlay_repo.as_ref(),
                self.reference_repo.as_ref(),
            )
            .await?;
        }
        Ok(saved)
    }

    /// 停用登机口。若存在未结束分配 → 409 带明细。
    pub async fn deactivate_gate(&self, gate_id: &str) -> Result<Gate, DomainError> {
        let gate = self
            .terminal_repo
            .find_gate_by_id(gate_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "gate",
                id: gate_id.to_string(),
            })?;
        if !gate.is_active {
            return Ok(gate);
        }
        reject_referenced_target(
            self.reference_repo.as_ref(),
            "Gate",
            &gate.code,
            "停用登机口失败：仍被 object_ref 引用",
        )
        .await?;
        let conflicts = self.terminal_repo.active_gate_assignments(&gate.code).await?;
        if !conflicts.is_empty() {
            return Err(conflict_with_details(
                "停用登机口失败：存在未结束分配",
                Value::Array(conflicts),
            ));
        }
        if let Some(writer) = self.attribute_writer.as_ref() {
            let mut gate = gate;
            gate.is_active = false;
            let references = collect_attribute_references(
                "Gate",
                &gate.gate_id,
                &gate.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_gate_with_references(&gate, &references).await;
        }
        self.terminal_repo
            .set_gate_active(gate_id, false)
            .await?
            .ok_or_else(|| DomainError::Internal("deactivate gate returned no row".into()))
    }

    // ------------------------------------------------------------ Carousel --
    pub async fn create_carousel(&self, payload: CarouselCreate) -> Result<BaggageCarousel, DomainError> {
        let terminal_id = require_non_empty(&payload.terminal_id, "terminal_id")?;
        let terminal = self
            .terminal_repo
            .find_terminal_by_id(&terminal_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "terminal",
                id: terminal_id.clone(),
            })?;
        if !terminal.is_active {
            return Err(DomainError::BusinessRuleViolation(
                "不能向已停用航站楼添加行李转盘".into(),
            ));
        }

        let carousel = BaggageCarousel {
            carousel_id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            name: payload
                .name
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty()),
            is_active: true,
            created_at: None,
            updated_at: None,
            attributes: self
                .validate_attributes_for("BaggageCarousel", payload.attributes)
                .await?,
        };
        self.validate_object_references("BaggageCarousel", &carousel.attributes)
            .await?;
        let carousel = if let Some(writer) = self.attribute_writer.as_ref() {
            let references = collect_attribute_references(
                "BaggageCarousel",
                &carousel.carousel_id,
                &carousel.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer
                .save_carousel_with_terminal_and_references(&terminal_id, &carousel, &references)
                .await?
        } else {
            self.terminal_repo.save_carousel(&carousel).await?
        };
        // 无 writer 兜底路径：owner 保存后补齐成员关系与 reference index。
        if self.attribute_writer.is_none() {
            sync_attribute_references(
                "BaggageCarousel",
                &carousel.carousel_id,
                &carousel.attributes,
                self.field_overlay_repo.as_ref(),
                self.reference_repo.as_ref(),
            )
            .await?;
            self.terminal_repo
                .add_carousel(&terminal_id, &carousel.carousel_id)
                .await?;
        }
        Ok(carousel)
    }

    pub async fn update_carousel(
        &self,
        carousel_id: &str,
        payload: CarouselUpdate,
    ) -> Result<BaggageCarousel, DomainError> {
        let mut carousel = self
            .terminal_repo
            .find_carousel_by_id(carousel_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "carousel",
                id: carousel_id.to_string(),
            })?;
        if let Some(name) = payload.name {
            carousel.name = Some(require_non_empty(&name, "name")?);
        }
        if let Some(is_active) = payload.is_active {
            // 停用须走与 deactivate_carousel 相同的占用/引用检查，避免 update 旁路。
            if !is_active && carousel.is_active {
                reject_referenced_target(
                    self.reference_repo.as_ref(),
                    "BaggageCarousel",
                    &carousel.code,
                    "停用行李转盘失败：仍被 object_ref 引用",
                )
                .await?;
                let conflicts = self.terminal_repo.active_carousel_assignments(&carousel.code).await?;
                if !conflicts.is_empty() {
                    return Err(conflict_with_details(
                        "停用行李转盘失败：存在未结束分配",
                        Value::Array(conflicts),
                    ));
                }
            }
            carousel.is_active = is_active;
        }
        if let Some(attributes) = payload.attributes {
            carousel.attributes = self.validate_attributes_for("BaggageCarousel", attributes).await?;
            self.validate_object_references("BaggageCarousel", &carousel.attributes)
                .await?;
        }
        let saved = if let Some(writer) = self.attribute_writer.as_ref() {
            let references = collect_attribute_references(
                "BaggageCarousel",
                &carousel.carousel_id,
                &carousel.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_carousel_with_references(&carousel, &references).await?
        } else {
            self.terminal_repo.save_carousel(&carousel).await?
        };
        if self.attribute_writer.is_none() {
            sync_attribute_references(
                "BaggageCarousel",
                &saved.carousel_id,
                &saved.attributes,
                self.field_overlay_repo.as_ref(),
                self.reference_repo.as_ref(),
            )
            .await?;
        }
        Ok(saved)
    }

    /// 停用行李转盘。若存在未结束分配 → 409 带明细。
    pub async fn deactivate_carousel(&self, carousel_id: &str) -> Result<BaggageCarousel, DomainError> {
        let carousel = self
            .terminal_repo
            .find_carousel_by_id(carousel_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "carousel",
                id: carousel_id.to_string(),
            })?;
        if !carousel.is_active {
            return Ok(carousel);
        }
        reject_referenced_target(
            self.reference_repo.as_ref(),
            "BaggageCarousel",
            &carousel.code,
            "停用行李转盘失败：仍被 object_ref 引用",
        )
        .await?;
        let conflicts = self.terminal_repo.active_carousel_assignments(&carousel.code).await?;
        if !conflicts.is_empty() {
            return Err(conflict_with_details(
                "停用行李转盘失败：存在未结束分配",
                Value::Array(conflicts),
            ));
        }
        if let Some(writer) = self.attribute_writer.as_ref() {
            let mut carousel = carousel;
            carousel.is_active = false;
            let references = collect_attribute_references(
                "BaggageCarousel",
                &carousel.carousel_id,
                &carousel.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_carousel_with_references(&carousel, &references).await;
        }
        self.terminal_repo
            .set_carousel_active(carousel_id, false)
            .await?
            .ok_or_else(|| DomainError::Internal("deactivate carousel returned no row".into()))
    }

    // --------------------------------------------------------------- Stand --
    /// 新建机位目录行并立刻挂到启用楼（同一服务调用内建行 + 成员表）。
    pub async fn create_stand(&self, payload: StandCreate) -> Result<Stand, DomainError> {
        let attributes = self.validate_attributes(payload.attributes.clone()).await?;
        self.validate_object_references("Stand", &attributes).await?;
        let terminal_id = require_non_empty(&payload.terminal_id, "terminal_id")?;
        let stand_code = require_non_empty(&payload.code, "code")?;
        self.validate_stand_composition(&stand_code, &attributes).await?;
        let terminal = self
            .terminal_repo
            .find_terminal_by_id(&terminal_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "terminal",
                id: terminal_id.clone(),
            })?;
        if !terminal.is_active {
            return Err(DomainError::BusinessRuleViolation("不能向已停用航站楼添加机位".into()));
        }
        self.validate_stand_gate_terminal(&terminal.code, &attributes).await?;

        let stand = Stand {
            id: ulid::Ulid::new().to_string(),
            code: stand_code,
            name: payload
                .name
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty()),
            terminal: Some(terminal.code.clone()),
            area: payload
                .area
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty()),
            position_lat: payload.position_lat,
            position_lng: payload.position_lng,
            stand_type: payload
                .stand_type
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty()),
            size_category: payload
                .size_category
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty()),
            attributes,
            is_active: true,
            created_at: None,
        };
        let stand = if let Some(writer) = self.attribute_writer.as_ref() {
            let references =
                collect_attribute_references("Stand", &stand.id, &stand.attributes, self.field_overlay_repo.as_ref())
                    .await?;
            writer
                .save_stand_with_terminal_and_references(&terminal_id, &stand, &references)
                .await?
        } else {
            self.terminal_repo.save_stand(&stand).await?
        };
        // 无 writer 兜底路径：owner 保存后补齐成员关系与 reference index。
        if self.attribute_writer.is_none() {
            sync_attribute_references(
                "Stand",
                &stand.id,
                &stand.attributes,
                self.field_overlay_repo.as_ref(),
                self.reference_repo.as_ref(),
            )
            .await?;
            self.terminal_repo.add_stand(&terminal_id, &stand.id).await?;
        }
        Ok(stand)
    }

    pub async fn update_stand(&self, stand_id: &str, payload: StandUpdate) -> Result<Stand, DomainError> {
        let mut stand = self
            .terminal_repo
            .find_stand_by_id(stand_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "stand",
                id: stand_id.to_string(),
            })?;
        if let Some(name) = payload.name {
            stand.name = Some(name.trim().to_string()).filter(|v| !v.is_empty());
        }
        if let Some(area) = payload.area {
            stand.area = Some(area.trim().to_string()).filter(|v| !v.is_empty());
        }
        if let Some(lat) = payload.position_lat {
            stand.position_lat = lat;
        }
        if let Some(lng) = payload.position_lng {
            stand.position_lng = lng;
        }
        if let Some(stand_type) = payload.stand_type {
            stand.stand_type = Some(stand_type.trim().to_string()).filter(|v| !v.is_empty());
        }
        if let Some(size_category) = payload.size_category {
            stand.size_category = Some(size_category.trim().to_string()).filter(|v| !v.is_empty());
        }
        if let Some(is_active) = payload.is_active {
            // 停用须走与 deactivate_stand 相同的占用/引用检查，避免 update 旁路。
            if !is_active && stand.is_active {
                reject_referenced_target(
                    self.reference_repo.as_ref(),
                    "Stand",
                    &stand.code,
                    "停用机位失败：仍被 object_ref 引用",
                )
                .await?;
                let conflicts = self.terminal_repo.active_stand_occupations(&stand.code).await?;
                if !conflicts.is_empty() {
                    return Err(conflict_with_details(
                        "停用机位失败：存在未结束占用",
                        Value::Array(conflicts),
                    ));
                }
            }
            stand.is_active = is_active;
        }
        if let Some(attributes) = payload.attributes {
            stand.attributes = self.validate_attributes(attributes).await?;
            self.validate_object_references("Stand", &stand.attributes).await?;
        }
        self.validate_stand_composition(&stand.code, &stand.attributes).await?;
        if let Some(terminal_code) = stand.terminal.as_deref() {
            self.validate_stand_gate_terminal(terminal_code, &stand.attributes)
                .await?;
        }
        let stand = if let Some(writer) = self.attribute_writer.as_ref() {
            let references =
                collect_attribute_references("Stand", &stand.id, &stand.attributes, self.field_overlay_repo.as_ref())
                    .await?;
            writer.save_stand_with_references(&stand, &references).await?
        } else {
            self.terminal_repo.save_stand(&stand).await?
        };
        if self.attribute_writer.is_none() {
            sync_attribute_references(
                "Stand",
                &stand.id,
                &stand.attributes,
                self.field_overlay_repo.as_ref(),
                self.reference_repo.as_ref(),
            )
            .await?;
        }
        Ok(stand)
    }

    async fn validate_stand_composition(&self, stand_code: &str, attributes: &Value) -> Result<(), DomainError> {
        let composed_of = composed_of_codes(attributes);
        if composed_of.is_empty() {
            return Ok(());
        }
        // 资源管理写入路径与派工资源写入路径共用同一份组成不变量
        // （自引用 / 子不存在或停用 / 成环 / 双父，全部 409）。
        if let Some(stand_repo) = self.stand_repo.as_ref() {
            let stands = stand_repo.find_all(None, true, 500, 0).await?;
            return validate_composition_snapshot(stand_code, &composed_of, &stands);
        }
        // 未注入机位仓储的降级路径：仅挡自引用 / 子不存在 / 子停用 / 一层互指。
        for child_code in composed_of.iter() {
            if child_code.eq_ignore_ascii_case(stand_code) {
                return Err(DomainError::Conflict("机位 composed_of 不能引用自身".into()));
            }
            let child = self
                .terminal_repo
                .find_stand_by_code(child_code)
                .await?
                .ok_or_else(|| DomainError::Conflict(format!("组成机位 {child_code} 不存在")))?;
            if !child.is_active {
                return Err(DomainError::Conflict(format!("组成机位 {child_code} 已停用")));
            }
            if child
                .attributes
                .get("composed_of")
                .and_then(Value::as_array)
                .map(|nested| nested.iter().any(|item| item.as_str() == Some(stand_code)))
                .unwrap_or(false)
            {
                return Err(DomainError::Conflict(format!(
                    "机位组成关系会形成环: {stand_code} ↔ {child_code}"
                )));
            }
        }
        Ok(())
    }

    async fn validate_attributes(&self, value: Value) -> Result<Value, DomainError> {
        self.validate_attributes_for("Stand", value).await
    }

    async fn validate_attributes_for(&self, object_name: &str, value: Value) -> Result<Value, DomainError> {
        validate_attributes(object_name, value, self.field_overlay_repo.as_ref()).await
    }

    /// Resolve object_ref/object_ref[] targets against the same directory
    /// repository before persisting attributes. Unknown target object kinds
    /// are left to their owning resource service; directory-owned targets are
    /// always checked here for existence and active status.
    async fn validate_object_references(&self, object_name: &str, attributes: &Value) -> Result<(), DomainError> {
        let (Some(field_repo), Some(map)) = (self.field_overlay_repo.as_ref(), attributes.as_object()) else {
            return Ok(());
        };
        let overlays = field_repo.list(Some(object_name), false).await?;
        for field in overlays.iter().filter(|item| item.is_active) {
            let Some(field_type) = OntologyFieldType::parse(&field.field_type) else {
                continue;
            };
            if !field_type.is_object() {
                continue;
            }
            let Some(target_name) = field.object_name_target.as_deref() else {
                continue;
            };
            let Some(raw) = map.get(&field.field_name) else {
                continue;
            };
            let keys: Vec<&str> = match field_type {
                OntologyFieldType::ObjectRef => raw.as_str().into_iter().collect(),
                OntologyFieldType::ObjectRefArray => raw
                    .as_array()
                    .map(|items| items.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            for key in keys {
                let active = match target_name {
                    "Terminal" => self
                        .terminal_repo
                        .find_terminal_by_code(key)
                        .await?
                        .map(|v| v.is_active),
                    "Gate" => self.terminal_repo.find_gate_by_code(key).await?.map(|v| v.is_active),
                    "BaggageCarousel" => self
                        .terminal_repo
                        .find_carousel_by_code(key)
                        .await?
                        .map(|v| v.is_active),
                    "Stand" => self.terminal_repo.find_stand_by_code(key).await?.map(|v| v.is_active),
                    _ => None,
                };
                if active != Some(true) {
                    return Err(DomainError::Conflict(format!(
                        "扩展字段 {object_name}.{} 引用了不存在或已停用的 {target_name}: {key}",
                        field.field_name
                    )));
                }
            }
        }
        Ok(())
    }

    async fn validate_stand_gate_terminal(&self, terminal_code: &str, attributes: &Value) -> Result<(), DomainError> {
        let (Some(field_repo), Some(map)) = (self.field_overlay_repo.as_ref(), attributes.as_object()) else {
            return Ok(());
        };
        let overlays = field_repo.list(Some("Stand"), false).await?;
        let remote = map.get("stand_use").and_then(Value::as_str) == Some("remote")
            || map.get("use").and_then(Value::as_str) == Some("remote");
        for field in overlays
            .iter()
            .filter(|item| item.is_active && item.object_name_target.as_deref() == Some("Gate"))
        {
            let Some(raw) = map.get(&field.field_name) else {
                continue;
            };
            if remote {
                return Err(DomainError::ValidationError(format!(
                    "远机位不能填写登机口: Stand.{}",
                    field.field_name
                )));
            }
            let keys: Vec<&str> = match OntologyFieldType::parse(&field.field_type) {
                Some(OntologyFieldType::ObjectRef) => raw.as_str().into_iter().collect(),
                Some(OntologyFieldType::ObjectRefArray) => raw
                    .as_array()
                    .map(|items| items.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default(),
                _ => continue,
            };
            for key in keys {
                match self.terminal_repo.gate_locale_by_code(key).await? {
                    fms_domain::ports::dispatch_repository::FacilityLocale::Terminal { code, .. }
                        if code == terminal_code => {}
                    _ => {
                        return Err(DomainError::ValidationError(format!(
                            "扩展字段 Stand.{} 引用的登机口必须与机位位于同一航站楼: {key}",
                            field.field_name
                        )))
                    }
                }
            }
        }
        Ok(())
    }

    /// 停用机位。若存在未结束占用 → 409 带明细。
    pub async fn deactivate_stand(&self, stand_id: &str) -> Result<Stand, DomainError> {
        let stand = self
            .terminal_repo
            .find_stand_by_id(stand_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "stand",
                id: stand_id.to_string(),
            })?;
        if !stand.is_active {
            return Ok(stand);
        }
        reject_referenced_target(
            self.reference_repo.as_ref(),
            "Stand",
            &stand.code,
            "停用机位失败：仍被 object_ref 引用",
        )
        .await?;
        let conflicts = self.terminal_repo.active_stand_occupations(&stand.code).await?;
        if !conflicts.is_empty() {
            return Err(conflict_with_details(
                "停用机位失败：存在未结束占用",
                Value::Array(conflicts),
            ));
        }
        self.terminal_repo
            .set_stand_active(stand_id, false)
            .await?
            .ok_or_else(|| DomainError::Internal("deactivate stand returned no row".into()))
    }

    // ------------------------------------------------------------ members --
    /// 把既有机位挂到某座启用楼。新机位建档走 `create_stand`（建行 + 挂楼）。
    pub async fn add_stand_member(&self, terminal_id: &str, stand_id: &str) -> Result<(), DomainError> {
        self.ensure_terminal_active(terminal_id).await?;
        self.terminal_repo.add_stand(terminal_id, stand_id).await
    }

    /// 从楼里移出机位；有未结束占用 → 409。
    pub async fn remove_stand_member(&self, stand_id: &str) -> Result<(), DomainError> {
        let stand = self
            .terminal_repo
            .find_stand_by_id(stand_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "stand",
                id: stand_id.to_string(),
            })?;
        let conflicts = self.terminal_repo.active_stand_occupations(&stand.code).await?;
        if !conflicts.is_empty() {
            return Err(conflict_with_details(
                "移出机位失败：存在未结束占用",
                Value::Array(conflicts),
            ));
        }
        self.terminal_repo.remove_stand(stand_id).await
    }

    pub async fn add_gate_member(&self, terminal_id: &str, gate_id: &str) -> Result<(), DomainError> {
        self.ensure_terminal_active(terminal_id).await?;
        self.terminal_repo.add_gate(terminal_id, gate_id).await
    }

    pub async fn remove_gate_member(&self, gate_id: &str) -> Result<(), DomainError> {
        let gate = self
            .terminal_repo
            .find_gate_by_id(gate_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "gate",
                id: gate_id.to_string(),
            })?;
        let conflicts = self.terminal_repo.active_gate_assignments(&gate.code).await?;
        if !conflicts.is_empty() {
            return Err(conflict_with_details(
                "移出登机口失败：存在未结束分配",
                Value::Array(conflicts),
            ));
        }
        self.terminal_repo.remove_gate(gate_id).await
    }

    pub async fn add_carousel_member(&self, terminal_id: &str, carousel_id: &str) -> Result<(), DomainError> {
        self.ensure_terminal_active(terminal_id).await?;
        self.terminal_repo.add_carousel(terminal_id, carousel_id).await
    }

    pub async fn remove_carousel_member(&self, carousel_id: &str) -> Result<(), DomainError> {
        let carousel = self
            .terminal_repo
            .find_carousel_by_id(carousel_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "carousel",
                id: carousel_id.to_string(),
            })?;
        let conflicts = self.terminal_repo.active_carousel_assignments(&carousel.code).await?;
        if !conflicts.is_empty() {
            return Err(conflict_with_details(
                "移出转盘失败：存在未结束分配",
                Value::Array(conflicts),
            ));
        }
        self.terminal_repo.remove_carousel(carousel_id).await
    }

    // ------------------------------------------------------------ context --
    /// 只读：楼 + 三类成员目录行。楼不存在返回 Ok(None)。
    pub async fn get_context(&self, terminal_id: &str) -> Result<Option<TerminalDirectory>, DomainError> {
        self.terminal_repo.terminal_directory(terminal_id).await
    }

    // ------------------------------------------------------------ helpers --
    async fn ensure_terminal_active(&self, terminal_id: &str) -> Result<(), DomainError> {
        let terminal = self
            .terminal_repo
            .find_terminal_by_id(terminal_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "terminal",
                id: terminal_id.to_string(),
            })?;
        if !terminal.is_active {
            return Err(DomainError::BusinessRuleViolation("不能向已停用航站楼添加成员".into()));
        }
        Ok(())
    }
}
