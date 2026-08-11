//! 移动端领域服务装配：mobile_device / mobile_upload /
//! mobile_workbench / mobile_operations。

use std::sync::Arc;

use crate::di::types::*;

use fms_application::services::mobile_device_service::MobileDeviceService;
use fms_application::services::mobile_operations_service::MobileOperationsService;
use fms_application::services::mobile_upload_service::MobileUploadService;
use fms_application::services::mobile_workbench_service::MobileWorkbenchService;

use crate::di::dispatch::DispatchServices;
use crate::di::observability::ObservabilityServices;
use crate::di::shared::{SharedInfra, SharedRepos, SharedServices};

pub(crate) struct MobileServices {
    pub mobile_device_svc: Arc<ConcreteMobileDeviceService>,
    pub mobile_upload_svc: Arc<MobileUploadService>,
    pub mobile_workbench_svc: Arc<ConcreteMobileWorkbenchService>,
    pub mobile_operations_svc: Arc<ConcreteMobileOperationsService>,
}

pub(crate) fn build_mobile_services(
    repos: &SharedRepos,
    infra: &SharedInfra,
    shared: &SharedServices,
    dispatch: &DispatchServices,
    observability: &ObservabilityServices,
) -> MobileServices {
    let mobile_device_svc = Arc::new(
        MobileDeviceService::new(
            repos.mobile_device_repo.clone(),
            std::env::var("MOBILE_DEVICE_STALE_MINUTES")
                .ok()
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(10),
        )
        .with_metrics_recorder(infra.mobile_realtime_metrics_recorder.clone()),
    );
    let mobile_upload_svc = Arc::new(MobileUploadService::new(
        repos.mobile_upload_repo.clone(),
        std::env::var("MOBILE_UPLOAD_STORAGE_ROOT").unwrap_or_else(|_| "data/mobile_uploads".to_string()),
        std::env::var("MOBILE_UPLOAD_MAX_FILE_SIZE_MB")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(20),
    ));
    let mobile_workbench_svc = Arc::new(MobileWorkbenchService::new(
        dispatch.dispatch_query_svc.clone(),
        Some(shared.notification_svc.clone()),
        Some(dispatch.dispatch_chat_svc.clone()),
        Some(observability.shift_handover_svc.clone()),
        Some(mobile_device_svc.clone()),
        Some(shared.todo_svc.clone()),
    ));
    let mobile_operations_svc = Arc::new(MobileOperationsService::new(
        dispatch.dispatch_query_svc.clone(),
        Some(dispatch.anomaly_svc.clone()),
        Some(shared.notification_svc.clone()),
    ));

    MobileServices {
        mobile_device_svc,
        mobile_upload_svc,
        mobile_workbench_svc,
        mobile_operations_svc,
    }
}
