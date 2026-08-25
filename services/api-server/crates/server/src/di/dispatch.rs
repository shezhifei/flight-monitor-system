//! 派工领域服务装配：dispatch_query / dispatch / dispatch_frontend_replan /
//! dispatch_collaboration_query / dispatch_chat / dispatch_rule /
//! dispatch_schedule / dispatch_analytics / dispatch_scenario /
//! dispatch_resource / resource_utilization / resource_availability /
//! workflow_dispatch / anomaly / llm_eval。

use std::sync::Arc;

use crate::di::types::*;

use crate::config::{env_flag, env_i64};
use fms_application::services::anomaly_service::AnomalyService;
use fms_application::services::dispatch_analytics_service::DispatchAnalyticsService;
use fms_application::services::dispatch_chat_service::DispatchChatService;
use fms_application::services::dispatch_collaboration_query_service::DispatchCollaborationQueryService;
use fms_application::services::dispatch_frontend_replan_service::DispatchFrontendReplanService;
use fms_application::services::dispatch_query_service::DispatchQueryService;
use fms_application::services::dispatch_resource_service::DispatchResourceService;
use fms_application::services::dispatch_rule_service::DispatchRuleService;
use fms_application::services::dispatch_scenario_service::DispatchScenarioService;
use fms_application::services::dispatch_schedule_service::DispatchScheduleService;
use fms_application::services::dispatch_service::dispatch_overrun_warning_service::DispatchOverrunWarningService;
use fms_application::services::dispatch_service::{
    DispatchNotificationServiceDependencies, DispatchOrderServiceDependencies, DispatchResourceServiceDependencies,
    DispatchRuleServiceDependencies, DispatchService, DispatchServiceDependencies,
};
use fms_application::services::event_rule_admin_service::EventRuleAdminService;
use fms_application::services::llm_eval_service::LLMEvalService;
use fms_application::services::resource_availability_service::{
    ResourceAvailabilityGateway, ResourceAvailabilityService,
};
use fms_application::services::resource_utilization_service::ResourceUtilizationService;
use fms_application::services::workflow_dispatch_service::WorkflowDispatchService;
use fms_application::sqlx_transactional_repositories::{
    SqlxDispatchOrderMemberTransactionalRepository, SqlxDispatchOrderTransactionalRepository,
};
use fms_domain::broadcaster::Broadcaster;

use fms_domain::ports::dispatch_repository::{
    DepartmentRepository, DispatchOrderRepository, EquipmentRepository, EquipmentTypeRepository,
    ScheduleExceptionRepository, ShiftInstanceRepository, ShiftTemplateRepository, StandRepository, TaskTypeRepository,
    TeamMemberRepository, TeamRepository, TeamTypeRepository,
};
use fms_domain::ports::event_rule_repository::EventRuleRepository;
use fms_domain::ports::user_repository::UserRepository;
use fms_domain::ports::workflow_dispatch_repository::WorkflowDispatchRepository;
use fms_infrastructure::repositories::pg_event_rule_repository::PgEventRuleRepository;

use crate::di::auth::AuthServices;
use crate::di::flight::FlightServices;
use crate::di::shared::{SharedInfra, SharedRepos, SharedServices};

pub(crate) struct DispatchServices {
    pub dispatch_query_svc: Arc<ConcreteDispatchQueryService>,
    pub dispatch_svc: Arc<ConcreteDispatchService>,
    pub dispatch_overrun_warning_svc: Arc<DispatchOverrunWarningService>,
    pub dispatch_frontend_replan_svc: Arc<DispatchFrontendReplanService>,
    pub llm_eval_svc: Arc<LLMEvalService>,
    pub dispatch_collaboration_query_svc: Arc<DispatchCollaborationQueryService>,
    pub dispatch_chat_svc: Arc<ConcreteDispatchChatService>,
    pub dispatch_rule_svc: Arc<DispatchRuleService>,
    pub event_rule_admin_svc: Arc<ConcreteEventRuleAdminService>,
    pub event_rule_repo: Arc<dyn EventRuleRepository + Send + Sync>,
    pub dispatch_schedule_svc: Arc<ConcreteDispatchScheduleService>,
    pub dispatch_analytics_svc: Arc<DispatchAnalyticsService>,
    pub dispatch_scenario_svc: Arc<DispatchScenarioService>,
    pub dispatch_resource_svc: Arc<ConcreteDispatchResourceService>,
    pub resource_utilization_svc: Arc<ResourceUtilizationService>,
    pub resource_availability_svc: Arc<ResourceAvailabilityService>,
    pub workflow_dispatch_svc: Arc<ConcreteWorkflowDispatchService>,
    pub anomaly_svc: Arc<ConcreteAnomalyService>,
}

pub(crate) fn build_dispatch_services(
    repos: &SharedRepos,
    infra: &SharedInfra,
    shared: &SharedServices,
    auth: &AuthServices,
    flight: &FlightServices,
) -> DispatchServices {
    let dispatch_query_svc = Arc::new(DispatchQueryService::new(
        repos.dispatch_order_repo.clone(),
        repos.dispatch_collaboration_repo.clone(),
    ));
    let llm_eval_svc = Arc::new(LLMEvalService::new(
        std::env::var("LLM_EVAL_MAX_RETAINED_JOBS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(30),
        std::env::var("LLM_EVAL_CASES_FILE").ok(),
    ));
    let dispatch_collaboration_query_svc = Arc::new(DispatchCollaborationQueryService::new(
        repos.dispatch_collaboration_repo.clone(),
        repos.dispatch_order_repo.clone(),
    ));
    let dispatch_chat_svc = Arc::new(
        DispatchChatService::new(repos.dispatch_collaboration_repo.clone())
            .with_dispatch_order_repo(repos.dispatch_order_repo.clone())
            .with_flight_repo(repos.flight_repo.clone())
            .with_event_publisher(infra.dispatch_chat_event_publisher.clone())
            .with_mention_notifier(shared.notification_svc.clone()),
    );
    let resource_availability_svc = Arc::new(ResourceAvailabilityService::new(
        repos.shift_instance_repo.clone(),
        repos.schedule_exception_repo.clone(),
        repos.team_member_repo.clone(),
        repos.team_repo.clone(),
        repos.dispatch_order_repo.clone(),
        repos.dispatch_member_repo.clone(),
    ));
    let sse_broadcaster: Arc<dyn Broadcaster + Send + Sync> = infra.sse_hub.clone();
    let scan_interval_secs = env_i64("DISPATCH_OVERRUN_SCAN_INTERVAL_SECS", 30).max(1) as u64;
    let dispatch_overrun_warning_svc = Arc::new(
        DispatchOverrunWarningService::new(repos.dispatch_order_repo.clone(), repos.dispatch_alert_repo.clone())
            .with_generation_rule_repo(repos.generation_rule_repo.clone())
            .with_broadcaster(sse_broadcaster)
            .with_feature_flags(
                env_flag("DISPATCH_OVERRUN_WARNING_ENABLED", true),
                env_flag("DISPATCH_OVERRUN_SSE_ENABLED", true),
            )
            .with_scan_interval(std::time::Duration::from_secs(scan_interval_secs)),
    );
    let dispatch_svc = Arc::new(DispatchService::new(DispatchServiceDependencies {
        order: DispatchOrderServiceDependencies {
            order_repo: repos.dispatch_order_repo.clone(),
            order_tx_repo: repos.dispatch_order_repo.clone() as Arc<dyn SqlxDispatchOrderTransactionalRepository>,
            member_repo: repos.dispatch_member_repo.clone(),
            member_tx_repo: repos.dispatch_member_repo.clone()
                as Arc<dyn SqlxDispatchOrderMemberTransactionalRepository>,
            todo_repo: repos.todo_repo.clone(),
        },
        rules: DispatchRuleServiceDependencies {
            department_repo: repos.department_repo.clone(),
            task_type_repo: repos.task_type_repo.clone(),
            task_type_requirement_repo: repos.task_type_requirement_repo.clone(),
            flight_repo: repos.flight_repo.clone(),
            generation_rule_repo: repos.generation_rule_repo.clone(),
            adjustment_rule_repo: repos.adjustment_rule_repo.clone(),
            temporary_task_template_repo: repos.temporary_task_template_repo.clone(),
        },
        resources: DispatchResourceServiceDependencies {
            team_repo: repos.team_repo.clone(),
            team_type_repo: repos.team_type_repo.clone(),
            stand_repo: repos.stand_repo.clone(),
            qualification_repo: repos.qualification_repo.clone(),
            qualification_grant_repo: repos.qualification_grant_repo.clone(),
            equipment_repo: repos.equipment_repo.clone(),
            team_member_repo: repos.team_member_repo.clone(),
            travel_stats_repo: repos.dispatch_travel_stats_repo.clone(),
            checklist_repo: repos.dispatch_checklist_repo.clone(),
            resource_availability_service: resource_availability_svc.clone(),
        },
        notifications: DispatchNotificationServiceDependencies {
            anomaly_repo: repos.anomaly_repo.clone(),
            collaboration_repo: repos.dispatch_collaboration_repo.clone(),
            alert_repo: repos.dispatch_alert_repo.clone(),
            notification_service: shared.notification_svc.clone(),
            dispatch_chat_service: dispatch_chat_svc.clone(),
        },
        overrun_warning_service: dispatch_overrun_warning_svc.clone(),
    }));
    let dispatch_frontend_replan_svc = Arc::new(
        DispatchFrontendReplanService::new(repos.dispatch_order_repo.clone(), repos.dispatch_member_repo.clone())
            .with_resource_repos(
                repos.team_repo.clone(),
                repos.team_member_repo.clone(),
                repos.equipment_repo.clone(),
                Some(repos.dispatch_travel_stats_repo.clone()),
            )
            .with_notification_service(shared.notification_svc.clone())
            .with_collaboration_repo(repos.dispatch_collaboration_repo.clone())
            .with_dispatch_chat_service(dispatch_chat_svc.clone())
            .with_generation_rule_repo(repos.generation_rule_repo.clone())
            .with_qualification_repos(repos.qualification_repo.clone(), repos.qualification_grant_repo.clone()),
    );
    let anomaly_svc =
        Arc::new(AnomalyService::new(repos.anomaly_repo.clone()).with_flight_repository(repos.flight_repo.clone()));
    let dispatch_resource_svc = Arc::new(DispatchResourceService::new(
        repos.department_repo.clone() as Arc<dyn DepartmentRepository + Send + Sync>,
        repos.team_type_repo.clone() as Arc<dyn TeamTypeRepository + Send + Sync>,
        repos.team_repo.clone() as Arc<dyn TeamRepository + Send + Sync>,
        repos.team_member_repo.clone() as Arc<dyn TeamMemberRepository + Send + Sync>,
        repos.equipment_type_repo.clone() as Arc<dyn EquipmentTypeRepository + Send + Sync>,
        repos.equipment_repo.clone() as Arc<dyn EquipmentRepository + Send + Sync>,
        repos.stand_repo.clone() as Arc<dyn StandRepository + Send + Sync>,
        repos.task_type_repo.clone() as Arc<dyn TaskTypeRepository + Send + Sync>,
    ));
    let resource_utilization_svc = Arc::new(ResourceUtilizationService::new(repos.dispatch_order_repo.clone()));
    let dispatch_rule_svc = Arc::new(DispatchRuleService::new(
        repos.department_repo.clone(),
        repos.qualification_repo.clone(),
        repos.qualification_grant_repo.clone(),
        repos.task_type_requirement_repo.clone(),
        repos.generation_rule_repo.clone(),
        repos.adjustment_rule_repo.clone(),
        repos.temporary_task_template_repo.clone(),
    ));
    let event_rule_repo: Arc<dyn EventRuleRepository + Send + Sync> =
        Arc::new(PgEventRuleRepository::new(repos.pool.clone()));
    let event_rule_dispatch_order_repo: Arc<dyn DispatchOrderRepository + Send + Sync> =
        repos.dispatch_order_repo.clone();
    let event_rule_admin_svc: Arc<ConcreteEventRuleAdminService> = Arc::new(EventRuleAdminService::new(
        event_rule_repo.clone(),
        event_rule_dispatch_order_repo,
    ));
    // 显式转成 trait object：API 处理器与 DI 必须落在同一个单态上。
    let dispatch_schedule_svc = Arc::new(DispatchScheduleService::new(
        repos.shift_template_repo.clone() as Arc<dyn ShiftTemplateRepository + Send + Sync>,
        repos.shift_instance_repo.clone() as Arc<dyn ShiftInstanceRepository + Send + Sync>,
        repos.schedule_exception_repo.clone() as Arc<dyn ScheduleExceptionRepository + Send + Sync>,
        repos.team_repo.clone() as Arc<dyn TeamRepository + Send + Sync>,
        repos.team_member_repo.clone() as Arc<dyn TeamMemberRepository + Send + Sync>,
        repos.equipment_repo.clone() as Arc<dyn EquipmentRepository + Send + Sync>,
        resource_availability_svc.clone() as Arc<dyn ResourceAvailabilityGateway + Send + Sync>,
    ));
    let dispatch_analytics_svc = Arc::new(DispatchAnalyticsService::new(
        repos.dispatch_order_repo.clone(),
        dispatch_query_svc.clone(),
        resource_utilization_svc.clone(),
    ));
    let dispatch_scenario_svc = Arc::new(DispatchScenarioService::new(repos.dispatch_order_repo.clone()));

    let workflow_dispatch_order_repo: Arc<dyn DispatchOrderRepository + Send + Sync> =
        repos.dispatch_order_repo.clone();
    let workflow_user_repo: Arc<dyn UserRepository + Send + Sync> = repos.user_repo.clone();
    let workflow_repo: Arc<dyn WorkflowDispatchRepository + Send + Sync> = repos.workflow_dispatch_repo.clone();
    let workflow_dispatch_svc: Arc<ConcreteWorkflowDispatchService> = Arc::new(
        WorkflowDispatchService::new(workflow_dispatch_order_repo, workflow_user_repo, workflow_repo)
            .with_auth_service(auth.auth_svc.clone())
            .with_notification_service(shared.notification_svc.clone())
            .with_flowable_service(infra.flowable_svc.clone())
            .with_dispatch_chat_service(dispatch_chat_svc.clone())
            .with_sse_publisher(infra.workflow_dispatch_sse_publisher.clone()
                as Arc<dyn fms_application::services::workflow_dispatch_service::WorkflowDispatchSsePublisher>)
            .with_dispatch_recommendation_service(Arc::new(NoopDispatchRecommendationService)
                as Arc<dyn fms_application::services::workflow_dispatch_service::DispatchRecommendationService>),
    );

    // flight 入参保留以维持装配契约（派工链路目前直接通过 repos 取 flight_repo）。
    let _ = flight;

    DispatchServices {
        dispatch_query_svc,
        dispatch_svc,
        dispatch_overrun_warning_svc,
        dispatch_frontend_replan_svc,
        llm_eval_svc,
        dispatch_collaboration_query_svc,
        dispatch_chat_svc,
        dispatch_rule_svc,
        event_rule_admin_svc,
        event_rule_repo,
        dispatch_schedule_svc,
        dispatch_analytics_svc,
        dispatch_scenario_svc,
        dispatch_resource_svc,
        resource_utilization_svc,
        resource_availability_svc,
        workflow_dispatch_svc,
        anomaly_svc,
    }
}
