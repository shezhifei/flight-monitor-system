//! 业务案例领域服务装配：business_case_type / business_case /
//! business_case_workflow。同时负责航班运行时投影预热，以及将
//! business_case_workflow_svc 注册为 notification 的回执分组同步器。

use std::sync::Arc;

use tracing::{info, warn};

use crate::di::types::*;

use fms_application::services::business_case_service::{
    BusinessCaseEventPublisher, BusinessCaseMentionAudience, BusinessCaseService, CollaborationMentionAudience,
};
use fms_application::services::business_case_type_service::BusinessCaseTypeService;
use fms_application::services::business_case_workflow_service::BusinessCaseWorkflowService;
use fms_application::services::notification_service::NotificationReceiptGroupSync;

use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;

use crate::di::dispatch::DispatchServices;
use crate::di::flight::FlightServices;
use crate::di::shared::{SharedInfra, SharedRepos, SharedServices};

pub(crate) struct BusinessCaseServices {
    pub business_case_svc: Arc<ConcreteBusinessCaseService>,
    pub business_case_type_svc: Arc<BusinessCaseTypeService>,
    pub business_case_workflow_svc: Arc<BusinessCaseWorkflowService>,
}

pub(crate) async fn build_business_case_services(
    repos: &SharedRepos,
    infra: &SharedInfra,
    shared: &SharedServices,
    flight: &FlightServices,
    dispatch: &DispatchServices,
) -> BusinessCaseServices {
    let business_case_type_svc = Arc::new(BusinessCaseTypeService::new(repos.business_case_type_repo.clone()));
    let business_case_repo_for_service: Arc<dyn BusinessCaseRepository + Send + Sync> =
        repos.business_case_repo.clone();
    let business_case_dispatch_chat_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync> =
        repos.dispatch_collaboration_repo.clone();
    // 两条分支以前只差事件发布器，却把整条构造链重复了一遍。
    // 依赖必填之后，先挑发布器，再构造一次。
    let event_publisher: Arc<dyn BusinessCaseEventPublisher> = match &infra.business_case_event_publisher {
        Some(publisher) => publisher.clone(),
        None => Arc::new(NoopBusinessCaseEventPublisher),
    };
    let mention_audience: Arc<dyn BusinessCaseMentionAudience> =
        Arc::new(CollaborationMentionAudience::new(business_case_dispatch_chat_repo));
    let mut business_case_svc_inner: ConcreteBusinessCaseService = BusinessCaseService::new(
        business_case_repo_for_service,
        event_publisher,
        mention_audience,
    );
    business_case_svc_inner.set_notification_service(shared.notification_svc.clone());
    business_case_svc_inner.set_business_case_type_service(business_case_type_svc.clone());
    business_case_svc_inner.set_flight_runtime_projection_repository(repos.flight_runtime_projection_repo.clone());
    let business_case_svc = Arc::new(business_case_svc_inner);

    match repos.flight_runtime_projection_repo.rebuild_recent(200).await {
        Ok(count) => {
            info!(rebuilt = count, "flight runtime list projection warmup completed");
        }
        Err(error) => {
            warn!(error = %error, "flight runtime list projection warmup failed");
        }
    }

    let business_case_workflow_svc = Arc::new(
        BusinessCaseWorkflowService::new(
            repos.business_case_workflow_run_repo.clone(),
            business_case_svc.clone(),
            flight.flight_svc.clone(),
        )
        .with_business_case_type_service(business_case_type_svc.clone())
        .with_flowable_service(infra.flowable_svc.clone())
        .with_notification_service(shared.notification_svc.clone())
        .with_user_repository(repos.auth_user_repo.clone())
        .with_bpmn_dir("bpmn")
        .with_workflow_dispatch_service(dispatch.workflow_dispatch_svc.clone())
        .with_flight_runtime_projection_repository(repos.flight_runtime_projection_repo.clone()),
    );
    shared
        .notification_svc
        .set_receipt_group_sync(business_case_workflow_svc.clone() as Arc<dyn NotificationReceiptGroupSync>);

    BusinessCaseServices {
        business_case_svc,
        business_case_type_svc,
        business_case_workflow_svc,
    }
}
