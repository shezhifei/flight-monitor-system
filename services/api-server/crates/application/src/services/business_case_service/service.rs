use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;

use crate::services::notification_service::DispatchBatchNotificationCreate;
use crate::types::{ConcreteBusinessCaseTypeService, ConcreteNotificationService};
use fms_domain::error::DomainError;
use fms_domain::models::business_case::{
    BusinessCaseAppendEntry, BusinessCaseTerminalMetadata, FlightBusinessCase, VisibilityScope,
};
use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_domain::ports::flight_runtime_projection_repository::FlightRuntimeProjectionRepository;
use tracing::warn;

use crate::sqlx_transactional_repositories::SqlxBusinessCaseTransactionalRepository;

use super::schemas::{
    BusinessCaseAppendResult, BusinessCaseEventPublisher, BusinessCaseMentionAudience,
    BusinessCaseTerminalUpdatePayload, BusinessCaseUpdatePayload, BUSINESS_CASE_ALLOWED_STATUSES,
};

pub struct BusinessCaseService<
    BR: BusinessCaseRepository + ?Sized,
    EP: BusinessCaseEventPublisher + ?Sized,
    MA: BusinessCaseMentionAudience + ?Sized,
> {
    repo: Arc<BR>,
    tx_repo: Option<Arc<dyn SqlxBusinessCaseTransactionalRepository>>,
    event_publisher: Arc<EP>,
    mention_audience: Arc<MA>,
    notification_svc: Option<Arc<ConcreteNotificationService>>,
    business_case_type_svc: Option<Arc<ConcreteBusinessCaseTypeService>>,
    flight_runtime_projection_repo: Option<Arc<dyn FlightRuntimeProjectionRepository>>,
}

/// Trait abstracting the subset of `BusinessCaseService` operations used by
/// `AiBusinessCaseCopilotService` and `BusinessCaseWorkflowService`.
///
/// This enables test-only fake implementations without coupling the copilot
/// service to `PgBusinessCaseRepository`.
#[async_trait::async_trait]
pub trait BusinessCaseServiceOps: Send + Sync {
    async fn get(&self, case_id: &str) -> Result<Option<FlightBusinessCase>, DomainError>;
    async fn get_accessible(
        &self,
        case_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<FlightBusinessCase>, DomainError>;
    async fn create_for_viewer(
        &self,
        case_type: &str,
        flight_id: &str,
        flight_no: &str,
        description: &str,
        context: HashMap<String, serde_json::Value>,
        status: Option<&str>,
        actor: &str,
        visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<FlightBusinessCase, DomainError>;
    async fn create_workflow_case_for_viewer(
        &self,
        flight_id: &str,
        flight_no: &str,
        case_type: &str,
        description: &str,
        actor: &str,
        context: HashMap<String, serde_json::Value>,
        stand: Option<String>,
        gate: Option<String>,
        visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<FlightBusinessCase, DomainError>;
    async fn delete(&self, case_id: &str) -> Result<bool, DomainError>;
    async fn get_by_flight_for_viewer(
        &self,
        flight_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn find_by_copilot_batch_action(
        &self,
        batch_id: &str,
        action_id: &str,
    ) -> Result<Option<FlightBusinessCase>, DomainError>;
    async fn list_by_copilot_batch(&self, batch_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn apply_workflow_terminal_action(
        &self,
        case_id: &str,
        payload: BusinessCaseTerminalUpdatePayload,
    ) -> Result<Option<FlightBusinessCase>, DomainError>;
}

#[async_trait::async_trait]
impl<BR, EP, MA> BusinessCaseServiceOps for BusinessCaseService<BR, EP, MA>
where
    BR: BusinessCaseRepository + ?Sized + Send + Sync,
    EP: BusinessCaseEventPublisher + ?Sized + Send + Sync,
    MA: BusinessCaseMentionAudience + ?Sized + Send + Sync,
{
    async fn get(&self, case_id: &str) -> Result<Option<FlightBusinessCase>, DomainError> {
        self.get(case_id).await
    }

    async fn create_for_viewer(
        &self,
        case_type: &str,
        flight_id: &str,
        flight_no: &str,
        description: &str,
        context: HashMap<String, serde_json::Value>,
        status: Option<&str>,
        actor: &str,
        visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<FlightBusinessCase, DomainError> {
        self.create_for_viewer(
            case_type,
            flight_id,
            flight_no,
            description,
            context,
            status,
            actor,
            visibility_scope,
            viewer_department_id,
            viewer_department_name,
        )
        .await
    }

    async fn delete(&self, case_id: &str) -> Result<bool, DomainError> {
        self.delete(case_id).await
    }

    async fn get_by_flight_for_viewer(
        &self,
        flight_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        self.get_by_flight_for_viewer(flight_id, viewer_department_id, viewer_department_name)
            .await
    }

    async fn find_by_copilot_batch_action(
        &self,
        batch_id: &str,
        action_id: &str,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        self.repo.find_by_copilot_batch_action(batch_id, action_id).await
    }

    async fn list_by_copilot_batch(&self, batch_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
        self.repo.list_by_copilot_batch(batch_id).await
    }

    async fn get_accessible(
        &self,
        case_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        self.get_accessible(case_id, viewer_department_id, viewer_department_name)
            .await
    }

    async fn create_workflow_case_for_viewer(
        &self,
        flight_id: &str,
        flight_no: &str,
        case_type: &str,
        description: &str,
        actor: &str,
        context: HashMap<String, serde_json::Value>,
        stand: Option<String>,
        gate: Option<String>,
        visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<FlightBusinessCase, DomainError> {
        self.create_workflow_case_for_viewer(
            flight_id,
            flight_no,
            case_type,
            description,
            actor,
            context,
            stand,
            gate,
            visibility_scope,
            viewer_department_id,
            viewer_department_name,
        )
        .await
    }

    async fn apply_workflow_terminal_action(
        &self,
        case_id: &str,
        payload: BusinessCaseTerminalUpdatePayload,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        self.apply_workflow_terminal_action(case_id, payload).await
    }
}

fn normalize_business_case_status(status: &str) -> Result<String, DomainError> {
    let normalized = status.trim().to_uppercase();
    if normalized.is_empty() {
        return Err(DomainError::ValidationError("业务事项状态不能为空".to_string()));
    }
    if BUSINESS_CASE_ALLOWED_STATUSES.contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(DomainError::ValidationError(format!(
        "不支持的业务事项状态: {normalized}"
    )))
}

fn resolve_create_status(
    explicit_status: Option<&str>,
    context: &HashMap<String, serde_json::Value>,
) -> Result<String, DomainError> {
    if let Some(status) = explicit_status {
        return normalize_business_case_status(status);
    }

    if let Some(status) = context.get("status").and_then(|value| value.as_str()) {
        return normalize_business_case_status(status);
    }

    Ok("PENDING".to_string())
}

fn normalize_optional_scope_value(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|item| !item.is_empty()).map(str::to_string)
}

fn require_department_scope(
    viewer_department_id: Option<&str>,
    viewer_department_name: Option<&str>,
) -> Result<(Option<String>, Option<String>), DomainError> {
    let department_id = normalize_optional_scope_value(viewer_department_id);
    let department_name = normalize_optional_scope_value(viewer_department_name);
    if department_id.is_none() && department_name.is_none() {
        return Err(DomainError::ValidationError(
            "当前用户未绑定业务部门，无法创建部门事项".to_string(),
        ));
    }
    Ok((department_id, department_name))
}

impl<
        BR: BusinessCaseRepository + ?Sized,
        EP: BusinessCaseEventPublisher + ?Sized,
        MA: BusinessCaseMentionAudience + ?Sized,
    > BusinessCaseService<BR, EP, MA>
{
    /// 把请求里的 @提及收敛到该航班允许被提及的人。空请求不查库。
    async fn retain_mentionable(&self, flight_id: &str, requested: Vec<String>) -> Vec<String> {
        if requested.is_empty() {
            return Vec::new();
        }
        let permitted: std::collections::HashSet<String> =
            self.mention_audience.mentionable_user_ids(flight_id).await.into_iter().collect();
        requested
            .into_iter()
            .filter(|uid| permitted.contains(uid.trim()))
            .collect()
    }

    /// 事件发布与 @提及范围都是必填的：以前它们是 `Option` + 空实现桩默认值，
    /// 于是「忘了接线」和「故意不接」在类型上无法区分，只能在运行时静默跳过。
    pub fn new(repo: Arc<BR>, event_publisher: Arc<EP>, mention_audience: Arc<MA>) -> Self {
        Self {
            repo,
            tx_repo: None,
            event_publisher,
            mention_audience,
            notification_svc: None,
            business_case_type_svc: None,
            flight_runtime_projection_repo: None,
        }
    }

    pub fn set_notification_service(&mut self, svc: Arc<ConcreteNotificationService>) {
        self.notification_svc = Some(svc);
    }

    pub fn set_business_case_type_service(&mut self, svc: Arc<ConcreteBusinessCaseTypeService>) {
        self.business_case_type_svc = Some(svc);
    }

    pub fn set_flight_runtime_projection_repository(&mut self, repo: Arc<dyn FlightRuntimeProjectionRepository>) {
        self.flight_runtime_projection_repo = Some(repo);
    }

    pub fn with_transactional_repository(mut self, tx_repo: Arc<dyn SqlxBusinessCaseTransactionalRepository>) -> Self {
        self.tx_repo = Some(tx_repo);
        self
    }

    pub async fn refresh_flight_runtime_projection(&self, flight_id: &str) {
        if let Some(repo) = self.flight_runtime_projection_repo.as_ref() {
            if let Err(error) = repo.rebuild_for_flight(flight_id).await {
                warn!(
                    flight_id = %flight_id,
                    error = %error,
                    "failed to refresh flight runtime list projection after business case write"
                );
                if let Err(delete_error) = repo.delete_for_flight(flight_id).await {
                    warn!(
                        flight_id = %flight_id,
                        error = %delete_error,
                        "failed to delete stale flight runtime list projection after refresh failure"
                    );
                }
            }
        }
    }

    async fn enrich_case_type_names(
        &self,
        mut items: Vec<FlightBusinessCase>,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let Some(case_type_svc) = self.business_case_type_svc.as_ref() else {
            return Ok(items);
        };
        if items.is_empty() {
            return Ok(items);
        }

        let type_name_map = case_type_svc
            .list_case_types(false)
            .await?
            .into_iter()
            .map(|item| (item.code, item.name))
            .collect::<HashMap<_, _>>();

        for item in &mut items {
            if item
                .case_type_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
            {
                continue;
            }
            item.case_type_name = type_name_map.get(&item.case_type).cloned();
        }

        Ok(items)
    }

    async fn enrich_case_type_name(
        &self,
        item: Option<FlightBusinessCase>,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        let Some(item) = item else {
            return Ok(None);
        };
        let mut items = self
            .enrich_case_type_names(vec![item], viewer_department_id, viewer_department_name)
            .await?;
        Ok(items.pop())
    }

    async fn resolve_case_type_source_for_viewer(
        &self,
        case_type: &str,
        fallback_visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<(VisibilityScope, Option<String>, Option<String>), DomainError> {
        let Some(case_type_svc) = self.business_case_type_svc.as_ref() else {
            return match fallback_visibility_scope {
                VisibilityScope::Common => Ok((VisibilityScope::Common, None, None)),
                VisibilityScope::Department => {
                    let (department_id, department_name_snapshot) =
                        require_department_scope(viewer_department_id, viewer_department_name)?;
                    Ok((VisibilityScope::Department, department_id, department_name_snapshot))
                }
            };
        };

        let Some(case_type_def) = case_type_svc
            .find_by_code_for_viewer(case_type, viewer_department_id, viewer_department_name)
            .await?
        else {
            return Err(DomainError::ValidationError(
                "当前用户不可创建该业务事项类型".to_string(),
            ));
        };

        match case_type_def.visibility_scope {
            VisibilityScope::Common => Ok((VisibilityScope::Common, None, None)),
            VisibilityScope::Department => {
                let department_id = case_type_def
                    .department_id
                    .or_else(|| normalize_optional_scope_value(viewer_department_id));
                let department_name_snapshot = case_type_def
                    .department_name_snapshot
                    .or_else(|| normalize_optional_scope_value(viewer_department_name));
                if department_id.is_none() && department_name_snapshot.is_none() {
                    return Err(DomainError::ValidationError(
                        "当前业务事项类型缺少部门来源，无法创建部门事项".to_string(),
                    ));
                }
                Ok((VisibilityScope::Department, department_id, department_name_snapshot))
            }
        }
    }

    pub async fn list_filtered(
        &self,
        flight_id: Option<&str>,
        case_type: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let items = self
            .repo
            .find_filtered(flight_id, case_type, status, None, None)
            .await?;
        self.enrich_case_type_names(items, None, None).await
    }

    pub async fn list_filtered_for_viewer(
        &self,
        flight_id: Option<&str>,
        case_type: Option<&str>,
        status: Option<&str>,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let items = self
            .repo
            .find_filtered(flight_id, case_type, status, None, None)
            .await?;
        self.enrich_case_type_names(items, viewer_department_id, viewer_department_name)
            .await
    }

    pub async fn list(
        &self,
        status: Option<&str>,
        page: i64,
        size: i64,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let offset = (page - 1).max(0) * size;
        let items = self.repo.find_all(status, size, offset).await?;
        self.enrich_case_type_names(items, None, None).await
    }

    pub async fn get(&self, case_id: &str) -> Result<Option<FlightBusinessCase>, DomainError> {
        let item = self.repo.find_by_id(case_id).await?;
        self.enrich_case_type_name(item, None, None).await
    }

    pub async fn get_by_flight(&self, flight_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let items = self.repo.find_by_flight(flight_id).await?;
        self.enrich_case_type_names(items, None, None).await
    }

    pub async fn get_accessible(
        &self,
        case_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        let item = self.repo.find_by_id(case_id).await?;
        self.enrich_case_type_name(item, viewer_department_id, viewer_department_name)
            .await
    }

    pub async fn get_by_flight_for_viewer(
        &self,
        flight_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let items = self.repo.find_by_flight(flight_id).await?;
        self.enrich_case_type_names(items, viewer_department_id, viewer_department_name)
            .await
    }

    pub async fn get_by_flight_ids(
        &self,
        flight_ids: &[String],
    ) -> Result<HashMap<String, Vec<FlightBusinessCase>>, DomainError> {
        self.get_by_flight_ids_for_viewer(flight_ids, None, None).await
    }

    pub async fn get_by_flight_ids_for_viewer(
        &self,
        flight_ids: &[String],
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<HashMap<String, Vec<FlightBusinessCase>>, DomainError> {
        let normalized_ids = flight_ids
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .fold(Vec::<String>::new(), |mut acc, item| {
                if !acc.iter().any(|existing| existing == item) {
                    acc.push(item.to_string());
                }
                acc
            });

        if normalized_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let cases = self.repo.find_by_flight_ids(&normalized_ids).await?;
        let grouped = normalized_ids
            .into_iter()
            .map(|flight_id| (flight_id, Vec::new()))
            .collect::<HashMap<_, _>>();

        let mut grouped = grouped;
        for case in self
            .enrich_case_type_names(cases, viewer_department_id, viewer_department_name)
            .await?
        {
            grouped.entry(case.flight_id.clone()).or_default().push(case);
        }

        Ok(grouped)
    }

    pub async fn create(
        &self,
        case_type: &str,
        flight_id: &str,
        flight_no: &str,
        description: &str,
        context: HashMap<String, serde_json::Value>,
        status: Option<&str>,
        actor: &str,
    ) -> Result<FlightBusinessCase, DomainError> {
        let now = Utc::now();
        let resolved_status = resolve_create_status(status, &context)?;
        let case = FlightBusinessCase {
            case_id: ulid::Ulid::new().to_string(),
            case_type: case_type.trim().to_string(),
            case_type_name: None,
            flight_id: flight_id.trim().to_string(),
            flight_no: flight_no.trim().to_string(),
            created_at: now,
            created_by: actor.to_string(),
            updated_by: actor.to_string(),
            description: description.to_string(),
            status: resolved_status,
            stand: None,
            gate: None,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            finished_at: None,
            cancelled_at: None,
            log: vec![],
            context,
            workflow_receipt: None,
            terminal_metadata: None,
            append_count: 0,
            latest_append: None,
            append_entries: vec![],
        };
        self.repo.save(&case).await?;
        self.refresh_flight_runtime_projection(&case.flight_id).await;
        self.enrich_case_type_name(Some(case), None, None)
            .await?
            .ok_or_else(|| DomainError::Internal("failed to enrich created business case".into()))
    }

    pub async fn create_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        case_type: &str,
        flight_id: &str,
        flight_no: &str,
        description: &str,
        context: HashMap<String, serde_json::Value>,
        status: Option<&str>,
        actor: &str,
    ) -> Result<FlightBusinessCase, DomainError> {
        let now = Utc::now();
        let resolved_status = resolve_create_status(status, &context)?;
        let case = FlightBusinessCase {
            case_id: ulid::Ulid::new().to_string(),
            case_type: case_type.trim().to_string(),
            case_type_name: None,
            flight_id: flight_id.trim().to_string(),
            flight_no: flight_no.trim().to_string(),
            created_at: now,
            created_by: actor.to_string(),
            updated_by: actor.to_string(),
            description: description.to_string(),
            status: resolved_status,
            stand: None,
            gate: None,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            finished_at: None,
            cancelled_at: None,
            log: vec![],
            context,
            workflow_receipt: None,
            terminal_metadata: None,
            append_count: 0,
            latest_append: None,
            append_entries: vec![],
        };
        let tx_repo = self.tx_repo.as_ref().ok_or_else(|| {
            DomainError::Internal("BusinessCaseService transactional repository is not configured".to_string())
        })?;
        tx_repo.save_in_tx(tx, &case).await?;
        self.enrich_case_type_name(Some(case), None, None)
            .await?
            .ok_or_else(|| DomainError::Internal("failed to enrich created business case".into()))
    }

    pub async fn create_for_viewer(
        &self,
        case_type: &str,
        flight_id: &str,
        flight_no: &str,
        description: &str,
        context: HashMap<String, serde_json::Value>,
        status: Option<&str>,
        actor: &str,
        visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<FlightBusinessCase, DomainError> {
        let now = Utc::now();
        let resolved_status = resolve_create_status(status, &context)?;
        let (visibility_scope, department_id, department_name_snapshot) = self
            .resolve_case_type_source_for_viewer(
                case_type,
                visibility_scope,
                viewer_department_id,
                viewer_department_name,
            )
            .await?;
        let case = FlightBusinessCase {
            case_id: ulid::Ulid::new().to_string(),
            case_type: case_type.trim().to_string(),
            case_type_name: None,
            flight_id: flight_id.trim().to_string(),
            flight_no: flight_no.trim().to_string(),
            created_at: now,
            created_by: actor.to_string(),
            updated_by: actor.to_string(),
            description: description.to_string(),
            status: resolved_status,
            stand: None,
            gate: None,
            visibility_scope,
            department_id,
            department_name_snapshot,
            finished_at: None,
            cancelled_at: None,
            log: vec![],
            context,
            workflow_receipt: None,
            terminal_metadata: None,
            append_count: 0,
            latest_append: None,
            append_entries: vec![],
        };
        self.repo.save(&case).await?;
        self.refresh_flight_runtime_projection(&case.flight_id).await;
        self.enrich_case_type_name(Some(case), viewer_department_id, viewer_department_name)
            .await?
            .ok_or_else(|| DomainError::Internal("failed to enrich created business case".into()))
    }

    pub async fn create_workflow_case(
        &self,
        flight_id: &str,
        flight_no: &str,
        case_type: &str,
        description: &str,
        actor: &str,
        context: HashMap<String, serde_json::Value>,
        stand: Option<String>,
        gate: Option<String>,
    ) -> Result<FlightBusinessCase, DomainError> {
        let now = Utc::now();
        let case = FlightBusinessCase {
            case_id: ulid::Ulid::new().to_string(),
            case_type: case_type.trim().to_string(),
            case_type_name: None,
            flight_id: flight_id.into(),
            flight_no: flight_no.into(),
            created_at: now,
            created_by: actor.into(),
            updated_by: actor.into(),
            description: description.into(),
            status: "PENDING".into(),
            stand,
            gate,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            finished_at: None,
            cancelled_at: None,
            log: vec![],
            context,
            workflow_receipt: None,
            terminal_metadata: None,
            append_count: 0,
            latest_append: None,
            append_entries: vec![],
        };
        self.repo.save(&case).await?;
        self.refresh_flight_runtime_projection(&case.flight_id).await;
        self.enrich_case_type_name(Some(case), None, None)
            .await?
            .ok_or_else(|| DomainError::Internal("failed to enrich workflow business case".into()))
    }

    pub async fn create_workflow_case_for_viewer(
        &self,
        flight_id: &str,
        flight_no: &str,
        case_type: &str,
        description: &str,
        actor: &str,
        context: HashMap<String, serde_json::Value>,
        stand: Option<String>,
        gate: Option<String>,
        visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<FlightBusinessCase, DomainError> {
        let now = Utc::now();
        let (visibility_scope, department_id, department_name_snapshot) = self
            .resolve_case_type_source_for_viewer(
                case_type,
                visibility_scope,
                viewer_department_id,
                viewer_department_name,
            )
            .await?;
        let case = FlightBusinessCase {
            case_id: ulid::Ulid::new().to_string(),
            case_type: case_type.trim().to_string(),
            case_type_name: None,
            flight_id: flight_id.into(),
            flight_no: flight_no.into(),
            created_at: now,
            created_by: actor.into(),
            updated_by: actor.into(),
            description: description.into(),
            status: "PENDING".into(),
            stand,
            gate,
            visibility_scope,
            department_id,
            department_name_snapshot,
            finished_at: None,
            cancelled_at: None,
            log: vec![],
            context,
            workflow_receipt: None,
            terminal_metadata: None,
            append_count: 0,
            latest_append: None,
            append_entries: vec![],
        };
        self.repo.save(&case).await?;
        self.refresh_flight_runtime_projection(&case.flight_id).await;
        self.enrich_case_type_name(Some(case), viewer_department_id, viewer_department_name)
            .await?
            .ok_or_else(|| DomainError::Internal("failed to enrich workflow business case".into()))
    }

    pub async fn update_case(
        &self,
        case_id: &str,
        payload: BusinessCaseUpdatePayload,
        actor: &str,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        let Some(mut case) = self.repo.find_by_id(case_id).await? else {
            return Ok(None);
        };

        if let Some(case_type) = payload.case_type {
            case.case_type = case_type;
        }
        if let Some(description) = payload.description {
            case.description = description;
        }
        if let Some(context) = payload.context {
            case.context = context;
            case.terminal_metadata = extract_terminal_metadata(&case.context);
        }
        if let Some(status) = payload.status {
            case.status = normalize_business_case_status(&status)?;
        }
        if let Some(stand) = payload.stand {
            case.stand = Some(stand);
        }
        if let Some(gate) = payload.gate {
            case.gate = Some(gate);
        }
        case.updated_by = actor.to_string();

        self.repo.update_case(&case).await?;
        self.refresh_flight_runtime_projection(&case.flight_id).await;
        let refreshed = self.repo.find_by_id(case_id).await?;
        self.enrich_case_type_name(refreshed, None, None).await
    }

    pub async fn update_case_if_accessible(
        &self,
        case_id: &str,
        payload: BusinessCaseUpdatePayload,
        actor: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        let Some(mut case) = self.repo.find_by_id(case_id).await? else {
            return Ok(None);
        };

        if let Some(case_type) = payload.case_type {
            case.case_type = case_type;
        }
        if let Some(description) = payload.description {
            case.description = description;
        }
        if let Some(context) = payload.context {
            case.context = context;
            case.terminal_metadata = extract_terminal_metadata(&case.context);
        }
        if let Some(status) = payload.status {
            case.status = normalize_business_case_status(&status)?;
        }
        if let Some(stand) = payload.stand {
            case.stand = Some(stand);
        }
        if let Some(gate) = payload.gate {
            case.gate = Some(gate);
        }
        case.updated_by = actor.to_string();

        self.repo.update_case(&case).await?;
        self.refresh_flight_runtime_projection(&case.flight_id).await;
        let refreshed = self.repo.find_by_id(case_id).await?;
        self.enrich_case_type_name(refreshed, viewer_department_id, viewer_department_name)
            .await
    }

    pub async fn apply_workflow_terminal_action(
        &self,
        case_id: &str,
        payload: BusinessCaseTerminalUpdatePayload,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        let Some(mut case) = self.repo.find_by_id(case_id).await? else {
            return Ok(None);
        };

        let now = Utc::now();
        case.status = payload.target_status.trim().to_string();
        case.updated_by = payload.actor.trim().to_string();
        if payload.write_finished_at {
            case.finished_at = Some(now);
        }

        let metadata = BusinessCaseTerminalMetadata {
            timestamp: now,
            operator: payload.actor.trim().to_string(),
            action: payload.action.trim().to_string(),
            target_status: payload.target_status.trim().to_string(),
            reason: payload.reason.as_ref().map(|value| value.trim().to_string()),
            workflow_run_id: payload.workflow_run_id,
            workflow_outcome: payload.workflow_outcome,
            receipt_group_id: payload.receipt_group_id,
        };

        case.log.push(serde_json::json!({
            "timestamp": now.to_rfc3339(),
            "operator": metadata.operator.clone(),
            "action": metadata.action.clone(),
            "target_status": metadata.target_status.clone(),
            "reason": metadata.reason.as_deref().unwrap_or_default(),
            "workflow_run_id": metadata.workflow_run_id.clone(),
            "workflow_outcome": metadata.workflow_outcome.clone(),
            "receipt_group_id": metadata.receipt_group_id.clone(),
        }));
        case.context.insert(
            "workflow_terminal".to_string(),
            serde_json::to_value(&metadata).unwrap_or(serde_json::Value::Null),
        );
        case.terminal_metadata = Some(metadata);

        self.repo.update_case(&case).await?;
        self.refresh_flight_runtime_projection(&case.flight_id).await;
        let refreshed = self.repo.find_by_id(case_id).await?;
        self.enrich_case_type_name(refreshed, None, None).await
    }

    pub async fn apply_workflow_terminal_action_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        case_id: &str,
        payload: BusinessCaseTerminalUpdatePayload,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        let Some(mut case) = self.repo.find_by_id(case_id).await? else {
            return Ok(None);
        };

        let now = Utc::now();
        case.status = payload.target_status.trim().to_string();
        case.updated_by = payload.actor.trim().to_string();
        if payload.write_finished_at {
            case.finished_at = Some(now);
        }

        let metadata = BusinessCaseTerminalMetadata {
            timestamp: now,
            operator: payload.actor.trim().to_string(),
            action: payload.action.trim().to_string(),
            target_status: payload.target_status.trim().to_string(),
            reason: payload.reason.as_ref().map(|value| value.trim().to_string()),
            workflow_run_id: payload.workflow_run_id,
            workflow_outcome: payload.workflow_outcome,
            receipt_group_id: payload.receipt_group_id,
        };

        case.log.push(serde_json::json!({
            "timestamp": now.to_rfc3339(),
            "operator": metadata.operator.clone(),
            "action": metadata.action.clone(),
            "target_status": metadata.target_status.clone(),
            "reason": metadata.reason.as_deref().unwrap_or_default(),
            "workflow_run_id": metadata.workflow_run_id.clone(),
            "workflow_outcome": metadata.workflow_outcome.clone(),
            "receipt_group_id": metadata.receipt_group_id.clone(),
        }));
        case.context.insert(
            "workflow_terminal".to_string(),
            serde_json::to_value(&metadata).unwrap_or(serde_json::Value::Null),
        );
        case.terminal_metadata = Some(metadata);

        let tx_repo = self.tx_repo.as_ref().ok_or_else(|| {
            DomainError::Internal("BusinessCaseService transactional repository is not configured".to_string())
        })?;
        tx_repo.update_case_in_tx(tx, &case).await?;
        let refreshed = self.repo.find_by_id(case_id).await?;
        self.enrich_case_type_name(refreshed, None, None).await
    }

    pub async fn append_case(
        &self,
        case_id: &str,
        content: &str,
        submitted_by: &str,
        submitted_operator_name: Option<String>,
        operator: &str,
        mention_user_ids: Vec<String>,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        let Some(target_case) = self.repo.find_by_id(case_id).await? else {
            return Ok(None);
        };

        let validated_mention_ids = self
            .retain_mentionable(&target_case.flight_id, mention_user_ids)
            .await;

        let mut metadata = serde_json::json!({});
        if !validated_mention_ids.is_empty() {
            metadata["mention_user_ids"] =
                serde_json::to_value(&validated_mention_ids).unwrap_or(serde_json::Value::Null);
        }

        let append = BusinessCaseAppendEntry {
            append_id: ulid::Ulid::new().to_string(),
            case_id: case_id.to_string(),
            content: content.trim().to_string(),
            client_action_id: None,
            submitted_by: submitted_by.trim().to_string(),
            submitted_operator_name: submitted_operator_name.clone(),
            appended_at: Utc::now(),
            metadata,
        };

        self.repo.insert_append(&append).await?;
        self.refresh_flight_runtime_projection(&target_case.flight_id).await;
        let refreshed = self.repo.find_by_id(case_id).await?;

        if let Some(case) = refreshed.as_ref() {
            self.event_publisher
                .publish_appended(case, &append.append_id, operator)
                .await?;

            if !validated_mention_ids.is_empty() {
                if let Some(notif_svc) = self.notification_svc.as_ref() {
                    let title_prefix = if !case.flight_no.is_empty() {
                        format!("[{}]", case.flight_no)
                    } else {
                        "".to_string()
                    };

                    let sender_name = submitted_operator_name.unwrap_or_else(|| submitted_by.to_string());
                    let body_preview = if append.content.chars().count() > 200 {
                        format!("{}...", append.content.chars().take(200).collect::<String>())
                    } else {
                        append.content.clone()
                    };

                    let batch_dto = DispatchBatchNotificationCreate {
                        user_ids: validated_mention_ids,
                        title: format!("{} 业务事项有新追加 @你", title_prefix).trim().to_string(),
                        body: format!("{}: {}", sender_name, body_preview),
                        category: "business_case_mention".to_string(),
                        severity: "info".to_string(),
                        flight_id: Some(case.flight_id.clone()),
                        related_entity_type: Some("business_case".to_string()),
                        related_entity_id: Some(case.case_id.clone()),
                        dispatch_order_id: None,
                        group_id: None,
                        sender_user_id: Some(operator.to_string()),
                        sender_username_snapshot: Some(sender_name),
                        origin_type: "manual".to_string(),
                        receipt_required: false,
                    };

                    let _ = notif_svc.send_batch(batch_dto).await;
                }
            }
        }

        self.enrich_case_type_name(refreshed, None, None).await
    }

    pub async fn append_case_if_accessible(
        &self,
        case_id: &str,
        content: &str,
        submitted_by: &str,
        submitted_operator_name: Option<String>,
        operator: &str,
        mention_user_ids: Vec<String>,
        client_action_id: Option<String>,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
    ) -> Result<Option<BusinessCaseAppendResult>, DomainError> {
        let Some(target_case) = self.repo.find_by_id(case_id).await? else {
            return Ok(None);
        };

        let validated_mention_ids = self
            .retain_mentionable(&target_case.flight_id, mention_user_ids)
            .await;

        let mut metadata = serde_json::json!({});
        if !validated_mention_ids.is_empty() {
            metadata["mention_user_ids"] =
                serde_json::to_value(&validated_mention_ids).unwrap_or(serde_json::Value::Null);
        }

        let append = BusinessCaseAppendEntry {
            append_id: ulid::Ulid::new().to_string(),
            case_id: case_id.to_string(),
            content: content.trim().to_string(),
            client_action_id,
            submitted_by: submitted_by.trim().to_string(),
            submitted_operator_name: submitted_operator_name.clone(),
            appended_at: Utc::now(),
            metadata,
        };

        let (append, inserted) = if append.client_action_id.is_some() {
            self.repo.insert_append_once(&append).await?
        } else {
            (self.repo.insert_append(&append).await?, true)
        };
        if inserted {
            self.refresh_flight_runtime_projection(&target_case.flight_id).await;
        }
        let refreshed = self.repo.find_by_id(case_id).await?;

        if inserted {
            if let Some(case) = refreshed.as_ref() {
                self.event_publisher
                    .publish_appended(case, &append.append_id, operator)
                    .await?;

                if !validated_mention_ids.is_empty() {
                    if let Some(notif_svc) = self.notification_svc.as_ref() {
                        let title_prefix = if !case.flight_no.is_empty() {
                            format!("[{}]", case.flight_no)
                        } else {
                            "".to_string()
                        };

                        let sender_name = submitted_operator_name.unwrap_or_else(|| submitted_by.to_string());
                        let body_preview = if append.content.chars().count() > 200 {
                            format!("{}...", append.content.chars().take(200).collect::<String>())
                        } else {
                            append.content.clone()
                        };

                        let batch_dto = DispatchBatchNotificationCreate {
                            user_ids: validated_mention_ids,
                            title: format!("{} 业务事项有新追加 @你", title_prefix).trim().to_string(),
                            body: format!("{}: {}", sender_name, body_preview),
                            category: "business_case_mention".to_string(),
                            severity: "info".to_string(),
                            flight_id: Some(case.flight_id.clone()),
                            related_entity_type: Some("business_case".to_string()),
                            related_entity_id: Some(case.case_id.clone()),
                            dispatch_order_id: None,
                            group_id: None,
                            sender_user_id: Some(operator.to_string()),
                            sender_username_snapshot: Some(sender_name),
                            origin_type: "manual".to_string(),
                            receipt_required: false,
                        };

                        let _ = notif_svc.send_batch(batch_dto).await;
                    }
                }
            }
        }

        Ok(refreshed.map(|case| BusinessCaseAppendResult { case, append, inserted }))
    }

    pub async fn acknowledge_append(
        &self,
        case_id: &str,
        append_id: &str,
        user_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        let Some(append_entry) = self.repo.find_append_by_id(append_id).await? else {
            return Err(DomainError::NotFound {
                entity_type: "BusinessCaseAppend",
                id: append_id.to_string(),
            });
        };

        if append_entry.case_id != case_id {
            return Err(DomainError::ValidationError("追加记录与业务事项不匹配".into()));
        }

        let mut metadata = append_entry.metadata.clone();
        let mention_ids = metadata
            .get("mention_user_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if !mention_ids.contains(&user_id.to_string()) {
            return Err(DomainError::PermissionDenied("无权确认：当前用户未被@提及".into()));
        }

        let mut acknowledgments = metadata
            .get("acknowledgments")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        if let Some(existing) = acknowledgments.get(user_id) {
            return Ok(serde_json::json!({
                "acknowledged": true,
                "acknowledged_at": existing.get("acknowledged_at"),
                "append_id": append_id,
                "user_id": user_id,
            }));
        }

        let now_iso = Utc::now().to_rfc3339();
        acknowledgments.insert(
            user_id.to_string(),
            serde_json::json!({
                "acknowledged_at": now_iso
            }),
        );

        if let Some(obj) = metadata.as_object_mut() {
            obj.insert(
                "acknowledgments".to_string(),
                serde_json::Value::Object(acknowledgments),
            );
        } else {
            metadata = serde_json::json!({
                "mention_user_ids": mention_ids,
                "acknowledgments": acknowledgments
            });
        }

        self.repo.update_append_metadata(append_id, metadata).await?;
        if let Some(case) = self.repo.find_by_id(case_id).await? {
            self.refresh_flight_runtime_projection(&case.flight_id).await;
        }

        Ok(serde_json::json!({
            "acknowledged": true,
            "acknowledged_at": now_iso,
            "append_id": append_id,
            "user_id": user_id,
        }))
    }

    pub async fn acknowledge_append_if_accessible(
        &self,
        case_id: &str,
        append_id: &str,
        user_id: &str,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
    ) -> Result<Option<serde_json::Value>, DomainError> {
        let Some(case) = self.repo.find_by_id(case_id).await? else {
            return Ok(None);
        };

        if case.case_id != case_id {
            return Ok(None);
        }

        let result = self.acknowledge_append(case_id, append_id, user_id).await?;
        Ok(Some(result))
    }

    pub async fn update_status(&self, case_id: &str, status: &str, actor: &str) -> Result<bool, DomainError> {
        let normalized_status = normalize_business_case_status(status)?;
        let flight_id = self.repo.find_by_id(case_id).await?.map(|case| case.flight_id);
        let updated = self.repo.update_status(case_id, &normalized_status, actor).await?;
        if updated {
            if let Some(flight_id) = flight_id.as_deref() {
                self.refresh_flight_runtime_projection(flight_id).await;
            }
        }
        Ok(updated)
    }

    pub async fn update_status_if_accessible(
        &self,
        case_id: &str,
        status: &str,
        actor: &str,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
    ) -> Result<bool, DomainError> {
        let Some(case) = self.repo.find_by_id(case_id).await? else {
            return Ok(false);
        };

        let normalized_status = normalize_business_case_status(status)?;
        let updated = self.repo.update_status(case_id, &normalized_status, actor).await?;
        if updated {
            self.refresh_flight_runtime_projection(&case.flight_id).await;
        }
        Ok(updated)
    }

    pub async fn delete(&self, case_id: &str) -> Result<bool, DomainError> {
        let flight_id = self.repo.find_by_id(case_id).await?.map(|case| case.flight_id);
        let deleted = self.repo.delete(case_id).await?;
        if deleted {
            if let Some(flight_id) = flight_id.as_deref() {
                self.refresh_flight_runtime_projection(flight_id).await;
            }
        }
        Ok(deleted)
    }

    pub async fn delete_if_accessible(
        &self,
        case_id: &str,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
    ) -> Result<bool, DomainError> {
        let Some(case) = self.repo.find_by_id(case_id).await? else {
            return Ok(false);
        };

        let deleted = self.repo.delete(case_id).await?;
        if deleted {
            self.refresh_flight_runtime_projection(&case.flight_id).await;
        }
        Ok(deleted)
    }
}

fn extract_terminal_metadata(context: &HashMap<String, serde_json::Value>) -> Option<BusinessCaseTerminalMetadata> {
    serde_json::from_value(context.get("workflow_terminal")?.clone()).ok()
}
