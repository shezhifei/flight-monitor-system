use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::ports::dispatch_repository::EquipmentRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{evidence, required_str};

/// `Equipment.get_context`：读取单台设备的资源档案（含设备类型）。只读、不进执行器；
/// 由 `dispatch_read_action` 经 `read_action_permission` 调用。
pub struct EquipmentContextService {
    equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
}

impl EquipmentContextService {
    pub fn new(equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>) -> Self {
        Self { equipment_repo }
    }

    pub async fn get(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let equipment_id = required_str(args, "equipment_id")?;

        let equipment = self
            .equipment_repo
            .find_by_id(equipment_id)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| OntologyActionError::NotFound(format!("equipment {equipment_id}")))?;

        // 只读投影，不含维护内部字段的敏感扩展。
        let profile = json!({
            "equipment_id": equipment.id,
            "code": equipment.code,
            "name": equipment.name,
            "license_plate": equipment.license_plate,
            "status": equipment.status,
            "current_dispatch_id": equipment.current_dispatch_id,
            "current_stand_id": equipment.current_stand_id,
            "current_position_lat": equipment.current_position_lat,
            "current_position_lng": equipment.current_position_lng,
            "last_position_update": equipment.last_position_update,
            "is_active": equipment.is_active,
            "equipment_type_id": equipment.equipment_type_id,
        });

        let equipment_type = equipment.equipment_type.map(|t| {
            json!({
                "equipment_type_id": t.id,
                "name": t.name,
                "code": t.code,
                "category": t.category,
                "requires_driver": t.requires_driver,
                "is_active": t.is_active,
            })
        });

        let mut response = json!({
            "equipment": profile,
            "equipment_type": equipment_type,
        });
        response["evidence"] = evidence(None);
        Ok(response)
    }
}