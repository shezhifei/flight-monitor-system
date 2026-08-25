//! 仓储接口 (Ports)
//!
//! 定义 domain 层需要的数据访问抽象，由 infrastructure 层实现。

pub mod ai_auth_context_loader;
pub mod ai_context_snapshot_repository;
pub mod ai_copilot_repository;
pub mod ai_entity_config_repository;
pub mod ai_execution_repository;
pub mod ai_job_repository;
pub mod ai_object_policy_repository;
pub mod ai_ontology_repository;
pub mod ai_proposal_repository;
pub mod ai_run_event_repository;
pub mod ai_run_repository;
pub mod anomaly_repository;
pub mod audit_log_repository;
pub mod business_case_repository;
pub mod business_case_workflow_run_repository;
pub mod database_metadata_port;
pub mod dispatch_collaboration_repository;
pub mod dispatch_repository;
pub mod domain_event_outbox_repository;
pub mod domain_event_subscription_state_repository;
pub mod event_rule_repository;
pub mod flight_archive_repository;
pub mod flight_cache_backend;
pub mod flight_repository;
pub mod flight_runtime_projection_repository;
pub mod flight_sync_repository;
pub mod flight_timeline_event_repository;
pub use flight_sync_repository::FlightSyncRepository;
pub mod flowable_gateway;
pub mod kpi_port;
pub mod label_repository;
pub mod message_queue;
pub mod mobile_repository;
pub mod nonce_replay_store;
pub mod notification_repository;
pub mod online_history_repository;
pub mod ontology_repository;
pub mod operator_identity_repository;
pub mod permission_template_repository;
pub mod session_runtime_repository;
pub mod shift_handover_repository;
pub mod system_flags_repository;
pub mod todo_agent_context_repository;
pub mod todo_repository;
pub mod user_repository;
pub mod workflow_dispatch_repository;
pub mod workflow_form_repository;

/// Null object implementations for all dispatch repository traits.
/// Used as default generic type parameters for optional dependencies.
pub struct NullRepository;

mod null_repository_impls {
    use super::anomaly_repository;
    use super::dispatch_collaboration_repository;
    use super::dispatch_repository;
    use super::flight_repository;
    use super::todo_repository;
    use crate::models::dispatch;
    use crate::models::dispatch_collaboration;
    use crate::models::notification;
    use async_trait::async_trait;

    #[async_trait]
    impl dispatch_repository::DepartmentRepository for super::NullRepository {
        async fn save(&self, _: &dispatch::Department) -> Result<dispatch::Department, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::Department>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_name(&self, _: &str) -> Result<Option<dispatch::Department>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_all(
            &self,
            _: bool,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::Department>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn has_dependencies(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn delete_permanently(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl dispatch_repository::TeamTypeRepository for super::NullRepository {
        async fn save(&self, _: &dispatch::TeamType) -> Result<dispatch::TeamType, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::TeamType>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_all(
            &self,
            _: bool,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::TeamType>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_task_type(&self, _: &str) -> Result<Vec<dispatch::TeamType>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn set_active(&self, _: &str, _: bool) -> Result<Option<dispatch::TeamType>, crate::error::DomainError> {
            Ok(None)
        }
    }

    #[async_trait]
    impl dispatch_repository::TeamRepository for super::NullRepository {
        async fn save(&self, _: &dispatch::Team) -> Result<dispatch::Team, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_id(&self, _: &str, _: bool) -> Result<Option<dispatch::Team>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_code(&self, _: &str) -> Result<Option<dispatch::Team>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_available_for_dispatch(
            &self,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<Vec<dispatch::Team>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_all(
            &self,
            _: bool,
            _: Option<&str>,
            _: Option<&str>,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::Team>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn update_position(
            &self,
            _: &str,
            _: f64,
            _: f64,
            _: Option<&str>,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn update_status(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl dispatch_repository::TeamMemberRepository for super::NullRepository {
        async fn save(&self, _: &dispatch::TeamMember) -> Result<dispatch::TeamMember, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_team(&self, _: &str, _: bool) -> Result<Vec<dispatch::TeamMember>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_user(&self, _: &str) -> Result<Vec<dispatch::TeamMember>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn list_active_users(&self) -> Result<Vec<String>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn remove_from_team(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl dispatch_repository::EquipmentTypeRepository for super::NullRepository {
        async fn save(
            &self,
            _: &dispatch::EquipmentType,
        ) -> Result<dispatch::EquipmentType, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::EquipmentType>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_all(
            &self,
            _: bool,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::EquipmentType>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn set_active(
            &self,
            _: &str,
            _: bool,
        ) -> Result<Option<dispatch::EquipmentType>, crate::error::DomainError> {
            Ok(None)
        }
    }

    #[async_trait]
    impl dispatch_repository::EquipmentRepository for super::NullRepository {
        async fn save(&self, _: &dispatch::Equipment) -> Result<dispatch::Equipment, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::Equipment>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_code(&self, _: &str) -> Result<Option<dispatch::Equipment>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_available_for_dispatch(
            &self,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<Vec<dispatch::Equipment>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_all(
            &self,
            _: bool,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::Equipment>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn update_position(
            &self,
            _: &str,
            _: f64,
            _: f64,
            _: Option<&str>,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn update_status(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl dispatch_repository::StandRepository for super::NullRepository {
        async fn save(&self, _: &dispatch::Stand) -> Result<dispatch::Stand, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::Stand>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_code(&self, _: &str) -> Result<Option<dispatch::Stand>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_all(
            &self,
            _: Option<&str>,
            _: bool,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::Stand>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn is_active(&self, id_or_code: &str) -> Result<bool, crate::error::DomainError> {
            Err(crate::error::DomainError::NotFound {
                entity_type: "stand",
                id: id_or_code.to_string(),
            })
        }
    }

    #[async_trait]
    impl dispatch_repository::TaskTypeRepository for super::NullRepository {
        async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::TaskType>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_code(&self, _: &str) -> Result<Option<dispatch::TaskType>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_all(
            &self,
            _: Option<&str>,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::TaskType>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn save(&self, _: &dispatch::TaskType) -> Result<dispatch::TaskType, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
    }

    #[async_trait]
    impl dispatch_repository::DepartmentQualificationRepository for super::NullRepository {
        async fn save_catalog(
            &self,
            _: &dispatch::DepartmentQualificationCatalog,
        ) -> Result<dispatch::DepartmentQualificationCatalog, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn list_catalogs(
            &self,
            _: &str,
            _: bool,
        ) -> Result<Vec<dispatch::DepartmentQualificationCatalog>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn save_level(
            &self,
            _: &dispatch::DepartmentQualificationLevel,
        ) -> Result<dispatch::DepartmentQualificationLevel, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn list_levels(
            &self,
            _: &str,
            _: Option<&str>,
            _: bool,
        ) -> Result<Vec<dispatch::DepartmentQualificationLevel>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl dispatch_repository::QualificationGrantRepository for super::NullRepository {
        async fn save(
            &self,
            _: &dispatch::QualificationGrant,
        ) -> Result<dispatch::QualificationGrant, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_department(
            &self,
            _: &str,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: &[String],
            _: bool,
        ) -> Result<Vec<dispatch::QualificationGrant>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl dispatch_repository::DepartmentTaskTypeRequirementRepository for super::NullRepository {
        async fn next_version_no(&self, _: &str, _: &str) -> Result<i32, crate::error::DomainError> {
            Ok(1)
        }
        async fn save(
            &self,
            _: &dispatch::DepartmentTaskTypeRequirementVersion,
        ) -> Result<dispatch::DepartmentTaskTypeRequirementVersion, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn list_versions(
            &self,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<Vec<dispatch::DepartmentTaskTypeRequirementVersion>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_id(
            &self,
            _: &str,
        ) -> Result<Option<dispatch::DepartmentTaskTypeRequirementVersion>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_latest_draft(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch::DepartmentTaskTypeRequirementVersion>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_published(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch::DepartmentTaskTypeRequirementVersion>, crate::error::DomainError> {
            Ok(None)
        }
        async fn archive_published(&self, _: &str, _: &str) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
    }

    #[async_trait]
    impl dispatch_repository::FlightGenerationRuleRepository for super::NullRepository {
        async fn next_version_no(&self, _: &str, _: &str, _: &str) -> Result<i32, crate::error::DomainError> {
            Ok(1)
        }
        async fn save(
            &self,
            _: &dispatch::FlightGenerationRule,
        ) -> Result<dispatch::FlightGenerationRule, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn save_replacing_published(
            &self,
            _: &dispatch::FlightGenerationRule,
            _: &str,
        ) -> Result<dispatch::FlightGenerationRule, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_id(
            &self,
            _: &str,
        ) -> Result<Option<dispatch::FlightGenerationRule>, crate::error::DomainError> {
            Ok(None)
        }
        async fn list_rules(
            &self,
            _: &str,
            _: Option<&str>,
        ) -> Result<Vec<dispatch::FlightGenerationRule>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl dispatch_repository::GenerationAdjustmentRuleRepository for super::NullRepository {
        async fn next_version_no(&self, _: &str, _: &str) -> Result<i32, crate::error::DomainError> {
            Ok(1)
        }
        async fn save(
            &self,
            _: &dispatch::GenerationAdjustmentRule,
        ) -> Result<dispatch::GenerationAdjustmentRule, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn save_replacing_published(
            &self,
            _: &dispatch::GenerationAdjustmentRule,
            _: &str,
        ) -> Result<dispatch::GenerationAdjustmentRule, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_id(
            &self,
            _: &str,
        ) -> Result<Option<dispatch::GenerationAdjustmentRule>, crate::error::DomainError> {
            Ok(None)
        }
        async fn list_rules(
            &self,
            _: &str,
            _: Option<&str>,
        ) -> Result<Vec<dispatch::GenerationAdjustmentRule>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl dispatch_repository::TemporaryTaskTemplateRepository for super::NullRepository {
        async fn save(
            &self,
            _: &dispatch::TemporaryTaskTemplate,
        ) -> Result<dispatch::TemporaryTaskTemplate, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_by_code(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch::TemporaryTaskTemplate>, crate::error::DomainError> {
            Ok(None)
        }
        async fn list_templates(
            &self,
            _: &str,
            _: bool,
        ) -> Result<Vec<dispatch::TemporaryTaskTemplate>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl dispatch_repository::ShiftTemplateRepository for super::NullRepository {
        async fn save(
            &self,
            _: &dispatch::ShiftTemplate,
        ) -> Result<dispatch::ShiftTemplate, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_all(
            &self,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<bool>,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::ShiftTemplate>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl dispatch_repository::ShiftInstanceRepository for super::NullRepository {
        async fn save(
            &self,
            _: &dispatch::ShiftInstance,
        ) -> Result<dispatch::ShiftInstance, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_all(
            &self,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::ShiftInstance>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_for_resource_window(
            &self,
            _: &str,
            _: &str,
            _: chrono::DateTime<chrono::Utc>,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<dispatch::ShiftInstance>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl dispatch_repository::ScheduleExceptionRepository for super::NullRepository {
        async fn save_leave_record(
            &self,
            _: &dispatch::LeaveRecord,
        ) -> Result<dispatch::LeaveRecord, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_leave_records(
            &self,
            _: &[String],
            _: Option<&str>,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<Vec<dispatch::LeaveRecord>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn save_equipment_downtime(
            &self,
            _: &dispatch::EquipmentDowntime,
        ) -> Result<dispatch::EquipmentDowntime, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_equipment_downtimes(
            &self,
            _: &[String],
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<Vec<dispatch::EquipmentDowntime>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn save_lock_rule(
            &self,
            _: &dispatch::DispatchLockRule,
        ) -> Result<dispatch::DispatchLockRule, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_lock_rules(
            &self,
            _: &[String],
            _: Option<&str>,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<Vec<dispatch::DispatchLockRule>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn list_exceptions(
            &self,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: i64,
        ) -> Result<Vec<serde_json::Value>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl dispatch_repository::DispatchOrderRepository for super::NullRepository {
        async fn save(&self, _: &dispatch::DispatchOrder) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn create_order_atomic(
            &self,
            _: dispatch_repository::CreateDispatchOrderCommand,
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn save_orders_atomic(
            &self,
            _: Vec<dispatch_repository::CreateDispatchOrderCommand>,
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn find_by_id(
            &self,
            _: &str,
            _: bool,
            _: Option<&str>,
        ) -> Result<Option<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_flight(&self, _: &str) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_flight_with_filters(
            &self,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_team(
            &self,
            _: &str,
            _: Option<&str>,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_team_filtered(
            &self,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_user(
            &self,
            _: &str,
            _: Option<&str>,
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_all(
            &self,
            _: Option<&str>,
            _: Option<&str>,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_all_filtered(
            &self,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
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
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_overlapping_orders(
            &self,
            _: chrono::DateTime<chrono::Utc>,
            _: chrono::DateTime<chrono::Utc>,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_equipment_conflicts(
            &self,
            _: &[String],
            _: chrono::DateTime<chrono::Utc>,
            _: chrono::DateTime<chrono::Utc>,
            _: Option<&str>,
        ) -> Result<Vec<serde_json::Value>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn list_logs(&self, _: &str, _: i64) -> Result<Vec<serde_json::Value>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_pending_for_flight(
            &self,
            _: &str,
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_publishable_orders(
            &self,
            _: chrono::DateTime<chrono::Utc>,
            _: i64,
        ) -> Result<Vec<dispatch::DispatchOrder>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn update_status(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: bool,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn start_order(
            &self,
            _: &str,
            _: chrono::DateTime<chrono::Utc>,
            _: &str,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn complete_order(
            &self,
            _: &str,
            _: chrono::DateTime<chrono::Utc>,
            _: &str,
            _: Option<&str>,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn append_log(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<serde_json::Value>,
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn append_log_once(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: serde_json::Value,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn has_logged_action(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn report_estimated_completion(
            &self,
            _: &str,
            _: chrono::DateTime<chrono::Utc>,
            _: &str,
            _: Option<&str>,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn update_planned_times(
            &self,
            _: &str,
            _: chrono::DateTime<chrono::Utc>,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn replace_order_equipment_assignments(
            &self,
            _: &str,
            _: &[String],
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
    }

    #[async_trait]
    impl<Tx: Send> dispatch_repository::DispatchOrderTransactionalRepository<Tx> for super::NullRepository {
        async fn save_in_tx(&self, _: &mut Tx, _: &dispatch::DispatchOrder) -> Result<(), crate::error::DomainError> {
            Ok(())
        }

        async fn append_log_in_tx(
            &self,
            _: &mut Tx,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<serde_json::Value>,
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
    }

    #[async_trait]
    impl dispatch_repository::DispatchOrderMemberRepository for super::NullRepository {
        async fn save(&self, _: &dispatch::DispatchOrderMember) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn find_by_order(
            &self,
            _: &str,
        ) -> Result<Vec<dispatch::DispatchOrderMember>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_order_and_user(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch::DispatchOrderMember>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_latest_checkout_for_user(
            &self,
            _: &str,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<Option<serde_json::Value>, crate::error::DomainError> {
            Ok(None)
        }
    }

    #[async_trait]
    impl<Tx: Send> dispatch_repository::DispatchOrderMemberTransactionalRepository<Tx> for super::NullRepository {
        async fn save_in_tx(
            &self,
            _: &mut Tx,
            _: &dispatch::DispatchOrderMember,
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
    }

    #[async_trait]
    impl dispatch_repository::DispatchTravelStatsRepository for super::NullRepository {
        async fn record_travel(&self, _: &str, _: &str, _: f64) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn get_average_travel(&self, _: &str, _: &str) -> Result<Option<f64>, crate::error::DomainError> {
            Ok(None)
        }
    }

    #[async_trait]
    impl dispatch_collaboration_repository::DispatchCollaborationRepository for super::NullRepository {
        async fn get_group_by_id(
            &self,
            _: &str,
        ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, crate::error::DomainError> {
            Ok(None)
        }
        async fn get_group_for_user(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, crate::error::DomainError> {
            Ok(None)
        }
        async fn get_group_for_user_by_flight(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, crate::error::DomainError> {
            Ok(None)
        }
        async fn get_group_by_flight(
            &self,
            _: &str,
        ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, crate::error::DomainError> {
            Ok(None)
        }
        async fn list_user_groups(
            &self,
            _: &str,
            _: &str,
            _: i64,
            _: i64,
        ) -> Result<dispatch_collaboration::DispatchChatGroupList, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn list_group_messages(
            &self,
            _: &str,
            _: i64,
            _: dispatch_collaboration::DispatchChatMessageCursor,
        ) -> Result<dispatch_collaboration::DispatchChatMessageList, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn insert_message(
            &self,
            _: &dispatch_collaboration::NewDispatchChatMessage,
        ) -> Result<dispatch_collaboration::DispatchChatMessage, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_message_by_client_id(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch_collaboration::DispatchChatMessage>, crate::error::DomainError> {
            Ok(None)
        }
        async fn update_message_event_id(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch_collaboration::DispatchChatMessage>, crate::error::DomainError> {
            Ok(None)
        }
        async fn mark_group_read(
            &self,
            _: &str,
            _: &str,
            _: i64,
        ) -> Result<Option<dispatch_collaboration::DispatchChatReadCursorUpdate>, crate::error::DomainError> {
            Ok(None)
        }
        async fn get_group_latest_seq(&self, _: &str) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
        async fn count_group_unread(&self, _: &str, _: &str) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
        async fn count_total_unread(&self, _: &str) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
        async fn count_unread_for_group_members(
            &self,
            _: &str,
        ) -> Result<Vec<dispatch_collaboration::DispatchChatMemberUnread>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_active_members(
            &self,
            _: &str,
        ) -> Result<Vec<dispatch_collaboration::DispatchChatMember>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_group_members(
            &self,
            _: &str,
        ) -> Result<Vec<dispatch_collaboration::DispatchChatMember>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_users_by_ids(
            &self,
            _: &[String],
        ) -> Result<Vec<dispatch_collaboration::DispatchChatUserProfile>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_dispatchers_by_departments(
            &self,
            _: &[String],
        ) -> Result<Vec<dispatch_collaboration::DispatchChatDispatcherCandidate>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn upsert_group_for_flight(
            &self,
            _: &str,
            _: &str,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: &serde_json::Value,
        ) -> Result<dispatch_collaboration::DispatchChatGroupSummary, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn upsert_group_memberships(
            &self,
            _: &str,
            _: &[dispatch_collaboration::DispatchChatMemberUpsert],
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn deactivate_members_except(
            &self,
            _: &str,
            _: &[String],
        ) -> Result<Vec<dispatch_collaboration::DispatchChatMember>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn clear_group_deprecation(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, crate::error::DomainError> {
            Ok(None)
        }
        async fn mark_group_deprecated(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dispatch_collaboration::DispatchChatGroupSummary>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_groups_pending_deprecation(
            &self,
            _: i64,
        ) -> Result<Vec<dispatch_collaboration::DispatchChatGroupSummary>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_due_archive_groups(
            &self,
            _: i64,
        ) -> Result<Vec<dispatch_collaboration::DispatchChatGroupSummary>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn archive_groups_batch(
            &self,
            _: &[String],
        ) -> Result<Vec<dispatch_collaboration::DispatchChatGroupSummary>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn create_event(
            &self,
            _: &dispatch_collaboration::DispatchCollaborationEvent,
        ) -> Result<dispatch_collaboration::DispatchCollaborationEvent, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn list_events_by_flight(
            &self,
            _: &str,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch_collaboration::DispatchCollaborationEvent>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn list_events_by_order(
            &self,
            _: &str,
            _: i64,
            _: i64,
        ) -> Result<Vec<dispatch_collaboration::DispatchCollaborationEvent>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_recent_notifications_by_flight(
            &self,
            _: &str,
            _: i64,
        ) -> Result<Vec<notification::Notification>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_recent_notifications_by_order(
            &self,
            _: &str,
            _: i64,
        ) -> Result<Vec<notification::Notification>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn summarize_receipts_for_flight(
            &self,
            _: &str,
        ) -> Result<dispatch_collaboration::NotificationReceiptSummary, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn summarize_receipts_for_order(
            &self,
            _: &str,
        ) -> Result<dispatch_collaboration::NotificationReceiptSummary, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
    }

    #[async_trait]
    impl anomaly_repository::AnomalyRepository for super::NullRepository {
        async fn find_by_id(
            &self,
            _: &str,
        ) -> Result<Option<crate::models::anomaly::Anomaly>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_flight(
            &self,
            _: &str,
        ) -> Result<Vec<crate::models::anomaly::Anomaly>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_status(
            &self,
            _: crate::models::anomaly::AnomalyStatus,
        ) -> Result<Vec<crate::models::anomaly::Anomaly>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn list_rules(
            &self,
            _: bool,
        ) -> Result<Vec<crate::models::anomaly::AnomalyRule>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn get_rule(
            &self,
            _: &str,
        ) -> Result<Option<crate::models::anomaly::AnomalyRule>, crate::error::DomainError> {
            Ok(None)
        }
        async fn upsert_rule(
            &self,
            _: &crate::models::anomaly::AnomalyRule,
        ) -> Result<crate::models::anomaly::AnomalyRule, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn save(&self, _: &crate::models::anomaly::Anomaly) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn update(&self, _: &crate::models::anomaly::Anomaly) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn acknowledge(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn resolve(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn escalate(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl<Tx: Send> anomaly_repository::AnomalyTransactionalRepository<Tx> for super::NullRepository {
        async fn acknowledge_in_tx(&self, _: &mut Tx, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }

        async fn escalate_in_tx(&self, _: &mut Tx, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }

        async fn resolve_in_tx(&self, _: &mut Tx, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl dispatch_repository::DispatchAlertRepository for super::NullRepository {
        async fn save(&self, _: &dispatch::DispatchAlert) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn find_by_id(&self, _: &str) -> Result<Option<dispatch::DispatchAlert>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_unresolved(
            &self,
            _: Option<&str>,
        ) -> Result<Vec<dispatch::DispatchAlert>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn resolve(&self, _: &str, _: &str, _: Option<&str>) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn upsert_overrun(
            &self,
            _: &dispatch::DispatchAlert,
        ) -> Result<dispatch_repository::OverrunAlertUpsert, crate::error::DomainError> {
            Ok(dispatch_repository::OverrunAlertUpsert {
                alert: dispatch::DispatchAlert {
                    id: String::new(),
                    flight_id: None,
                    task_type: None,
                    alert_type: String::new(),
                    severity: dispatch::AlertSeverity::Warning,
                    message: String::new(),
                    is_resolved: true,
                    resolved_at: None,
                    resolved_by: None,
                    resolution_notes: None,
                    notify_users: Vec::new(),
                    created_at: None,
                    dedupe_key: None,
                    current_order_id: None,
                    next_order_id: None,
                    last_detected_at: None,
                    occurrence_count: 1,
                    acknowledged_at: None,
                    acknowledged_by: None,
                    details: Default::default(),
                },
                inserted: false,
                reopened: false,
            })
        }
        async fn acknowledge(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn auto_resolve(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl flight_repository::FlightRepository for super::NullRepository {
        async fn find_by_id(
            &self,
            _: &str,
        ) -> Result<Option<crate::models::flight::Flight>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_all(
            &self,
            _: i64,
            _: i64,
        ) -> Result<Vec<crate::models::flight::Flight>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_date(
            &self,
            _: chrono::NaiveDate,
        ) -> Result<Vec<crate::models::flight::Flight>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_flight_number(
            &self,
            _: &str,
        ) -> Result<Vec<crate::models::flight::Flight>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_status(
            &self,
            _: i32,
            _: i64,
            _: i64,
        ) -> Result<Vec<crate::models::flight::Flight>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn save(&self, _: &crate::models::flight::Flight) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn update_partial(
            &self,
            _: &str,
            _: &flight_repository::FlightUpdatePatch,
        ) -> Result<Option<crate::models::flight::Flight>, crate::error::DomainError> {
            Ok(None)
        }
        async fn save_batch(&self, _: &[crate::models::flight::Flight]) -> Result<usize, crate::error::DomainError> {
            Ok(0)
        }
        async fn update_status(&self, _: &str, _: i32) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn delete(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn search(
            &self,
            _: &flight_repository::FlightSearchCriteria,
            _: i64,
            _: i64,
        ) -> Result<Vec<crate::models::flight::Flight>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn count_by_date(&self, _: chrono::NaiveDate) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
    }

    #[async_trait]
    impl<Tx: Send> flight_repository::FlightTransactionalRepository<Tx> for super::NullRepository {
        async fn save_in_tx(
            &self,
            _: &mut Tx,
            _: &crate::models::flight::Flight,
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn update_partial_in_tx(
            &self,
            _: &mut Tx,
            _: &str,
            _: &flight_repository::FlightUpdatePatch,
        ) -> Result<Option<crate::models::flight::Flight>, crate::error::DomainError> {
            Ok(None)
        }
        async fn delete_in_tx(&self, _: &mut Tx, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl todo_repository::TodoRepository for super::NullRepository {
        async fn find_by_id(&self, _: &str) -> Result<Option<crate::models::todo::Todo>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_all(
            &self,
            _: Option<crate::models::todo::TodoStatus>,
            _: Option<crate::models::todo::TodoPriority>,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: i64,
            _: i64,
        ) -> Result<Vec<crate::models::todo::Todo>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_ids(&self, _: &[String]) -> Result<Vec<crate::models::todo::Todo>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_source(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Vec<crate::models::todo::Todo>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_overdue(&self) -> Result<Vec<crate::models::todo::Todo>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_children(&self, _: &str) -> Result<Vec<crate::models::todo::Todo>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn save(&self, _: &crate::models::todo::Todo) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn update(&self, _: &crate::models::todo::Todo) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn soft_delete(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn count_by_status(&self, _: crate::models::todo::TodoStatus) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
        async fn count_by_source_ids(
            &self,
            _: &str,
            _: &[String],
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
    }

    #[async_trait]
    impl<Tx: Send> todo_repository::TodoTransactionalRepository<Tx> for super::NullRepository {
        async fn save_in_tx(&self, _: &mut Tx, _: &crate::models::todo::Todo) -> Result<(), crate::error::DomainError> {
            Ok(())
        }

        async fn update_in_tx(
            &self,
            _: &mut Tx,
            _: &crate::models::todo::Todo,
        ) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }

        async fn soft_delete_by_source_ids(
            &self,
            _: &mut Tx,
            _: &str,
            _: &[String],
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<u64, crate::error::DomainError> {
            Ok(0)
        }
    }

    #[async_trait]
    impl dispatch_repository::DispatchChecklistRepository for super::NullRepository {
        async fn get_template(&self, _: &str) -> Result<Option<serde_json::Value>, crate::error::DomainError> {
            Ok(None)
        }
        async fn upsert_template(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: &[serde_json::Value],
            _: bool,
            _: Option<&str>,
        ) -> Result<serde_json::Value, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn list_records(&self, _: &str) -> Result<Vec<serde_json::Value>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn submit_item_result(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
            _: &str,
        ) -> Result<serde_json::Value, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn evaluate_completion_gate(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl super::notification_repository::NotificationRepository for super::NullRepository {
        async fn save(&self, _: &crate::models::notification::Notification) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn find_by_id(
            &self,
            _: &str,
        ) -> Result<Option<crate::models::notification::Notification>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_id_for_user(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<crate::models::notification::Notification>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_user(
            &self,
            _: &str,
            _: bool,
            _: i64,
            _: i64,
        ) -> Result<Vec<crate::models::notification::Notification>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn mark_read(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn mark_delivered(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn mark_all_read(&self, _: &str) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
        async fn count_unread(&self, _: &str) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
        async fn acknowledge(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: Option<&str>,
        ) -> Result<Option<crate::models::notification::Notification>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_receipt_group(
            &self,
            _: &str,
        ) -> Result<Vec<crate::models::notification::Notification>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn summarize_receipt_group(
            &self,
            _: &str,
        ) -> Result<Option<serde_json::Value>, crate::error::DomainError> {
            Ok(None)
        }
        async fn list_sent_receipt_groups(
            &self,
            _: &str,
            _: i64,
            _: i64,
        ) -> Result<Vec<serde_json::Value>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl<Tx: Send> super::notification_repository::NotificationTransactionalRepository<Tx> for super::NullRepository {
        async fn save_in_tx(
            &self,
            _: &mut Tx,
            _: &crate::models::notification::Notification,
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
    }

    #[async_trait]
    impl super::notification_repository::NotificationPreferenceRepository for super::NullRepository {
        async fn find_by_user(
            &self,
            _: &str,
        ) -> Result<Option<crate::models::notification::NotificationPreference>, crate::error::DomainError> {
            Ok(None)
        }
        async fn save(
            &self,
            _: &crate::models::notification::NotificationPreference,
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
    }

    #[async_trait]
    impl super::online_history_repository::OnlineHistoryRepository for super::NullRepository {
        async fn record_login(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn record_logout(&self, _: &str, _: &str, _: bool) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn list_history(
            &self,
            _: Option<&str>,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: i64,
            _: i64,
        ) -> Result<Vec<crate::models::online_history::OnlineHistoryRecord>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn count_history(
            &self,
            _: Option<&str>,
            _: Option<chrono::DateTime<chrono::Utc>>,
            _: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
    }

    #[async_trait]
    impl super::user_repository::UserRepository for super::NullRepository {
        async fn find_by_id(&self, _: &str) -> Result<Option<crate::models::user::User>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_username(
            &self,
            _: &str,
        ) -> Result<Option<crate::models::user::User>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_email(&self, _: &str) -> Result<Option<crate::models::user::User>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_all(&self, _: i64, _: i64) -> Result<Vec<crate::models::user::User>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn list_distinct_departments_in_use(&self) -> Result<Vec<String>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn has_any_user_with_department_id(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn save(&self, _: &crate::models::user::User) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn update(&self, _: &crate::models::user::User) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn delete(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn update_password(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn update_last_login(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn find_permission_version_by_id(&self, _: &str) -> Result<Option<i32>, crate::error::DomainError> {
            Ok(None)
        }
    }

    #[async_trait]
    impl super::user_repository::RoleRepository for super::NullRepository {
        async fn find_by_id(&self, _: &str) -> Result<Option<crate::models::user::Role>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_by_name(&self, _: &str) -> Result<Option<crate::models::user::Role>, crate::error::DomainError> {
            Ok(None)
        }
        async fn find_all(&self) -> Result<Vec<crate::models::user::Role>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn save(&self, _: &crate::models::user::Role) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn delete(&self, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn find_by_user_id(&self, _: &str) -> Result<Vec<crate::models::user::Role>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn count_users(&self, _: &str) -> Result<i64, crate::error::DomainError> {
            Ok(0)
        }
        async fn set_permissions(&self, _: &str, _: &[String]) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn assign_role_to_user(&self, _: &str, _: &str) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn remove_user_from_role(&self, _: &str, _: &str) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn add_permission(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn remove_permission(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl super::user_repository::PermissionRepository for super::NullRepository {
        async fn find_all(&self) -> Result<Vec<crate::models::user::Permission>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn find_by_role_id(
            &self,
            _: &str,
        ) -> Result<Vec<crate::models::user::Permission>, crate::error::DomainError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl super::session_runtime_repository::SessionRuntimeRepository for super::NullRepository {
        async fn establish_session(
            &self,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<crate::models::session_runtime::SessionEstablishResult, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn validate_refresh_token(&self, _: &str, _: &str) -> Result<bool, crate::error::DomainError> {
            Ok(false)
        }
        async fn revoke_refresh_tokens(&self, _: &str) -> Result<(), crate::error::DomainError> {
            Ok(())
        }
        async fn revoke_session(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<crate::models::session_runtime::OnlineSessionStatus>, crate::error::DomainError> {
            Ok(None)
        }
        async fn heartbeat(
            &self,
            _: &str,
        ) -> Result<Option<crate::models::session_runtime::OnlineSessionStatus>, crate::error::DomainError> {
            Ok(None)
        }
        async fn get_online_users(&self) -> Result<Vec<String>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn get_online_status(
            &self,
            _: &str,
        ) -> Result<crate::models::session_runtime::OnlineSessionStatus, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn get_all_online_status(
            &self,
        ) -> Result<Vec<crate::models::session_runtime::OnlineSessionStatus>, crate::error::DomainError> {
            Ok(vec![])
        }
        async fn get_runtime_status(
            &self,
        ) -> Result<crate::models::session_runtime::SessionRuntimeStatus, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn get_permission_version(&self, _: &str) -> Result<i64, crate::error::DomainError> {
            Ok(1)
        }
    }

    #[async_trait]
    impl super::todo_agent_context_repository::TodoAgentContextRepository for super::NullRepository {
        async fn get(
            &self,
            _: &str,
        ) -> Result<Option<super::todo_agent_context_repository::TodoAgentContext>, crate::error::DomainError> {
            Ok(None)
        }
        async fn batch_get(
            &self,
            _: &[String],
        ) -> Result<
            std::collections::HashMap<String, super::todo_agent_context_repository::TodoAgentContext>,
            crate::error::DomainError,
        > {
            Ok(std::collections::HashMap::new())
        }
        async fn upsert_partial(
            &self,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: &str,
        ) -> Result<super::todo_agent_context_repository::TodoAgentContext, crate::error::DomainError> {
            Err(crate::error::DomainError::Internal("NullRepository".into()))
        }
        async fn find_todo_ids(
            &self,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: i64,
            _: i64,
        ) -> Result<Vec<String>, crate::error::DomainError> {
            Ok(vec![])
        }
        fn get_metrics_snapshot(&self) -> std::collections::HashMap<String, serde_json::Value> {
            std::collections::HashMap::new()
        }
    }
}
