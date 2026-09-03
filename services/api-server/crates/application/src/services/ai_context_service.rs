use crate::services::authorization_service::AuthorizationService;
use crate::types::{
    ConcreteAnomalyService, ConcreteBusinessCaseService, ConcreteDispatchQueryService, ConcreteFlightService,
    ConcreteNotificationService, ConcreteTodoService,
};
use fms_domain::models::ai_context_envelope::*;
use fms_domain::ontology::governed::{load_governed_schema, ActionOverlay};
use fms_domain::ports::ai_context_snapshot_repository::{AiContextSnapshotKind, AiContextSnapshotRepository};
use fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository;
use fms_domain::ports::ai_object_policy_repository::{
    AiObjectAccessRequest, AiObjectPolicyRepository, AiObjectPolicySubject,
};
use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;
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
    dispatch_query_service: Option<Arc<ConcreteDispatchQueryService>>,
    anomaly_service: Option<Arc<ConcreteAnomalyService>>,
    business_case_service: Option<Arc<ConcreteBusinessCaseService>>,
    notification_service: Option<Arc<ConcreteNotificationService>>,
    todo_service: Option<Arc<ConcreteTodoService>>,
    object_policy_repository: Option<Arc<dyn AiObjectPolicyRepository + Send + Sync>>,
    snapshot_repository: Option<Arc<dyn AiContextSnapshotRepository>>,
    ontology_repository: Option<Arc<dyn AiOntologyRepository + Send + Sync>>,
    entity_config_repository: Option<Arc<dyn AiEntityConfigRepository + Send + Sync>>,
}

impl AiContextService {
    pub fn new(flight_service: Arc<ConcreteFlightService>) -> Self {
        Self {
            flight_service,
            dispatch_query_service: None,
            anomaly_service: None,
            business_case_service: None,
            notification_service: None,
            todo_service: None,
            object_policy_repository: None,
            snapshot_repository: None,
            ontology_repository: None,
            entity_config_repository: None,
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

    pub fn with_ontology_repository(mut self, repository: Arc<dyn AiOntologyRepository + Send + Sync>) -> Self {
        self.ontology_repository = Some(repository);
        self
    }

    pub fn with_entity_config_repository(
        mut self,
        repository: Arc<dyn AiEntityConfigRepository + Send + Sync>,
    ) -> Self {
        self.entity_config_repository = Some(repository);
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
        self.build_envelope_for_entity(
            user_id,
            roles,
            department_id,
            task_type,
            user_message,
            target_objects,
            None,
        )
        .await
    }

    pub async fn build_envelope_for_entity(
        &self,
        user_id: &str,
        roles: &[String],
        department_id: Option<&str>,
        task_type: &str,
        user_message: &str,
        target_objects: &[(String, String)],
        entity_id: Option<&str>,
    ) -> Result<ContextEnvelope, AiContextError> {
        let overlays = self.load_action_overlays().await?;
        let schema = load_governed_schema(&overlays);
        let allowed_actions = AuthorizationService::allowed_ai_actions_from_schema(&schema, roles);
        let risk_ceiling = self.resolve_entity_risk_ceiling(entity_id).await?;

        let mut objects = Vec::new();
        let mut evidence = Vec::new();

        let mut allowed_object_types: Vec<String> = schema.objects.keys().cloned().collect();
        allowed_object_types.retain(|object_type| {
            allowed_actions
                .iter()
                .any(|action| action.split('.').next() == Some(object_type.as_str()))
                || can_read_context_object_type(roles, object_type)
        });
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
                risk_ceiling: resolved_ai_risk_ceiling(risk_ceiling.as_deref()),
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

    async fn load_action_overlays(&self) -> Result<Vec<ActionOverlay>, AiContextError> {
        let Some(repository) = &self.ontology_repository else {
            return Ok(Vec::new());
        };
        match repository.load_action_overlays().await {
            Ok(overlays) => Ok(overlays),
            Err(error) => {
                tracing::warn!("failed to load AI ontology overlays for envelope: {}", error);
                Ok(Vec::new())
            }
        }
    }

    async fn resolve_entity_risk_ceiling(&self, entity_id: Option<&str>) -> Result<Option<String>, AiContextError> {
        let Some(entity_id) = entity_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let Some(repository) = &self.entity_config_repository else {
            return Ok(None);
        };
        let record = repository
            .find_by_id(entity_id)
            .await
            .map_err(|error| AiContextError::Internal(error.to_string()))?;
        let Some(record) = record else {
            return Ok(None);
        };
        let ceiling = record
            .config
            .get("context_policy")
            .and_then(|policy| policy.get("risk_ceiling"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        Ok(ceiling)
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
            "Stand" => self.load_snapshot(AiContextSnapshotKind::Stand, obj_id).await,
            "Team" => self.load_snapshot(AiContextSnapshotKind::Team, obj_id).await,
            "Equipment" => self.load_snapshot(AiContextSnapshotKind::Equipment, obj_id).await,
            "Terminal" => self.load_snapshot(AiContextSnapshotKind::Terminal, obj_id).await,
            "Gate" => self.load_snapshot(AiContextSnapshotKind::Gate, obj_id).await,
            "BaggageCarousel" => self.load_snapshot(AiContextSnapshotKind::BaggageCarousel, obj_id).await,
            "StandOccupation" => self.load_snapshot(AiContextSnapshotKind::StandOccupation, obj_id).await,
            "GateAssignment" => self.load_snapshot(AiContextSnapshotKind::GateAssignment, obj_id).await,
            "CarouselAssignment" => {
                self.load_snapshot(AiContextSnapshotKind::CarouselAssignment, obj_id)
                    .await
            }
            "Department" => self.load_snapshot(AiContextSnapshotKind::Department, obj_id).await,
            "EquipmentType" => self.load_snapshot(AiContextSnapshotKind::EquipmentType, obj_id).await,
            "Aircraft" => self.load_snapshot(AiContextSnapshotKind::Aircraft, obj_id).await,
            "TurnaroundLink" => self.load_snapshot(AiContextSnapshotKind::TurnaroundLink, obj_id).await,
            "Qualification" => self.load_snapshot(AiContextSnapshotKind::Qualification, obj_id).await,
            "TaskType" => self.load_snapshot(AiContextSnapshotKind::TaskType, obj_id).await,
            "Personnel" => self.load_snapshot(AiContextSnapshotKind::Personnel, obj_id).await,
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

pub fn resolved_ai_risk_ceiling(configured: Option<&str>) -> String {
    let from_config = configured.map(str::trim).filter(|value| !value.is_empty());
    let from_env = std::env::var("FMS_AI_ONTOLOGY_RISK_CEILING")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let raw = from_config.or(from_env.as_deref()).unwrap_or("medium");
    match raw.to_ascii_lowercase().as_str() {
        "low" | "medium" | "high" | "critical" => raw.to_ascii_lowercase(),
        _ => "medium".to_string(),
    }
}

fn can_read_context_object_type(user_permissions: &[String], object_type: &str) -> bool {
    let required = match object_type {
        "Flight" => &["flight:read", "flight.read", "flight:write"][..],
        "DispatchOrder" => &[
            "dispatch:read",
            "dispatch:write",
            "dispatch:manage",
            "dispatch_order.read",
            "dispatch_order.update",
        ][..],
        "Stand" | "Team" | "Equipment" | "Terminal" | "Gate" | "BaggageCarousel" | "Department" | "EquipmentType"
        | "Qualification" | "TaskType" => &[
            "dispatch:read",
            "dispatch:write",
            "dispatch:manage",
            "dispatch_catalog.read",
            "ontology:manage",
            "ontology.manage",
        ][..],
        "StandOccupation" => &["ontology:stand.manage", "ontology.stand.manage", "ontology:manage"][..],
        "GateAssignment" => &["ontology:gate.manage", "ontology.gate.manage", "ontology:manage"][..],
        "CarouselAssignment" => &[
            "ontology:carousel.manage",
            "ontology.carousel.manage",
            "ontology:manage",
        ][..],
        "Personnel" => &[
            "ontology:personnel.manage",
            "ontology.personnel.manage",
            "dispatch:manage",
            "ontology:manage",
        ][..],
        "Aircraft" | "TurnaroundLink" => &[
            "ontology:aircraft.manage",
            "ontology.aircraft.manage",
            "ontology:turnaround.create",
            "ontology:turnaround.manage",
            "ontology:manage",
            "ontology.read",
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
        AiContextService::new(Arc::new(crate::services::flight_service::FlightService::new(Arc::new(
            EmptyFlightRepository,
        ))))
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

    #[tokio::test]
    async fn envelope_object_types_follow_schema_not_exited_objects() {
        let envelope = service()
            .build_envelope("admin", &["*".to_string()], None, "nl_query", "hello", &[])
            .await
            .unwrap();
        assert!(envelope
            .ontology
            .allowed_object_types
            .contains(&"Personnel".to_string()));
        assert!(envelope.ontology.allowed_object_types.contains(&"Terminal".to_string()));
        assert!(envelope
            .ontology
            .allowed_object_types
            .contains(&"CarouselAssignment".to_string()));
        assert!(!envelope
            .ontology
            .allowed_object_types
            .iter()
            .any(|item| item == "FlightLeg"));
        assert!(!envelope.ontology.allowed_object_types.iter().any(|item| item == "Todo"));
        assert!(!envelope
            .ontology
            .allowed_object_types
            .iter()
            .any(|item| item == "Notification"));
        assert!(!envelope
            .ontology
            .allowed_object_types
            .iter()
            .any(|item| item == "WorkflowRun"));
        assert!(envelope
            .ontology
            .allowed_actions
            .contains(&"Flight.add_note".to_string()));
        assert!(envelope
            .ontology
            .allowed_actions
            .contains(&"DispatchOrder.assign_slot".to_string()));
        assert!(!envelope
            .ontology
            .allowed_actions
            .iter()
            .any(|item| item == "Flight.change_stand"));
        assert_eq!(envelope.ontology.risk_ceiling, "medium");
    }

    struct OverlayOntologyRepository {
        overlays: Vec<ActionOverlay>,
    }

    #[async_trait]
    impl AiOntologyRepository for OverlayOntologyRepository {
        async fn load_action_overlays(
            &self,
        ) -> Result<Vec<ActionOverlay>, fms_domain::ports::ai_ontology_repository::AiOntologyRepositoryError> {
            Ok(self.overlays.clone())
        }
        async fn save_action_overlay(
            &self,
            _overlay: &ActionOverlay,
        ) -> Result<(), fms_domain::ports::ai_ontology_repository::AiOntologyRepositoryError> {
            Ok(())
        }
        async fn delete_action_overlay(
            &self,
            _object: &str,
            _action: &str,
        ) -> Result<(), fms_domain::ports::ai_ontology_repository::AiOntologyRepositoryError> {
            Ok(())
        }
        async fn count_active_objects(
            &self,
        ) -> Result<i64, fms_domain::ports::ai_ontology_repository::AiOntologyRepositoryError> {
            Ok(0)
        }
        async fn count_active_write_actions(
            &self,
        ) -> Result<i64, fms_domain::ports::ai_ontology_repository::AiOntologyRepositoryError> {
            Ok(0)
        }
    }

    struct CeilingEntityConfigRepository {
        ceiling: String,
    }

    #[async_trait]
    impl AiEntityConfigRepository for CeilingEntityConfigRepository {
        async fn find_all(
            &self,
        ) -> Result<Vec<fms_domain::models::ai_entity_config::AiEntityConfigRecord>, DomainError> {
            Ok(Vec::new())
        }
        async fn find_by_id(
            &self,
            id: &str,
        ) -> Result<Option<fms_domain::models::ai_entity_config::AiEntityConfigRecord>, DomainError> {
            if id != "ops-entity" {
                return Ok(None);
            }
            Ok(Some(fms_domain::models::ai_entity_config::AiEntityConfigRecord {
                id: id.to_string(),
                config: serde_json::json!({ "context_policy": { "risk_ceiling": self.ceiling } }),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }))
        }
        async fn save(
            &self,
            _id: &str,
            _config: &serde_json::Value,
        ) -> Result<fms_domain::models::ai_entity_config::AiEntityConfigRecord, DomainError> {
            Err(DomainError::Internal("not implemented".into()))
        }
        async fn delete(&self, _id: &str) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn envelope_omits_overlay_disabled_actions() {
        let repository = Arc::new(OverlayOntologyRepository {
            overlays: vec![ActionOverlay {
                object: "Flight".to_string(),
                action: "add_note".to_string(),
                is_active: Some(false),
                risk: None,
                requires_approval: None,
            }],
        });
        let envelope = service()
            .with_ontology_repository(repository)
            .build_envelope("admin", &["*".to_string()], None, "nl_query", "hello", &[])
            .await
            .unwrap();
        assert!(!envelope
            .ontology
            .allowed_actions
            .iter()
            .any(|item| item == "Flight.add_note"));
        assert!(envelope
            .ontology
            .allowed_actions
            .contains(&"Flight.update_status".to_string()));
    }

    #[tokio::test]
    async fn envelope_risk_ceiling_reads_entity_config() {
        let repository = Arc::new(CeilingEntityConfigRepository {
            ceiling: "high".to_string(),
        });
        let envelope = service()
            .with_entity_config_repository(repository)
            .build_envelope_for_entity(
                "admin",
                &["*".to_string()],
                None,
                "nl_query",
                "hello",
                &[],
                Some("ops-entity"),
            )
            .await
            .unwrap();
        assert_eq!(envelope.ontology.risk_ceiling, "high");
    }
}
