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

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    BaggageCarousel, Gate, Stand, Terminal, TerminalDirectory,
};
use fms_domain::ports::dispatch_repository::TerminalRepository;

use super::schemas::{
    CarouselCreate, CarouselUpdate, GateCreate, GateUpdate, StandCreate, StandUpdate, TerminalCreate, TerminalUpdate,
};

pub struct TerminalResourceService<TR: TerminalRepository + ?Sized> {
    terminal_repo: Arc<TR>,
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

impl<TR: TerminalRepository + ?Sized> TerminalResourceService<TR> {
    pub fn new(terminal_repo: Arc<TR>) -> Self {
        Self { terminal_repo }
    }

    // ------------------------------------------------------------ Terminal --
    pub async fn list_terminals(&self, include_inactive: bool) -> Result<Vec<Terminal>, DomainError> {
        self.terminal_repo.find_terminals(include_inactive).await
    }

    pub async fn get_terminal(&self, terminal_id: &str) -> Result<Option<Terminal>, DomainError> {
        self.terminal_repo.find_terminal_by_id(terminal_id).await
    }

    pub async fn create_terminal(&self, payload: TerminalCreate) -> Result<Terminal, DomainError> {
        let terminal = Terminal {
            terminal_id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            name: require_non_empty(&payload.name, "name")?,
            is_active: true,
            created_at: None,
            updated_at: None,
        };
        self.terminal_repo.save_terminal(&terminal).await
    }

    pub async fn update_terminal(
        &self,
        terminal_id: &str,
        payload: TerminalUpdate,
    ) -> Result<Terminal, DomainError> {
        let mut terminal = self
            .terminal_repo
            .find_terminal_by_id(terminal_id)
            .await?
            .ok_or_else(|| DomainError::NotFound { entity_type: "terminal", id: terminal_id.to_string() })?;
        if let Some(name) = payload.name {
            terminal.name = require_non_empty(&name, "name")?;
        }
        self.terminal_repo.save_terminal(&terminal).await
    }

    /// 停用楼。若有任一成员存在未结束占用/分配 → 409 带明细。
    pub async fn deactivate_terminal(&self, terminal_id: &str) -> Result<Terminal, DomainError> {
        let terminal = self
            .terminal_repo
            .find_terminal_by_id(terminal_id)
            .await?
            .ok_or_else(|| DomainError::NotFound { entity_type: "terminal", id: terminal_id.to_string() })?;
        if !terminal.is_active {
            return Ok(terminal);
        }

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
            return Err(conflict_with_details("停用楼失败：存在未结束占用/分配", Value::Array(conflicts)));
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
            .ok_or_else(|| DomainError::NotFound { entity_type: "terminal", id: terminal_id.clone() })?;
        if !terminal.is_active {
            return Err(DomainError::BusinessRuleViolation(
                "不能向已停用航站楼添加登机口".into(),
            ));
        }

        // 目录行 + 成员行原子建立：同一仓储内先插目录行，再写成员表。
        let gate = Gate {
            gate_id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            name: payload.name.map(|value| value.trim().to_string()).filter(|v| !v.is_empty()),
            is_active: true,
            created_at: None,
            updated_at: None,
        };
        let gate = self.terminal_repo.save_gate(&gate).await?;
        self.terminal_repo.add_gate(&terminal_id, &gate.gate_id).await?;
        Ok(gate)
    }

    pub async fn update_gate(&self, gate_id: &str, payload: GateUpdate) -> Result<Gate, DomainError> {
        let mut gate = self
            .terminal_repo
            .find_gate_by_id(gate_id)
            .await?
            .ok_or_else(|| DomainError::NotFound { entity_type: "gate", id: gate_id.to_string() })?;
        if let Some(name) = payload.name {
            gate.name = Some(require_non_empty(&name, "name")?);
        }
        if let Some(is_active) = payload.is_active {
            gate.is_active = is_active;
        }
        self.terminal_repo.save_gate(&gate).await
    }

    /// 停用登机口。若存在未结束分配 → 409 带明细。
    pub async fn deactivate_gate(&self, gate_id: &str) -> Result<Gate, DomainError> {
        let gate = self
            .terminal_repo
            .find_gate_by_id(gate_id)
            .await?
            .ok_or_else(|| DomainError::NotFound { entity_type: "gate", id: gate_id.to_string() })?;
        if !gate.is_active {
            return Ok(gate);
        }
        let conflicts = self.terminal_repo.active_gate_assignments(&gate.code).await?;
        if !conflicts.is_empty() {
            return Err(conflict_with_details("停用登机口失败：存在未结束分配", Value::Array(conflicts)));
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
            .ok_or_else(|| DomainError::NotFound { entity_type: "terminal", id: terminal_id.clone() })?;
        if !terminal.is_active {
            return Err(DomainError::BusinessRuleViolation(
                "不能向已停用航站楼添加行李转盘".into(),
            ));
        }

        let carousel = BaggageCarousel {
            carousel_id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            name: payload.name.map(|value| value.trim().to_string()).filter(|v| !v.is_empty()),
            is_active: true,
            created_at: None,
            updated_at: None,
        };
        let carousel = self.terminal_repo.save_carousel(&carousel).await?;
        self.terminal_repo.add_carousel(&terminal_id, &carousel.carousel_id).await?;
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
            .ok_or_else(|| DomainError::NotFound { entity_type: "carousel", id: carousel_id.to_string() })?;
        if let Some(name) = payload.name {
            carousel.name = Some(require_non_empty(&name, "name")?);
        }
        if let Some(is_active) = payload.is_active {
            carousel.is_active = is_active;
        }
        self.terminal_repo.save_carousel(&carousel).await
    }

    /// 停用行李转盘。若存在未结束分配 → 409 带明细。
    pub async fn deactivate_carousel(&self, carousel_id: &str) -> Result<BaggageCarousel, DomainError> {
        let carousel = self
            .terminal_repo
            .find_carousel_by_id(carousel_id)
            .await?
            .ok_or_else(|| DomainError::NotFound { entity_type: "carousel", id: carousel_id.to_string() })?;
        if !carousel.is_active {
            return Ok(carousel);
        }
        let conflicts = self.terminal_repo.active_carousel_assignments(&carousel.code).await?;
        if !conflicts.is_empty() {
            return Err(conflict_with_details("停用行李转盘失败：存在未结束分配", Value::Array(conflicts)));
        }
        self.terminal_repo
            .set_carousel_active(carousel_id, false)
            .await?
            .ok_or_else(|| DomainError::Internal("deactivate carousel returned no row".into()))
    }

    // --------------------------------------------------------------- Stand --
    /// 新建机位目录行并立刻挂到启用楼（同一服务调用内建行 + 成员表）。
    pub async fn create_stand(&self, payload: StandCreate) -> Result<Stand, DomainError> {
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
                "不能向已停用航站楼添加机位".into(),
            ));
        }

        let stand = Stand {
            id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
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
            is_active: true,
            created_at: None,
        };
        let stand = self.terminal_repo.save_stand(&stand).await?;
        self.terminal_repo.add_stand(&terminal_id, &stand.id).await?;
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
            stand.is_active = is_active;
        }
        self.terminal_repo.save_stand(&stand).await
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
            .ok_or_else(|| DomainError::NotFound { entity_type: "stand", id: stand_id.to_string() })?;
        let conflicts = self.terminal_repo.active_stand_occupations(&stand.code).await?;
        if !conflicts.is_empty() {
            return Err(conflict_with_details("移出机位失败：存在未结束占用", Value::Array(conflicts)));
        }
        self.terminal_repo.remove_stand(stand_id).await
    }

    pub async fn add_gate_member(&self, terminal_id: &str, gate_id: &str) -> Result<(), DomainError> {
        self.ensure_terminal_active(terminal_id).await?;
        self.terminal_repo.add_gate(terminal_id, gate_id).await
    }

    pub async fn remove_gate_member(&self, gate_id: &str) -> Result<(), DomainError> {
        self.terminal_repo.remove_gate(gate_id).await
    }

    pub async fn add_carousel_member(&self, terminal_id: &str, carousel_id: &str) -> Result<(), DomainError> {
        self.ensure_terminal_active(terminal_id).await?;
        self.terminal_repo.add_carousel(terminal_id, carousel_id).await
    }

    pub async fn remove_carousel_member(&self, carousel_id: &str) -> Result<(), DomainError> {
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
            .ok_or_else(|| DomainError::NotFound { entity_type: "terminal", id: terminal_id.to_string() })?;
        if !terminal.is_active {
            return Err(DomainError::BusinessRuleViolation("不能向已停用航站楼添加成员".into()));
        }
        Ok(())
    }
}