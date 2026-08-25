//! 测试专用的「未接线仓储」。
//!
//! `DispatchServiceDependencies` 的字段全部必填（漏接依赖是编译错误，不是运行期 500）。
//! 测试通常只关心其中一两个端口，其余需要一个占位实现——就是这个类型。
//!
//! **每个方法都失败，并在错误里点名自己。** 占位实现若返回 `Ok(None)` / `Ok(vec![])` /
//! `Ok(0)`，测试踩到未接线端口时拿到的是「查无此物」而不是失败，于是静默通过——被测代码
//! 走了哪条分支、真正依赖哪些端口，都看不出来。这里全部 `Err(unwired("Trait::method"))`：
//! 测试要用哪个端口，就必须把 `DispatchServiceDependencies` 里那个字段换成真桩件，
//! 依赖关系因此写在测试自己的代码里。
//!
//! 只实现 `stub_dispatch_dependencies` 真正需要的 24 个 trait。

use async_trait::async_trait;

use fms_domain::error::DomainError;
use fms_domain::models::{dispatch, dispatch_collaboration, notification};
use fms_domain::ports::{
    anomaly_repository, dispatch_collaboration_repository, dispatch_repository, flight_repository, todo_repository,
};

use super::unwired;

/// 所有方法都失败并点名端口的仓储桩。测试要用哪个端口，就把
/// `DispatchServiceDependencies` 里那个字段换成真桩件。
pub struct UnwiredRepository;

#[async_trait]
impl dispatch_repository::DepartmentRepository for UnwiredRepository {
    async fn save(&self, _: &dispatch::Department) -> Result<dispatch::Department, DomainError> {
        Err(unwired("DepartmentRepository::save"))
    }
    async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::Department>, DomainError> {
        Err(unwired("DepartmentRepository::find_by_id"))
    }
    async fn find_by_name(&self, _: &str) -> Result<Option<dispatch::Department>, DomainError> {
        Err(unwired("DepartmentRepository::find_by_name"))
    }
    async fn find_all(
        &self,
        _: bool,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::Department>, DomainError> {
        Err(unwired("DepartmentRepository::find_all"))
    }
    async fn has_dependencies(&self, _: &str) -> Result<bool, DomainError> {
        Err(unwired("DepartmentRepository::has_dependencies"))
    }
    async fn delete_permanently(&self, _: &str) -> Result<bool, DomainError> {
        Err(unwired("DepartmentRepository::delete_permanently"))
    }
}

#[async_trait]
impl dispatch_repository::TeamTypeRepository for UnwiredRepository {
    async fn save(&self, _: &dispatch::TeamType) -> Result<dispatch::TeamType, DomainError> {
        Err(unwired("TeamTypeRepository::save"))
    }
    async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::TeamType>, DomainError> {
        Err(unwired("TeamTypeRepository::find_by_id"))
    }
    async fn find_all(
        &self,
        _: bool,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::TeamType>, DomainError> {
        Err(unwired("TeamTypeRepository::find_all"))
    }
    async fn find_by_task_type(&self, _: &str) -> Result<Vec<dispatch::TeamType>, DomainError> {
        Err(unwired("TeamTypeRepository::find_by_task_type"))
    }
    async fn set_active(&self, _: &str, _: bool) -> Result<Option<dispatch::TeamType>, DomainError> {
        Err(unwired("TeamTypeRepository::set_active"))
    }
}

#[async_trait]
impl dispatch_repository::TeamRepository for UnwiredRepository {
    async fn save(&self, _: &dispatch::Team) -> Result<dispatch::Team, DomainError> {
        Err(unwired("TeamRepository::save"))
    }
    async fn find_by_id(&self, _: &str, _: bool) -> Result<Option<dispatch::Team>, DomainError> {
        Err(unwired("TeamRepository::find_by_id"))
    }
    async fn find_by_code(&self, _: &str) -> Result<Option<dispatch::Team>, DomainError> {
        Err(unwired("TeamRepository::find_by_code"))
    }
    async fn find_available_for_dispatch(
        &self,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Vec<dispatch::Team>, DomainError> {
        Err(unwired("TeamRepository::find_available_for_dispatch"))
    }
    async fn find_all(
        &self,
        _: bool,
        _: Option<&str>,
        _: Option<&str>,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::Team>, DomainError> {
        Err(unwired("TeamRepository::find_all"))
    }
    async fn update_position(
        &self,
        _: &str,
        _: f64,
        _: f64,
        _: Option<&str>,
    ) -> Result<bool, DomainError> {
        Err(unwired("TeamRepository::update_position"))
    }
    async fn update_status(&self, _: &str, _: &str) -> Result<bool, DomainError> {
        Err(unwired("TeamRepository::update_status"))
    }
}

#[async_trait]
impl dispatch_repository::TeamMemberRepository for UnwiredRepository {
    async fn save(&self, _: &dispatch::TeamMember) -> Result<dispatch::TeamMember, DomainError> {
        Err(unwired("TeamMemberRepository::save"))
    }
    async fn find_by_team(&self, _: &str, _: bool) -> Result<Vec<dispatch::TeamMember>, DomainError> {
        Err(unwired("TeamMemberRepository::find_by_team"))
    }
    async fn find_by_user(&self, _: &str) -> Result<Vec<dispatch::TeamMember>, DomainError> {
        Err(unwired("TeamMemberRepository::find_by_user"))
    }
    async fn list_active_users(&self) -> Result<Vec<String>, DomainError> {
        Err(unwired("TeamMemberRepository::list_active_users"))
    }
    async fn remove_from_team(&self, _: &str, _: &str) -> Result<bool, DomainError> {
        Err(unwired("TeamMemberRepository::remove_from_team"))
    }
}

#[async_trait]
impl dispatch_repository::EquipmentRepository for UnwiredRepository {
    async fn save(&self, _: &dispatch::Equipment) -> Result<dispatch::Equipment, DomainError> {
        Err(unwired("EquipmentRepository::save"))
    }
    async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::Equipment>, DomainError> {
        Err(unwired("EquipmentRepository::find_by_id"))
    }
    async fn find_by_code(&self, _: &str) -> Result<Option<dispatch::Equipment>, DomainError> {
        Err(unwired("EquipmentRepository::find_by_code"))
    }
    async fn find_available_for_dispatch(
        &self,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Vec<dispatch::Equipment>, DomainError> {
        Err(unwired("EquipmentRepository::find_available_for_dispatch"))
    }
    async fn find_all(
        &self,
        _: bool,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::Equipment>, DomainError> {
        Err(unwired("EquipmentRepository::find_all"))
    }
    async fn update_position(
        &self,
        _: &str,
        _: f64,
        _: f64,
        _: Option<&str>,
    ) -> Result<bool, DomainError> {
        Err(unwired("EquipmentRepository::update_position"))
    }
    async fn update_status(&self, _: &str, _: &str) -> Result<bool, DomainError> {
        Err(unwired("EquipmentRepository::update_status"))
    }
}

#[async_trait]
impl dispatch_repository::StandRepository for UnwiredRepository {
    async fn save(&self, _: &dispatch::Stand) -> Result<dispatch::Stand, DomainError> {
        Err(unwired("StandRepository::save"))
    }
    async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::Stand>, DomainError> {
        Err(unwired("StandRepository::find_by_id"))
    }
    async fn find_by_code(&self, _: &str) -> Result<Option<dispatch::Stand>, DomainError> {
        Err(unwired("StandRepository::find_by_code"))
    }
    async fn find_all(
        &self,
        _: Option<&str>,
        _: bool,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::Stand>, DomainError> {
        Err(unwired("StandRepository::find_all"))
    }
    async fn is_active(&self, id_or_code: &str) -> Result<bool, DomainError> {
        Err(unwired("StandRepository::is_active"))
    }
}

#[async_trait]
impl dispatch_repository::TaskTypeRepository for UnwiredRepository {
    async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::TaskType>, DomainError> {
        Err(unwired("TaskTypeRepository::find_by_id"))
    }
    async fn find_by_code(&self, _: &str) -> Result<Option<dispatch::TaskType>, DomainError> {
        Err(unwired("TaskTypeRepository::find_by_code"))
    }
    async fn find_all(
        &self,
        _: Option<&str>,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::TaskType>, DomainError> {
        Err(unwired("TaskTypeRepository::find_all"))
    }
    async fn save(&self, _: &dispatch::TaskType) -> Result<dispatch::TaskType, DomainError> {
        Err(unwired("TaskTypeRepository::save"))
    }
}

#[async_trait]
impl dispatch_repository::DepartmentQualificationRepository for UnwiredRepository {
    async fn save_catalog(
        &self,
        _: &dispatch::DepartmentQualificationCatalog,
    ) -> Result<dispatch::DepartmentQualificationCatalog, DomainError> {
        Err(unwired("DepartmentQualificationRepository::save_catalog"))
    }
    async fn list_catalogs(
        &self,
        _: &str,
        _: bool,
    ) -> Result<Vec<dispatch::DepartmentQualificationCatalog>, DomainError> {
        Err(unwired("DepartmentQualificationRepository::list_catalogs"))
    }
    async fn save_level(
        &self,
        _: &dispatch::DepartmentQualificationLevel,
    ) -> Result<dispatch::DepartmentQualificationLevel, DomainError> {
        Err(unwired("DepartmentQualificationRepository::save_level"))
    }
    async fn list_levels(
        &self,
        _: &str,
        _: Option<&str>,
        _: bool,
    ) -> Result<Vec<dispatch::DepartmentQualificationLevel>, DomainError> {
        Err(unwired("DepartmentQualificationRepository::list_levels"))
    }
}

#[async_trait]
impl dispatch_repository::QualificationGrantRepository for UnwiredRepository {
    async fn save(
        &self,
        _: &dispatch::QualificationGrant,
    ) -> Result<dispatch::QualificationGrant, DomainError> {
        Err(unwired("QualificationGrantRepository::save"))
    }
    async fn find_by_department(
        &self,
        _: &str,
        _: Option<chrono::DateTime<chrono::Utc>>,
        _: &[String],
        _: bool,
    ) -> Result<Vec<dispatch::QualificationGrant>, DomainError> {
        Err(unwired("QualificationGrantRepository::find_by_department"))
    }
}

#[async_trait]
impl dispatch_repository::DepartmentTaskTypeRequirementRepository for UnwiredRepository {
    async fn next_version_no(&self, _: &str, _: &str) -> Result<i32, DomainError> {
        Err(unwired("DepartmentTaskTypeRequirementRepository::next_version_no"))
    }
    async fn save(
        &self,
        _: &dispatch::DepartmentTaskTypeRequirementVersion,
    ) -> Result<dispatch::DepartmentTaskTypeRequirementVersion, DomainError> {
        Err(unwired("DepartmentTaskTypeRequirementRepository::save"))
    }
    async fn list_versions(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Vec<dispatch::DepartmentTaskTypeRequirementVersion>, DomainError> {
        Err(unwired("DepartmentTaskTypeRequirementRepository::list_versions"))
    }
    async fn find_by_id(
        &self,
        _: &str,
    ) -> Result<Option<dispatch::DepartmentTaskTypeRequirementVersion>, DomainError> {
        Err(unwired("DepartmentTaskTypeRequirementRepository::find_by_id"))
    }
    async fn find_latest_draft(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch::DepartmentTaskTypeRequirementVersion>, DomainError> {
        Err(unwired("DepartmentTaskTypeRequirementRepository::find_latest_draft"))
    }
    async fn find_published(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch::DepartmentTaskTypeRequirementVersion>, DomainError> {
        Err(unwired("DepartmentTaskTypeRequirementRepository::find_published"))
    }
    async fn archive_published(&self, _: &str, _: &str) -> Result<i64, DomainError> {
        Err(unwired("DepartmentTaskTypeRequirementRepository::archive_published"))
    }
}

#[async_trait]
impl dispatch_repository::FlightGenerationRuleRepository for UnwiredRepository {
    async fn next_version_no(&self, _: &str, _: &str, _: &str) -> Result<i32, DomainError> {
        Err(unwired("FlightGenerationRuleRepository::next_version_no"))
    }
    async fn save(
        &self,
        _: &dispatch::FlightGenerationRule,
    ) -> Result<dispatch::FlightGenerationRule, DomainError> {
        Err(unwired("FlightGenerationRuleRepository::save"))
    }
    async fn save_replacing_published(
        &self,
        _: &dispatch::FlightGenerationRule,
        _: &str,
    ) -> Result<dispatch::FlightGenerationRule, DomainError> {
        Err(unwired("FlightGenerationRuleRepository::save_replacing_published"))
    }
    async fn find_by_id(
        &self,
        _: &str,
    ) -> Result<Option<dispatch::FlightGenerationRule>, DomainError> {
        Err(unwired("FlightGenerationRuleRepository::find_by_id"))
    }
    async fn list_rules(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> Result<Vec<dispatch::FlightGenerationRule>, DomainError> {
        Err(unwired("FlightGenerationRuleRepository::list_rules"))
    }
}

#[async_trait]
impl dispatch_repository::GenerationAdjustmentRuleRepository for UnwiredRepository {
    async fn next_version_no(&self, _: &str, _: &str) -> Result<i32, DomainError> {
        Err(unwired("GenerationAdjustmentRuleRepository::next_version_no"))
    }
    async fn save(
        &self,
        _: &dispatch::GenerationAdjustmentRule,
    ) -> Result<dispatch::GenerationAdjustmentRule, DomainError> {
        Err(unwired("GenerationAdjustmentRuleRepository::save"))
    }
    async fn save_replacing_published(
        &self,
        _: &dispatch::GenerationAdjustmentRule,
        _: &str,
    ) -> Result<dispatch::GenerationAdjustmentRule, DomainError> {
        Err(unwired("GenerationAdjustmentRuleRepository::save_replacing_published"))
    }
    async fn find_by_id(
        &self,
        _: &str,
    ) -> Result<Option<dispatch::GenerationAdjustmentRule>, DomainError> {
        Err(unwired("GenerationAdjustmentRuleRepository::find_by_id"))
    }
    async fn list_rules(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> Result<Vec<dispatch::GenerationAdjustmentRule>, DomainError> {
        Err(unwired("GenerationAdjustmentRuleRepository::list_rules"))
    }
}

#[async_trait]
impl dispatch_repository::TemporaryTaskTemplateRepository for UnwiredRepository {
    async fn save(
        &self,
        _: &dispatch::TemporaryTaskTemplate,
    ) -> Result<dispatch::TemporaryTaskTemplate, DomainError> {
        Err(unwired("TemporaryTaskTemplateRepository::save"))
    }
    async fn find_by_code(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch::TemporaryTaskTemplate>, DomainError> {
        Err(unwired("TemporaryTaskTemplateRepository::find_by_code"))
    }
    async fn list_templates(
        &self,
        _: &str,
        _: bool,
    ) -> Result<Vec<dispatch::TemporaryTaskTemplate>, DomainError> {
        Err(unwired("TemporaryTaskTemplateRepository::list_templates"))
    }
}

#[async_trait]
impl dispatch_repository::DispatchOrderRepository for UnwiredRepository {
    async fn save(&self, _: &dispatch::DispatchOrder) -> Result<(), DomainError> {
        Err(unwired("DispatchOrderRepository::save"))
    }
    async fn create_order_atomic(
        &self,
        _: dispatch_repository::CreateDispatchOrderCommand,
    ) -> Result<(), DomainError> {
        Err(unwired("DispatchOrderRepository::create_order_atomic"))
    }
    async fn save_orders_atomic(
        &self,
        _: Vec<dispatch_repository::CreateDispatchOrderCommand>,
    ) -> Result<(), DomainError> {
        Err(unwired("DispatchOrderRepository::save_orders_atomic"))
    }
    async fn find_by_id(
        &self,
        _: &str,
        _: bool,
        _: Option<&str>,
    ) -> Result<Option<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_by_id"))
    }
    async fn find_by_flight(&self, _: &str) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_by_flight"))
    }
    async fn find_by_flight_with_filters(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_by_flight_with_filters"))
    }
    async fn find_by_team(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<chrono::DateTime<chrono::Utc>>,
        _: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_by_team"))
    }
    async fn find_by_team_filtered(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_by_team_filtered"))
    }
    async fn find_by_user(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_by_user"))
    }
    async fn find_all(
        &self,
        _: Option<&str>,
        _: Option<&str>,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_all"))
    }
    async fn find_all_filtered(
        &self,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_all_filtered"))
    }
    async fn find_orders_in_window(
        &self,
        _: chrono::DateTime<chrono::Utc>,
        _: chrono::DateTime<chrono::Utc>,
        _: &[&str],
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: bool,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_orders_in_window"))
    }
    async fn find_overlapping_orders(
        &self,
        _: chrono::DateTime<chrono::Utc>,
        _: chrono::DateTime<chrono::Utc>,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_overlapping_orders"))
    }
    async fn find_equipment_conflicts(
        &self,
        _: &[String],
        _: chrono::DateTime<chrono::Utc>,
        _: chrono::DateTime<chrono::Utc>,
        _: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_equipment_conflicts"))
    }
    async fn list_logs(&self, _: &str, _: i64) -> Result<Vec<serde_json::Value>, DomainError> {
        Err(unwired("DispatchOrderRepository::list_logs"))
    }
    async fn find_pending_for_flight(
        &self,
        _: &str,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_pending_for_flight"))
    }
    async fn find_publishable_orders(
        &self,
        _: chrono::DateTime<chrono::Utc>,
        _: i64,
    ) -> Result<Vec<dispatch::DispatchOrder>, DomainError> {
        Err(unwired("DispatchOrderRepository::find_publishable_orders"))
    }
    async fn update_status(
        &self,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: bool,
    ) -> Result<bool, DomainError> {
        Err(unwired("DispatchOrderRepository::update_status"))
    }
    async fn start_order(
        &self,
        _: &str,
        _: chrono::DateTime<chrono::Utc>,
        _: &str,
    ) -> Result<bool, DomainError> {
        Err(unwired("DispatchOrderRepository::start_order"))
    }
    async fn complete_order(
        &self,
        _: &str,
        _: chrono::DateTime<chrono::Utc>,
        _: &str,
        _: Option<&str>,
    ) -> Result<bool, DomainError> {
        Err(unwired("DispatchOrderRepository::complete_order"))
    }
    async fn append_log(
        &self,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: Option<serde_json::Value>,
    ) -> Result<(), DomainError> {
        Err(unwired("DispatchOrderRepository::append_log"))
    }
    async fn append_log_once(
        &self,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: serde_json::Value,
    ) -> Result<bool, DomainError> {
        Err(unwired("DispatchOrderRepository::append_log_once"))
    }
    async fn has_logged_action(
        &self,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<bool, DomainError> {
        Err(unwired("DispatchOrderRepository::has_logged_action"))
    }
    async fn report_estimated_completion(
        &self,
        _: &str,
        _: chrono::DateTime<chrono::Utc>,
        _: &str,
        _: Option<&str>,
    ) -> Result<bool, DomainError> {
        Err(unwired("DispatchOrderRepository::report_estimated_completion"))
    }
    async fn update_planned_times(
        &self,
        _: &str,
        _: chrono::DateTime<chrono::Utc>,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, DomainError> {
        Err(unwired("DispatchOrderRepository::update_planned_times"))
    }
    async fn replace_order_equipment_assignments(
        &self,
        _: &str,
        _: &[String],
    ) -> Result<(), DomainError> {
        Err(unwired("DispatchOrderRepository::replace_order_equipment_assignments"))
    }
}

#[async_trait]
impl<Tx: Send> dispatch_repository::DispatchOrderTransactionalRepository<Tx> for UnwiredRepository {
    async fn save_in_tx(&self, _: &mut Tx, _: &dispatch::DispatchOrder) -> Result<(), DomainError> {
        Err(unwired("DispatchOrderTransactionalRepository::save_in_tx"))
    }

    async fn append_log_in_tx(
        &self,
        _: &mut Tx,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: Option<serde_json::Value>,
    ) -> Result<(), DomainError> {
        Err(unwired("DispatchOrderTransactionalRepository::append_log_in_tx"))
    }
}

#[async_trait]
impl dispatch_repository::DispatchOrderMemberRepository for UnwiredRepository {
    async fn save(&self, _: &dispatch::DispatchOrderMember) -> Result<(), DomainError> {
        Err(unwired("DispatchOrderMemberRepository::save"))
    }
    async fn find_by_order(
        &self,
        _: &str,
    ) -> Result<Vec<dispatch::DispatchOrderMember>, DomainError> {
        Err(unwired("DispatchOrderMemberRepository::find_by_order"))
    }
    async fn find_by_order_and_user(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch::DispatchOrderMember>, DomainError> {
        Err(unwired("DispatchOrderMemberRepository::find_by_order_and_user"))
    }
    async fn find_latest_checkout_for_user(
        &self,
        _: &str,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<serde_json::Value>, DomainError> {
        Err(unwired("DispatchOrderMemberRepository::find_latest_checkout_for_user"))
    }
}

#[async_trait]
impl<Tx: Send> dispatch_repository::DispatchOrderMemberTransactionalRepository<Tx> for UnwiredRepository {
    async fn save_in_tx(
        &self,
        _: &mut Tx,
        _: &dispatch::DispatchOrderMember,
    ) -> Result<(), DomainError> {
        Err(unwired("DispatchOrderMemberTransactionalRepository::save_in_tx"))
    }
}

#[async_trait]
impl dispatch_repository::DispatchTravelStatsRepository for UnwiredRepository {
    async fn record_travel(&self, _: &str, _: &str, _: f64) -> Result<(), DomainError> {
        Err(unwired("DispatchTravelStatsRepository::record_travel"))
    }
    async fn get_average_travel(&self, _: &str, _: &str) -> Result<Option<f64>, DomainError> {
        Err(unwired("DispatchTravelStatsRepository::get_average_travel"))
    }
}

#[async_trait]
impl dispatch_collaboration_repository::DispatchCollaborationRepository for UnwiredRepository {
    async fn get_group_by_id(
        &self,
        _: &str,
    ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::get_group_by_id"))
    }
    async fn get_group_for_user(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::get_group_for_user"))
    }
    async fn get_group_for_user_by_flight(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::get_group_for_user_by_flight"))
    }
    async fn get_group_by_flight(
        &self,
        _: &str,
    ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::get_group_by_flight"))
    }
    async fn list_user_groups(
        &self,
        _: &str,
        _: &str,
        _: i64,
        _: i64,
    ) -> Result<dispatch_collaboration::DispatchChatGroupList, DomainError> {
        Err(unwired("DispatchCollaborationRepository::list_user_groups"))
    }
    async fn list_group_messages(
        &self,
        _: &str,
        _: i64,
        _: dispatch_collaboration::DispatchChatMessageCursor,
    ) -> Result<dispatch_collaboration::DispatchChatMessageList, DomainError> {
        Err(unwired("DispatchCollaborationRepository::list_group_messages"))
    }
    async fn insert_message(
        &self,
        _: &dispatch_collaboration::NewDispatchChatMessage,
    ) -> Result<dispatch_collaboration::DispatchChatMessage, DomainError> {
        Err(unwired("DispatchCollaborationRepository::insert_message"))
    }
    async fn find_message_by_client_id(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch_collaboration::DispatchChatMessage>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::find_message_by_client_id"))
    }
    async fn update_message_event_id(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch_collaboration::DispatchChatMessage>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::update_message_event_id"))
    }
    async fn mark_group_read(
        &self,
        _: &str,
        _: &str,
        _: i64,
    ) -> Result<Option<dispatch_collaboration::DispatchChatReadCursorUpdate>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::mark_group_read"))
    }
    async fn get_group_latest_seq(&self, _: &str) -> Result<i64, DomainError> {
        Err(unwired("DispatchCollaborationRepository::get_group_latest_seq"))
    }
    async fn count_group_unread(&self, _: &str, _: &str) -> Result<i64, DomainError> {
        Err(unwired("DispatchCollaborationRepository::count_group_unread"))
    }
    async fn count_total_unread(&self, _: &str) -> Result<i64, DomainError> {
        Err(unwired("DispatchCollaborationRepository::count_total_unread"))
    }
    async fn count_unread_for_group_members(
        &self,
        _: &str,
    ) -> Result<Vec<dispatch_collaboration::DispatchChatMemberUnread>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::count_unread_for_group_members"))
    }
    async fn find_active_members(
        &self,
        _: &str,
    ) -> Result<Vec<dispatch_collaboration::DispatchChatMember>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::find_active_members"))
    }
    async fn find_group_members(
        &self,
        _: &str,
    ) -> Result<Vec<dispatch_collaboration::DispatchChatMember>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::find_group_members"))
    }
    async fn find_users_by_ids(
        &self,
        _: &[String],
    ) -> Result<Vec<dispatch_collaboration::DispatchChatUserProfile>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::find_users_by_ids"))
    }
    async fn find_dispatchers_by_departments(
        &self,
        _: &[String],
    ) -> Result<Vec<dispatch_collaboration::DispatchChatDispatcherCandidate>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::find_dispatchers_by_departments"))
    }
    async fn upsert_group_for_flight(
        &self,
        _: &str,
        _: &str,
        _: Option<chrono::DateTime<chrono::Utc>>,
        _: &serde_json::Value,
    ) -> Result<dispatch_collaboration::DispatchChatGroupSummary, DomainError> {
        Err(unwired("DispatchCollaborationRepository::upsert_group_for_flight"))
    }
    async fn upsert_group_memberships(
        &self,
        _: &str,
        _: &[dispatch_collaboration::DispatchChatMemberUpsert],
    ) -> Result<(), DomainError> {
        Err(unwired("DispatchCollaborationRepository::upsert_group_memberships"))
    }
    async fn deactivate_members_except(
        &self,
        _: &str,
        _: &[String],
    ) -> Result<Vec<dispatch_collaboration::DispatchChatMember>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::deactivate_members_except"))
    }
    async fn clear_group_deprecation(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::clear_group_deprecation"))
    }
    async fn mark_group_deprecated(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::mark_group_deprecated"))
    }
    async fn find_groups_pending_deprecation(
        &self,
        _: i64,
    ) -> Result<Vec<dispatch_collaboration::DispatchChatGroupSummary>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::find_groups_pending_deprecation"))
    }
    async fn find_due_archive_groups(
        &self,
        _: i64,
    ) -> Result<Vec<dispatch_collaboration::DispatchChatGroupSummary>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::find_due_archive_groups"))
    }
    async fn archive_groups_batch(
        &self,
        _: &[String],
    ) -> Result<Vec<dispatch_collaboration::DispatchChatGroupSummary>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::archive_groups_batch"))
    }
    async fn create_event(
        &self,
        _: &dispatch_collaboration::DispatchCollaborationEvent,
    ) -> Result<dispatch_collaboration::DispatchCollaborationEvent, DomainError> {
        Err(unwired("DispatchCollaborationRepository::create_event"))
    }
    async fn list_events_by_flight(
        &self,
        _: &str,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch_collaboration::DispatchCollaborationEvent>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::list_events_by_flight"))
    }
    async fn list_events_by_order(
        &self,
        _: &str,
        _: i64,
        _: i64,
    ) -> Result<Vec<dispatch_collaboration::DispatchCollaborationEvent>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::list_events_by_order"))
    }
    async fn find_recent_notifications_by_flight(
        &self,
        _: &str,
        _: i64,
    ) -> Result<Vec<notification::Notification>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::find_recent_notifications_by_flight"))
    }
    async fn find_recent_notifications_by_order(
        &self,
        _: &str,
        _: i64,
    ) -> Result<Vec<notification::Notification>, DomainError> {
        Err(unwired("DispatchCollaborationRepository::find_recent_notifications_by_order"))
    }
    async fn summarize_receipts_for_flight(
        &self,
        _: &str,
    ) -> Result<dispatch_collaboration::NotificationReceiptSummary, DomainError> {
        Err(unwired("DispatchCollaborationRepository::summarize_receipts_for_flight"))
    }
    async fn summarize_receipts_for_order(
        &self,
        _: &str,
    ) -> Result<dispatch_collaboration::NotificationReceiptSummary, DomainError> {
        Err(unwired("DispatchCollaborationRepository::summarize_receipts_for_order"))
    }
}

#[async_trait]
impl anomaly_repository::AnomalyRepository for UnwiredRepository {
    async fn find_by_id(
        &self,
        _: &str,
    ) -> Result<Option<fms_domain::models::anomaly::Anomaly>, DomainError> {
        Err(unwired("AnomalyRepository::find_by_id"))
    }
    async fn find_by_flight(
        &self,
        _: &str,
    ) -> Result<Vec<fms_domain::models::anomaly::Anomaly>, DomainError> {
        Err(unwired("AnomalyRepository::find_by_flight"))
    }
    async fn find_by_status(
        &self,
        _: fms_domain::models::anomaly::AnomalyStatus,
    ) -> Result<Vec<fms_domain::models::anomaly::Anomaly>, DomainError> {
        Err(unwired("AnomalyRepository::find_by_status"))
    }
    async fn list_rules(
        &self,
        _: bool,
    ) -> Result<Vec<fms_domain::models::anomaly::AnomalyRule>, DomainError> {
        Err(unwired("AnomalyRepository::list_rules"))
    }
    async fn get_rule(
        &self,
        _: &str,
    ) -> Result<Option<fms_domain::models::anomaly::AnomalyRule>, DomainError> {
        Err(unwired("AnomalyRepository::get_rule"))
    }
    async fn upsert_rule(
        &self,
        _: &fms_domain::models::anomaly::AnomalyRule,
    ) -> Result<fms_domain::models::anomaly::AnomalyRule, DomainError> {
        Err(unwired("AnomalyRepository::upsert_rule"))
    }
    async fn save(&self, _: &fms_domain::models::anomaly::Anomaly) -> Result<(), DomainError> {
        Err(unwired("AnomalyRepository::save"))
    }
    async fn update(&self, _: &fms_domain::models::anomaly::Anomaly) -> Result<bool, DomainError> {
        Err(unwired("AnomalyRepository::update"))
    }
    async fn acknowledge(&self, _: &str) -> Result<bool, DomainError> {
        Err(unwired("AnomalyRepository::acknowledge"))
    }
    async fn resolve(&self, _: &str) -> Result<bool, DomainError> {
        Err(unwired("AnomalyRepository::resolve"))
    }
    async fn escalate(&self, _: &str) -> Result<bool, DomainError> {
        Err(unwired("AnomalyRepository::escalate"))
    }
}

#[async_trait]
impl dispatch_repository::DispatchAlertRepository for UnwiredRepository {
    async fn save(&self, _: &dispatch::DispatchAlert) -> Result<(), DomainError> {
        Err(unwired("DispatchAlertRepository::save"))
    }
    async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::DispatchAlert>, DomainError> {
        Err(unwired("DispatchAlertRepository::find_by_id"))
    }
    async fn find_unresolved(
        &self,
        _: Option<&str>,
    ) -> Result<Vec<dispatch::DispatchAlert>, DomainError> {
        Err(unwired("DispatchAlertRepository::find_unresolved"))
    }
    async fn resolve(&self, _: &str, _: &str, _: Option<&str>) -> Result<bool, DomainError> {
        Err(unwired("DispatchAlertRepository::resolve"))
    }
    async fn upsert_overrun(
        &self,
        _: &dispatch::DispatchAlert,
    ) -> Result<dispatch_repository::OverrunAlertUpsert, DomainError> {
        Err(unwired("DispatchAlertRepository::upsert_overrun"))
    }
    async fn acknowledge(&self, _: &str, _: &str) -> Result<bool, DomainError> {
        Err(unwired("DispatchAlertRepository::acknowledge"))
    }
    async fn auto_resolve(&self, _: &str) -> Result<bool, DomainError> {
        Err(unwired("DispatchAlertRepository::auto_resolve"))
    }
}

#[async_trait]
impl flight_repository::FlightRepository for UnwiredRepository {
    async fn find_by_id(
        &self,
        _: &str,
    ) -> Result<Option<fms_domain::models::flight::Flight>, DomainError> {
        Err(unwired("FlightRepository::find_by_id"))
    }
    async fn find_all(
        &self,
        _: i64,
        _: i64,
    ) -> Result<Vec<fms_domain::models::flight::Flight>, DomainError> {
        Err(unwired("FlightRepository::find_all"))
    }
    async fn find_by_date(
        &self,
        _: chrono::NaiveDate,
    ) -> Result<Vec<fms_domain::models::flight::Flight>, DomainError> {
        Err(unwired("FlightRepository::find_by_date"))
    }
    async fn find_by_flight_number(
        &self,
        _: &str,
    ) -> Result<Vec<fms_domain::models::flight::Flight>, DomainError> {
        Err(unwired("FlightRepository::find_by_flight_number"))
    }
    async fn find_by_status(
        &self,
        _: i32,
        _: i64,
        _: i64,
    ) -> Result<Vec<fms_domain::models::flight::Flight>, DomainError> {
        Err(unwired("FlightRepository::find_by_status"))
    }
    async fn save(&self, _: &fms_domain::models::flight::Flight) -> Result<(), DomainError> {
        Err(unwired("FlightRepository::save"))
    }
    async fn update_partial(
        &self,
        _: &str,
        _: &flight_repository::FlightUpdatePatch,
    ) -> Result<Option<fms_domain::models::flight::Flight>, DomainError> {
        Err(unwired("FlightRepository::update_partial"))
    }
    async fn save_batch(&self, _: &[fms_domain::models::flight::Flight]) -> Result<usize, DomainError> {
        Err(unwired("FlightRepository::save_batch"))
    }
    async fn update_status(&self, _: &str, _: i32) -> Result<bool, DomainError> {
        Err(unwired("FlightRepository::update_status"))
    }
    async fn delete(&self, _: &str) -> Result<bool, DomainError> {
        Err(unwired("FlightRepository::delete"))
    }
    async fn search(
        &self,
        _: &flight_repository::FlightSearchCriteria,
        _: i64,
        _: i64,
    ) -> Result<Vec<fms_domain::models::flight::Flight>, DomainError> {
        Err(unwired("FlightRepository::search"))
    }
    async fn count_by_date(&self, _: chrono::NaiveDate) -> Result<i64, DomainError> {
        Err(unwired("FlightRepository::count_by_date"))
    }
}

#[async_trait]
impl todo_repository::TodoRepository for UnwiredRepository {
    async fn find_by_id(&self, _: &str) -> Result<Option<fms_domain::models::todo::Todo>, DomainError> {
        Err(unwired("TodoRepository::find_by_id"))
    }
    async fn soft_delete_by_source_ids(
        &self,
        _: &str,
        _: &[String],
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, DomainError> {
        Err(unwired("TodoRepository::soft_delete_by_source_ids"))
    }
    async fn find_all(
        &self,
        _: Option<fms_domain::models::todo::TodoStatus>,
        _: Option<fms_domain::models::todo::TodoPriority>,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: i64,
        _: i64,
    ) -> Result<Vec<fms_domain::models::todo::Todo>, DomainError> {
        Err(unwired("TodoRepository::find_all"))
    }
    async fn find_by_ids(&self, _: &[String]) -> Result<Vec<fms_domain::models::todo::Todo>, DomainError> {
        Err(unwired("TodoRepository::find_by_ids"))
    }
    async fn find_by_source(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Vec<fms_domain::models::todo::Todo>, DomainError> {
        Err(unwired("TodoRepository::find_by_source"))
    }
    async fn find_overdue(&self) -> Result<Vec<fms_domain::models::todo::Todo>, DomainError> {
        Err(unwired("TodoRepository::find_overdue"))
    }
    async fn find_children(&self, _: &str) -> Result<Vec<fms_domain::models::todo::Todo>, DomainError> {
        Err(unwired("TodoRepository::find_children"))
    }
    async fn save(&self, _: &fms_domain::models::todo::Todo) -> Result<(), DomainError> {
        Err(unwired("TodoRepository::save"))
    }
    async fn update(&self, _: &fms_domain::models::todo::Todo) -> Result<bool, DomainError> {
        Err(unwired("TodoRepository::update"))
    }
    async fn soft_delete(&self, _: &str, _: &str) -> Result<bool, DomainError> {
        Err(unwired("TodoRepository::soft_delete"))
    }
    async fn count_by_status(&self, _: fms_domain::models::todo::TodoStatus) -> Result<i64, DomainError> {
        Err(unwired("TodoRepository::count_by_status"))
    }
    async fn count_by_source_ids(
        &self,
        _: &str,
        _: &[String],
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<i64, DomainError> {
        Err(unwired("TodoRepository::count_by_source_ids"))
    }
}

#[async_trait]
impl dispatch_repository::DispatchChecklistRepository for UnwiredRepository {
    async fn get_template(&self, _: &str) -> Result<Option<serde_json::Value>, DomainError> {
        Err(unwired("DispatchChecklistRepository::get_template"))
    }
    async fn upsert_template(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &[serde_json::Value],
        _: bool,
        _: Option<&str>,
    ) -> Result<serde_json::Value, DomainError> {
        Err(unwired("DispatchChecklistRepository::upsert_template"))
    }
    async fn list_records(&self, _: &str) -> Result<Vec<serde_json::Value>, DomainError> {
        Err(unwired("DispatchChecklistRepository::list_records"))
    }
    async fn submit_item_result(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
        _: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Err(unwired("DispatchChecklistRepository::submit_item_result"))
    }
    async fn evaluate_completion_gate(&self, _: &str, _: &str) -> Result<bool, DomainError> {
        Err(unwired("DispatchChecklistRepository::evaluate_completion_gate"))
    }
}
