use crate::services::authorization_service::AuthorizationService;
use crate::types::{
    ConcreteAnomalyService, ConcreteBusinessCaseService, ConcreteDispatchQueryService, ConcreteFlightService,
    ConcreteNotificationService, ConcreteTodoService,
};
use fms_domain::models::ai_context_envelope::*;
use fms_domain::ports::ai_context_snapshot_repository::{AiContextSnapshotKind, AiContextSnapshotRepository};
use fms_domain::ports::ai_object_policy_repository::{
    AiObjectAccessRequest, AiObjectPolicyRepository, AiObjectPolicySubject,
};
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiContextError {
    #[error("Internal error: {0}")]
    Internal(String),
}

pub struct AiContextService {
    flight_service: Arc<ConcreteFlightService>,
    authorization_service: Arc<AuthorizationService>,
    dispatch_query_service: Option<Arc<ConcreteDispatchQueryService>>,
    anomaly_service: Option<Arc<ConcreteAnomalyService>>,
    business_case_service: Option<Arc<ConcreteBusinessCaseService>>,
    notification_service: Option<Arc<ConcreteNotificationService>>,
    todo_service: Option<Arc<ConcreteTodoService>>,
    object_policy_repository: Option<Arc<dyn AiObjectPolicyRepository + Send + Sync>>,
    snapshot_repository: Option<Arc<dyn AiContextSnapshotRepository>>,
}

impl AiContextService {
    pub fn new(flight_service: Arc<ConcreteFlightService>, authorization_service: Arc<AuthorizationService>) -> Self {
        Self {
            flight_service,
            authorization_service,
            dispatch_query_service: None,
            anomaly_service: None,
            business_case_service: None,
            notification_service: None,
            todo_service: None,
            object_policy_repository: None,
            snapshot_repository: None,
        }
    }

    pub fn with_dispatch_query_service(mut self, service: Arc<ConcreteDispatchQueryService>) -> Self {
        self.dispatch_query_service = Some(service);
        self
    }

    pub fn with_anomaly_service(mut self, service: Arc<ConcreteAnomalyService>) -> Self {
        self.anomaly_service = Some(service);
        self
    }

    pub fn with_business_case_service(mut self, service: Arc<ConcreteBusinessCaseService>) -> Self {
        self.business_case_service = Some(service);
        self
    }

    pub fn with_notification_service(mut self, service: Arc<ConcreteNotificationService>) -> Self {
        self.notification_service = Some(service);
        self
    }

    pub fn with_todo_service(mut self, service: Arc<ConcreteTodoService>) -> Self {
        self.todo_service = Some(service);
        self
    }

    pub fn with_object_policy_repository(
        mut self,
        repository: Arc<dyn AiObjectPolicyRepository + Send + Sync>,
    ) -> Self {
        self.object_policy_repository = Some(repository);
        self
    }

    pub fn with_snapshot_repository(mut self, repository: Arc<dyn AiContextSnapshotRepository>) -> Self {
        self.snapshot_repository = Some(repository);
        self
    }

    pub async fn build_envelope(
        &self,
        user_id: &str,
        roles: &[String],
        department_id: Option<&str>,
        task_type: &str,
        user_message: &str,
        target_objects: &[(String, String)],
    ) -> Result<ContextEnvelope, AiContextError> {
        let allowed_actions = self
            .authorization_service
            .get_allowed_ai_actions(user_id, roles)
            .await
            .map_err(|e| AiContextError::Internal(e.to_string()))?;

        let mut objects = Vec::new();
        let mut evidence = Vec::new();

        let mut allowed_object_types: Vec<String> = allowed_actions
            .iter()
            .map(|a| a.split('.').next().unwrap_or("").to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        for object_type in [
            "Flight",
            "FlightLeg",
            "DispatchOrder",
            "Stand",
            "Team",
            "Equipment",
            "Anomaly",
            "BusinessCase",
            "WorkflowRun",
            "Notification",
            "Todo",
        ] {
            if can_read_context_object_type(roles, object_type)
                && !allowed_object_types.iter().any(|allowed| allowed == object_type)
            {
                allowed_object_types.push(object_type.to_string());
            }
        }
        allowed_object_types.sort();
        let allowed_object_type_set: HashSet<&str> = allowed_object_types.iter().map(String::as_str).collect();

        for (obj_type, obj_id) in target_objects {
            if !allowed_object_type_set.contains(obj_type.as_str()) {
                continue;
            }
            let data = self.load_object_data(obj_type, obj_id).await?;
            if !self
                .object_policy_allows_read(user_id, roles, department_id, obj_type, obj_id, &data)
                .await?
            {
                continue;
            }
            objects.push(EnvelopeObject {
                object_type: obj_type.clone(),
                object_id: obj_id.clone(),
                version: None,
                data,
            });
            evidence.push(EnvelopeEvidence {
                source: format!("ai_query.v_{}s", obj_type.to_lowercase()),
                object_type: obj_type.clone(),
                object_id: obj_id.clone(),
                retrieved_at: Some(chrono::Utc::now().to_rfc3339()),
            });
        }

        Ok(ContextEnvelope {
            contract_version: "ai-runtime.v1".to_string(),
            job_id: format!("job_{}", uuid::Uuid::new_v4()),
            run_id: format!("run_{}", uuid::Uuid::new_v4()),
            correlation_id: format!("req_{}", uuid::Uuid::new_v4()),
            requester: EnvelopeRequester {
                user_id: user_id.to_string(),
                roles: roles.to_vec(),
                department_id: department_id.map(str::to_string),
                permission_version: None,
            },
            ontology: EnvelopeOntology {
                version: "flight-ops.v1".to_string(),
                allowed_object_types,
                allowed_actions: allowed_actions.iter().map(|s| s.to_string()).collect(),
                risk_ceiling: "medium".to_string(),
            },
            context: EnvelopeContext {
                objects,
                relations: vec![],
                evidence,
                limits: EnvelopeLimits {
                    max_objects: 100,
                    max_tokens: 12000,
                    redaction: "standard".to_string(),
                },
            },
            task: EnvelopeTask {
                task_type: task_type.to_string(),
                user_message: user_message.to_string(),
            },
        })
    }

    async fn load_object_data(&self, obj_type: &str, obj_id: &str) -> Result<serde_json::Value, AiContextError> {
        match obj_type {
            "Flight" => {
                let flight = self
                    .flight_service
                    .get_flight(obj_id)
                    .await
                    .map_err(|e| AiContextError::Internal(e.to_string()))?
                    .ok_or_else(|| AiContextError::Internal(format!("Flight not found: {obj_id}")))?;
                Ok(serde_json::to_value(flight).unwrap_or_default())
            }
            "FlightLeg" => self.load_snapshot(AiContextSnapshotKind::FlightLeg, obj_id).await,
            "DispatchOrder" => {
                let service = self
                    .dispatch_query_service
                    .as_ref()
                    .ok_or_else(|| AiContextError::Internal("DispatchQueryService unavailable".into()))?;
                let order = service
                    .get_order(obj_id, true, None)
                    .await
                    .map_err(|e| AiContextError::Internal(e.to_string()))?
                    .ok_or_else(|| AiContextError::Internal(format!("DispatchOrder not found: {obj_id}")))?;
                Ok(serde_json::to_value(order).unwrap_or_default())
            }
            "Stand" => self.load_snapshot(AiContextSnapshotKind::Stand, obj_id).await,
            "Team" => self.load_snapshot(AiContextSnapshotKind::Team, obj_id).await,
            "Equipment" => self.load_snapshot(AiContextSnapshotKind::Equipment, obj_id).await,
            "Anomaly" => {
                let service = self
                    .anomaly_service
                    .as_ref()
                    .ok_or_else(|| AiContextError::Internal("AnomalyService unavailable".into()))?;
                let anomaly = service
                    .get_anomaly(obj_id)
                    .await
                    .map_err(|e| AiContextError::Internal(e.to_string()))?
                    .ok_or_else(|| AiContextError::Internal(format!("Anomaly not found: {obj_id}")))?;
                Ok(serde_json::to_value(anomaly).unwrap_or_default())
            }
            "BusinessCase" => {
                let service = self
                    .business_case_service
                    .as_ref()
                    .ok_or_else(|| AiContextError::Internal("BusinessCaseService unavailable".into()))?;
                let case = service
                    .get(obj_id)
                    .await
                    .map_err(|e| AiContextError::Internal(e.to_string()))?
                    .ok_or_else(|| AiContextError::Internal(format!("BusinessCase not found: {obj_id}")))?;
                Ok(serde_json::to_value(case).unwrap_or_default())
            }
            "WorkflowRun" => self.load_snapshot(AiContextSnapshotKind::WorkflowRun, obj_id).await,
            "Notification" => {
                let service = self
                    .notification_service
                    .as_ref()
                    .ok_or_else(|| AiContextError::Internal("NotificationService unavailable".into()))?;
                let notification = service
                    .get_notification(obj_id, "")
                    .await
                    .map_err(|e| AiContextError::Internal(e.to_string()))?;
                if let Some(notification) = notification {
                    Ok(serde_json::to_value(notification).unwrap_or_default())
                } else {
                    self.load_snapshot(AiContextSnapshotKind::Notification, obj_id).await
                }
            }
            "Todo" => {
                let service = self
                    .todo_service
                    .as_ref()
                    .ok_or_else(|| AiContextError::Internal("TodoService unavailable".into()))?;
                let todo = service
                    .get_todo(obj_id)
                    .await
                    .map_err(|e| AiContextError::Internal(e.to_string()))?
                    .ok_or_else(|| AiContextError::Internal(format!("Todo not found: {obj_id}")))?;
                Ok(serde_json::to_value(todo).unwrap_or_default())
            }
            _ => Err(AiContextError::Internal(format!(
                "unsupported context object type: {obj_type}"
            ))),
        }
    }

    async fn load_snapshot(
        &self,
        kind: AiContextSnapshotKind,
        obj_id: &str,
    ) -> Result<serde_json::Value, AiContextError> {
        let repository = self
            .snapshot_repository
            .as_ref()
            .ok_or_else(|| AiContextError::Internal("context snapshot repository unavailable".into()))?;
        repository
            .load_snapshot(kind, obj_id)
            .await
            .map_err(|error| AiContextError::Internal(error.to_string()))?
            .ok_or_else(|| AiContextError::Internal(format!("{kind:?} snapshot not found: {obj_id}")))
    }

    async fn object_policy_allows_read(
        &self,
        user_id: &str,
        permissions: &[String],
        department_id: Option<&str>,
        object_type: &str,
        object_id: &str,
        object_snapshot: &serde_json::Value,
    ) -> Result<bool, AiContextError> {
        let Some(repository) = &self.object_policy_repository else {
            return Ok(true);
        };

        let decision = repository
            .evaluate_access(&AiObjectAccessRequest {
                subject: object_policy_subject(user_id, permissions, department_id),
                object_type: object_type.to_string(),
                object_id: Some(object_id.to_string()),
                permission: "read".to_string(),
                object_snapshot: Some(object_snapshot.clone()),
            })
            .await
            .map_err(|e| AiContextError::Internal(e.to_string()))?;

        Ok(!decision.is_denied())
    }
}

fn object_policy_subject(user_id: &str, permissions: &[String], department_id: Option<&str>) -> AiObjectPolicySubject {
    let mut subject = AiObjectPolicySubject::new(user_id, permissions.to_vec());
    subject.department_id = department_id.map(str::to_string);
    subject
}

fn can_read_context_object_type(user_permissions: &[String], object_type: &str) -> bool {
    let required = match object_type {
        "Flight" | "FlightLeg" => &["flight:read", "flight.read", "flight:write"][..],
        "DispatchOrder" => &[
            "dispatch:read",
            "dispatch:write",
            "dispatch:manage",
            "dispatch_order.read",
            "dispatch_order.update",
        ][..],
        "Stand" | "Team" | "Equipment" => &[
            "dispatch:read",
            "dispatch:write",
            "dispatch:manage",
            "dispatch_catalog.read",
        ][..],
        "Anomaly" => &["anomaly:read", "anomaly:write"][..],
        "BusinessCase" => &[
            "business_case:read",
            "business_case:create",
            "business_case:update",
            "business_case.read",
            "business_case.create",
            "business_case.update",
        ][..],
        "WorkflowRun" => &["workflow_run.read", "workflow_run.act"][..],
        "Notification" => &["notification:read", "notification.read", "notification:send"][..],
        "Todo" => &["todo:read", "todo:write", "todo.write"][..],
        _ => &[][..],
    };

    user_permissions.iter().any(|permission| {
        permission == "*"
            || required.iter().any(|candidate| {
                permission == candidate
                    || candidate
                        .split_once(':')
                        .map(|(resource, _)| permission == &format!("{resource}:*"))
                        .unwrap_or(false)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::NaiveDate;
    use fms_domain::error::DomainError;
    use fms_domain::models::flight::Flight;
    use fms_domain::ports::flight_repository::{FlightRepository, FlightSearchCriteria, FlightUpdatePatch};
    use std::sync::Mutex;

    struct EmptyFlightRepository;

    #[async_trait]
    impl FlightRepository for EmptyFlightRepository {
        async fn find_by_id(&self, _flight_id: &str) -> Result<Option<Flight>, DomainError> {
            Ok(None)
        }
        async fn find_all(&self, _limit: i64, _offset: i64) -> Result<Vec<Flight>, DomainError> {
            Ok(Vec::new())
        }
        async fn find_by_date(&self, _date: NaiveDate) -> Result<Vec<Flight>, DomainError> {
            Ok(Vec::new())
        }
        async fn find_by_flight_number(&self, _flight_no: &str) -> Result<Vec<Flight>, DomainError> {
            Ok(Vec::new())
        }
        async fn find_by_status(&self, _status: i32, _limit: i64, _offset: i64) -> Result<Vec<Flight>, DomainError> {
            Ok(Vec::new())
        }
        async fn save(&self, _flight: &Flight) -> Result<(), DomainError> {
            Ok(())
        }
        async fn update_partial(
            &self,
            _flight_id: &str,
            _patch: &FlightUpdatePatch,
        ) -> Result<Option<Flight>, DomainError> {
            Ok(None)
        }
        async fn save_batch(&self, _flights: &[Flight]) -> Result<usize, DomainError> {
            Ok(0)
        }
        async fn update_status(&self, _flight_id: &str, _status: i32) -> Result<bool, DomainError> {
            Ok(false)
        }
        async fn delete(&self, _flight_id: &str) -> Result<bool, DomainError> {
            Ok(false)
        }
        async fn search(
            &self,
            _criteria: &FlightSearchCriteria,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<Flight>, DomainError> {
            Ok(Vec::new())
        }
        async fn count_by_date(&self, _date: NaiveDate) -> Result<i64, DomainError> {
            Ok(0)
        }
    }

    struct RecordingSnapshotRepository {
        requested: Mutex<Vec<AiContextSnapshotKind>>,
    }

    #[async_trait]
    impl AiContextSnapshotRepository for RecordingSnapshotRepository {
        async fn load_snapshot(
            &self,
            kind: AiContextSnapshotKind,
            _object_id: &str,
        ) -> Result<Option<serde_json::Value>, DomainError> {
            self.requested.lock().unwrap().push(kind);
            Ok(Some(serde_json::json!({ "id": "stand-1" })))
        }
    }

    fn service() -> AiContextService {
        AiContextService::new(
            Arc::new(crate::services::flight_service::FlightService::new(Arc::new(
                EmptyFlightRepository,
            ))),
            Arc::new(AuthorizationService),
        )
    }

    #[tokio::test]
    async fn snapshot_backed_objects_fail_closed_without_repository() {
        let error = service().load_object_data("Stand", "stand-1").await.unwrap_err();
        assert!(error.to_string().contains("context snapshot repository unavailable"));
    }

    #[tokio::test]
    async fn snapshot_backed_objects_use_semantic_kind() {
        let repository = Arc::new(RecordingSnapshotRepository {
            requested: Mutex::new(Vec::new()),
        });
        let service = service().with_snapshot_repository(repository.clone());

        let value = service.load_object_data("Stand", "stand-1").await.unwrap();

        assert_eq!(value["id"], "stand-1");
        assert_eq!(
            repository.requested.lock().unwrap().as_slice(),
            &[AiContextSnapshotKind::Stand]
        );
    }
}
