use std::sync::Arc;

use chrono::{DateTime, Duration};
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::ai_copilot::AiCopilotOperationalMetrics;
use fms_domain::models::business_case::FlightBusinessCase;
use fms_domain::models::flight::Flight;
use fms_domain::ports::ai_copilot_repository::AiCopilotBusinessCaseBatchRepository;

use super::config::{
    parse_ai_extraction_config, AiFieldConfig, AiFlightMatchingConfig, AiLegBindingConfig,
    BusinessCaseAiExtractionConfig, BusinessCaseProperties, CaseBindingPolicy, CaseDuplicatePolicy,
    CaseFlightMatchPolicy, CopilotCaseTypeCatalogEntry, PreparedCommitAction,
};
use super::helpers::*;
use super::*;
use crate::services::business_case_service::{BusinessCaseServiceOps, BusinessCaseTerminalUpdatePayload};
use crate::services::business_case_workflow_service::BusinessCaseWorkflowService;
use fms_domain::models::ai_copilot::{
    AiCopilotBatchStatusMetrics, AiCopilotOperationalError, AiCopilotWorkflowDispatchMetrics,
};
use fms_domain::ports::flight_repository::{FlightRepository, FlightSearchCriteria, FlightUpdatePatch};

pub(super) mod batch;
pub(super) mod commit;
pub(super) mod saga;

// ---- Fake FlightRepository for deterministic E2E tests ----

#[derive(Default)]
pub(super) struct FakeFlightRepository {
    flights: std::sync::Mutex<HashMap<String, Flight>>,
}

#[async_trait::async_trait]
impl FlightRepository for FakeFlightRepository {
    async fn find_by_id(&self, flight_id: &str) -> Result<Option<Flight>, DomainError> {
        Ok(self.flights.lock().unwrap().get(flight_id).cloned())
    }
    async fn find_all(&self, limit: i64, offset: i64) -> Result<Vec<Flight>, DomainError> {
        let map = self.flights.lock().unwrap();
        let mut list: Vec<Flight> = map.values().cloned().collect();
        list.sort_by(|a, b| b.flight_id.0.cmp(&a.flight_id.0));
        Ok(list.into_iter().skip(offset as usize).take(limit as usize).collect())
    }
    async fn find_by_date(&self, _date: chrono::NaiveDate) -> Result<Vec<Flight>, DomainError> {
        unimplemented!("fake: find_by_date not needed")
    }
    async fn find_by_flight_number(&self, flight_no: &str) -> Result<Vec<Flight>, DomainError> {
        let term = flight_no.to_uppercase();
        let map = self.flights.lock().unwrap();
        Ok(map
            .values()
            .filter(|f| {
                f.flight_number
                    .as_ref()
                    .map(|n| n.0.to_uppercase().contains(&term))
                    .unwrap_or(false)
                    || f.inbound_leg
                        .as_ref()
                        .map(|l| l.flight_no.to_uppercase().contains(&term))
                        .unwrap_or(false)
                    || f.outbound_leg
                        .as_ref()
                        .map(|l| l.flight_no.to_uppercase().contains(&term))
                        .unwrap_or(false)
            })
            .cloned()
            .collect())
    }
    async fn find_by_status(&self, _status: i32, _limit: i64, _offset: i64) -> Result<Vec<Flight>, DomainError> {
        unimplemented!("fake: find_by_status not needed")
    }
    async fn save(&self, flight: &Flight) -> Result<(), DomainError> {
        self.flights
            .lock()
            .unwrap()
            .insert(flight.flight_id.0.clone(), flight.clone());
        Ok(())
    }
    async fn update_partial(
        &self,
        _flight_id: &str,
        _patch: &FlightUpdatePatch,
    ) -> Result<Option<Flight>, DomainError> {
        unimplemented!("fake: update_partial not needed")
    }
    async fn save_batch(&self, _flights: &[Flight]) -> Result<usize, DomainError> {
        unimplemented!("fake: save_batch not needed")
    }
    async fn update_status(&self, _flight_id: &str, _status: i32) -> Result<bool, DomainError> {
        unimplemented!("fake: update_status not needed")
    }
    async fn delete(&self, _flight_id: &str) -> Result<bool, DomainError> {
        unimplemented!("fake: delete not needed")
    }
    async fn search(
        &self,
        _criteria: &FlightSearchCriteria,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<Flight>, DomainError> {
        unimplemented!("fake: search not needed")
    }
    async fn count_by_date(&self, _date: chrono::NaiveDate) -> Result<i64, DomainError> {
        unimplemented!("fake: count_by_date not needed")
    }
}

// ---- Fake BusinessCaseServiceOps for deterministic E2E tests ----

#[derive(Default)]
pub(super) struct FakeBusinessCaseServiceOps {
    pub cases: std::sync::Mutex<HashMap<String, FlightBusinessCase>>,
}

#[async_trait::async_trait]
impl BusinessCaseServiceOps for FakeBusinessCaseServiceOps {
    async fn get(&self, case_id: &str) -> Result<Option<FlightBusinessCase>, DomainError> {
        Ok(self.cases.lock().unwrap().get(case_id).cloned())
    }
    async fn get_accessible(
        &self,
        case_id: &str,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        Ok(self.cases.lock().unwrap().get(case_id).cloned())
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
        _visibility_scope: VisibilityScope,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
    ) -> Result<FlightBusinessCase, DomainError> {
        let case = FlightBusinessCase {
            case_id: ulid::Ulid::new().to_string(),
            case_type: case_type.to_string(),
            case_type_name: None,
            flight_id: flight_id.to_string(),
            flight_no: flight_no.to_string(),
            created_at: Utc::now(),
            created_by: actor.to_string(),
            updated_by: actor.to_string(),
            description: description.to_string(),
            context,
            status: status.unwrap_or("PENDING").to_string(),
            stand: None,
            gate: None,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            finished_at: None,
            cancelled_at: None,
            log: vec![],
            workflow_receipt: None,
            terminal_metadata: None,
            append_count: 0,
            latest_append: None,
            append_entries: vec![],
        };
        self.cases.lock().unwrap().insert(case.case_id.clone(), case.clone());
        Ok(case)
    }
    async fn create_workflow_case_for_viewer(
        &self,
        flight_id: &str,
        flight_no: &str,
        case_type: &str,
        description: &str,
        actor: &str,
        context: HashMap<String, serde_json::Value>,
        _stand: Option<String>,
        _gate: Option<String>,
        _visibility_scope: VisibilityScope,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
    ) -> Result<FlightBusinessCase, DomainError> {
        let case = FlightBusinessCase {
            case_id: ulid::Ulid::new().to_string(),
            case_type: case_type.to_string(),
            case_type_name: None,
            flight_id: flight_id.to_string(),
            flight_no: flight_no.to_string(),
            created_at: Utc::now(),
            created_by: actor.to_string(),
            updated_by: actor.to_string(),
            description: description.to_string(),
            context,
            status: "PENDING".to_string(),
            stand: None,
            gate: None,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            finished_at: None,
            cancelled_at: None,
            log: vec![],
            workflow_receipt: None,
            terminal_metadata: None,
            append_count: 0,
            latest_append: None,
            append_entries: vec![],
        };
        self.cases.lock().unwrap().insert(case.case_id.clone(), case.clone());
        Ok(case)
    }
    async fn delete(&self, case_id: &str) -> Result<bool, DomainError> {
        Ok(self.cases.lock().unwrap().remove(case_id).is_some())
    }
    async fn get_by_flight_for_viewer(
        &self,
        flight_id: &str,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .cases
            .lock()
            .unwrap()
            .values()
            .filter(|c| c.flight_id == flight_id)
            .cloned()
            .collect())
    }
    async fn find_by_copilot_batch_action(
        &self,
        batch_id: &str,
        action_id: &str,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        let mut cases = self
            .cases
            .lock()
            .unwrap()
            .values()
            .filter(|case| {
                case.context
                    .get("source")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value == "ai_copilot_voice")
                    && case
                        .context
                        .get("copilot_batch_id")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value == batch_id)
                    && case
                        .context
                        .get("copilot_action_id")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value == action_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        cases.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.case_id.cmp(&right.case_id))
        });
        Ok(cases.into_iter().next())
    }
    async fn list_by_copilot_batch(&self, batch_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let mut cases = self
            .cases
            .lock()
            .unwrap()
            .values()
            .filter(|case| {
                case.context
                    .get("source")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value == "ai_copilot_voice")
                    && case
                        .context
                        .get("copilot_batch_id")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value == batch_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        cases.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.case_id.cmp(&right.case_id))
        });
        Ok(cases)
    }
    async fn apply_workflow_terminal_action(
        &self,
        _case_id: &str,
        _payload: BusinessCaseTerminalUpdatePayload,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        Ok(None)
    }
}

pub(super) fn extract_error_stage(value: &Value) -> Option<String> {
    value.get("stage").and_then(Value::as_str).map(str::to_string)
}

pub(super) fn extract_error_message(value: &Value) -> Option<String> {
    value.get("message").and_then(Value::as_str).map(str::to_string)
}

#[test]
fn parse_llm_payload_accepts_code_fence() {
    let parsed = parse_llm_payload(
        r#"```json
{"summary":"s","actions":[{"case_type":"gate_baggage_check","flight_number_raw":"7714","remarks":"座位号 23A","fields":{"seat_no":"23A"},"confidence":0.9}]}
```"#,
    )
    .unwrap();
    assert_eq!(parsed.actions.len(), 1);
    assert_eq!(parsed.actions[0].flight_number_raw, "7714");
}

#[test]
fn parse_llm_payload_accepts_three_gate_baggage_actions() {
    let parsed = parse_llm_payload(
        r#"{
          "summary": "三个航班登机口开包",
          "actions": [
            {"case_type":"gate_baggage_check","case_type_name":"登机口开包","flight_number_raw":"7714","leg_type_hint":"outbound","description":"登机口开包，座位号 23A","remarks":"座位号 23A","fields":{"seat_no":"23A"},"confidence":0.9},
            {"case_type":"gate_baggage_check","case_type_name":"登机口开包","flight_number_raw":"5352","leg_type_hint":"outbound","description":"登机口开包，座位号 32F","remarks":"座位号 32F","fields":{"seat_no":"32F"},"confidence":0.9},
            {"case_type":"gate_baggage_check","case_type_name":"登机口开包","flight_number_raw":"6333","leg_type_hint":"outbound","description":"登机口开包，座位号 1A","remarks":"座位号 1A","fields":{"seat_no":"1A"},"confidence":0.9}
          ]
        }"#,
    )
    .unwrap();
    assert_eq!(parsed.actions.len(), 3);
    assert_eq!(parsed.actions[0].case_type, "gate_baggage_check");
    assert!(parsed.actions[1].remarks.contains("32F"));
}

#[test]
fn parse_ai_extraction_config_works() {
    let raw = serde_json::json!({
        "enabled": true,
        "aliases": ["开包"],
        "leg_binding": {
            "allowed": ["outbound"],
            "default": "outbound",
            "required": true
        },
        "fields": {
            "seat_no": {
                "type": "string",
                "label": "座位号",
                "required": true
            }
        }
    });
    let parsed = parse_ai_extraction_config(&raw);
    assert!(parsed.is_some());
    let config = parsed.unwrap();
    assert!(config.enabled);
    assert_eq!(config.aliases[0], "开包");
    assert_eq!(config.leg_binding.default.as_deref(), Some("outbound"));
    assert_eq!(
        config.fields.get("seat_no").unwrap().field_type.as_deref(),
        Some("string")
    );

    let disabled = serde_json::json!({
        "enabled": false
    });
    assert!(parse_ai_extraction_config(&disabled).is_none());
}

pub(super) fn make_existing_case_for_duplicate_test(
    case_id: &str,
    status: &str,
    context: HashMap<String, Value>,
) -> FlightBusinessCase {
    FlightBusinessCase {
        case_id: case_id.to_string(),
        case_type: "gate_baggage_check".to_string(),
        case_type_name: Some("登机口开包".to_string()),
        flight_id: "flight-1".to_string(),
        flight_no: "CZ7714".to_string(),
        created_at: Utc::now(),
        created_by: "tester".to_string(),
        updated_by: "tester".to_string(),
        description: "登机口开包".to_string(),
        status: status.to_string(),
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
    }
}

pub(super) fn make_prepared_action_for_duplicate_test(
    seat_no: &str,
    policy: CaseDuplicatePolicy,
) -> PreparedCommitAction {
    let mut context = HashMap::new();
    context.insert("bound_leg_type".to_string(), serde_json::json!("outbound"));
    context.insert("seat_no".to_string(), serde_json::json!(seat_no));
    context.insert("extra_info".to_string(), serde_json::json!("座位号 23A"));

    PreparedCommitAction {
        action: AiCopilotApprovedAction {
            action_id: "act_1".to_string(),
            case_type: "gate_baggage_check".to_string(),
            flight_id: "flight-1".to_string(),
            flight_no: "CZ7714".to_string(),
            bound_leg_type: Some("outbound".to_string()),
            bound_flight_no: Some("CZ7714".to_string()),
            description: Some("登机口开包".to_string()),
            remarks: Some("座位号 23A".to_string()),
            fields: serde_json::json!({ "seat_no": seat_no }),
            status: None,
        },
        flight_id: "flight-1".to_string(),
        flight_no: "CZ7714".to_string(),
        description: "登机口开包".to_string(),
        status: None,
        context,
        duplicate_policy: policy,
    }
}

#[test]
fn duplicate_policy_matches_configured_fields_and_leg() {
    let policy = CaseDuplicatePolicy {
        enabled: true,
        fields: vec!["seat_no".to_string()],
        include_extra_info: false,
        include_bound_leg: true,
        active_statuses: vec![],
    };
    let prepared = make_prepared_action_for_duplicate_test("23A", policy);

    let existing = make_existing_case_for_duplicate_test(
        "case-1",
        "INITIAL",
        HashMap::from([
            ("bound_leg_type".to_string(), serde_json::json!("outbound")),
            ("seat_no".to_string(), serde_json::json!("23a")),
            ("extra_info".to_string(), serde_json::json!("旧备注可以不同")),
        ]),
    );
    assert!(is_duplicate_copilot_case(&existing, &prepared));

    let different_seat = make_existing_case_for_duplicate_test(
        "case-2",
        "INITIAL",
        HashMap::from([
            ("bound_leg_type".to_string(), serde_json::json!("outbound")),
            ("seat_no".to_string(), serde_json::json!("32F")),
        ]),
    );
    assert!(!is_duplicate_copilot_case(&different_seat, &prepared));

    let finished = make_existing_case_for_duplicate_test(
        "case-3",
        "FINISHED",
        HashMap::from([
            ("bound_leg_type".to_string(), serde_json::json!("outbound")),
            ("seat_no".to_string(), serde_json::json!("23A")),
        ]),
    );
    assert!(!is_duplicate_copilot_case(&finished, &prepared));

    let inbound = make_existing_case_for_duplicate_test(
        "case-4",
        "INITIAL",
        HashMap::from([
            ("bound_leg_type".to_string(), serde_json::json!("inbound")),
            ("seat_no".to_string(), serde_json::json!("23A")),
        ]),
    );
    assert!(!is_duplicate_copilot_case(&inbound, &prepared));
}

#[test]
fn duplicate_policy_honors_explicit_active_statuses_and_extra_info() {
    let policy = CaseDuplicatePolicy {
        enabled: true,
        fields: vec!["seat_no".to_string()],
        include_extra_info: true,
        include_bound_leg: false,
        active_statuses: vec!["notification_sent".to_string()],
    };
    let prepared = make_prepared_action_for_duplicate_test("23A", policy);

    let matching_status = make_existing_case_for_duplicate_test(
        "case-1",
        "NOTIFICATION_SENT",
        HashMap::from([
            ("seat_no".to_string(), serde_json::json!("23A")),
            ("extra_info".to_string(), serde_json::json!("座位号 23a")),
        ]),
    );
    assert!(is_duplicate_copilot_case(&matching_status, &prepared));

    let wrong_status = make_existing_case_for_duplicate_test(
        "case-2",
        "INITIAL",
        HashMap::from([
            ("seat_no".to_string(), serde_json::json!("23A")),
            ("extra_info".to_string(), serde_json::json!("座位号 23A")),
        ]),
    );
    assert!(!is_duplicate_copilot_case(&wrong_status, &prepared));

    let different_extra_info = make_existing_case_for_duplicate_test(
        "case-3",
        "notification_sent",
        HashMap::from([
            ("seat_no".to_string(), serde_json::json!("23A")),
            ("extra_info".to_string(), serde_json::json!("座位号 23A，补充说明")),
        ]),
    );
    assert!(!is_duplicate_copilot_case(&different_extra_info, &prepared));
}

#[test]
fn duplicate_policy_rejects_duplicate_actions_in_same_batch() {
    let policy = CaseDuplicatePolicy {
        enabled: true,
        fields: vec!["seat_no".to_string()],
        include_extra_info: false,
        include_bound_leg: true,
        active_statuses: vec![],
    };
    let first = make_prepared_action_for_duplicate_test("23A", policy.clone());
    let mut second = make_prepared_action_for_duplicate_test("23a", policy.clone());
    second.action.action_id = "act_2".to_string();

    let err = reject_duplicate_copilot_actions_in_batch(&[first, second])
        .expect_err("same batch duplicate should be rejected");
    assert!(err.to_string().contains("批次内存在重复业务事项"));

    let first = make_prepared_action_for_duplicate_test("23A", policy.clone());
    let mut different_flight = make_prepared_action_for_duplicate_test("23A", policy);
    different_flight.action.action_id = "act_3".to_string();
    different_flight.flight_id = "flight-2".to_string();
    different_flight.action.flight_id = "flight-2".to_string();
    different_flight.flight_no = "CZ5352".to_string();
    different_flight.action.flight_no = "CZ5352".to_string();

    reject_duplicate_copilot_actions_in_batch(&[first, different_flight]).unwrap();
}

#[test]
fn duplicate_action_ids_are_rejected_before_commit() {
    let policy = CaseDuplicatePolicy {
        enabled: false,
        ..Default::default()
    };
    let first = make_prepared_action_for_duplicate_test("23A", policy.clone());
    let mut second = make_prepared_action_for_duplicate_test("32F", policy);
    second.action.action_id = "ACT_1".to_string();

    let err = reject_duplicate_copilot_action_ids_in_batch(&[first, second])
        .expect_err("duplicate action ids should be rejected");
    assert!(err.to_string().contains("重复 action_id"));
}

#[test]
fn test_build_extraction_prompt_dynamic() {
    let catalog = vec![CopilotCaseTypeCatalogEntry {
        code: "custom_check".to_string(),
        name: "自定义检查".to_string(),
        description: Some("自定义事项".to_string()),
        config: BusinessCaseAiExtractionConfig {
            enabled: true,
            aliases: vec!["自定义".to_string()],
            ..Default::default()
        },
        case_properties: BusinessCaseProperties::default(),
    }];
    let prompt = build_extraction_prompt("测试 transcript", &catalog);
    assert!(prompt.contains("custom_check"));
    assert!(prompt.contains("自定义检查"));
    assert!(!prompt.contains("gate_baggage_check"));
}

#[test]
fn test_retrieve_candidate_case_types_ranking() {
    let catalog = vec![
        CopilotCaseTypeCatalogEntry {
            code: "other_case".to_string(),
            name: "无关事项".to_string(),
            description: None,
            config: BusinessCaseAiExtractionConfig {
                enabled: true,
                aliases: vec!["无关".to_string()],
                ..Default::default()
            },
            case_properties: BusinessCaseProperties::default(),
        },
        CopilotCaseTypeCatalogEntry {
            code: "baggage_case".to_string(),
            name: "开包检查".to_string(),
            description: None,
            config: BusinessCaseAiExtractionConfig {
                enabled: true,
                aliases: vec!["登机口开包".to_string(), "开包".to_string()],
                ..Default::default()
            },
            case_properties: BusinessCaseProperties::default(),
        },
        CopilotCaseTypeCatalogEntry {
            code: "zebra_case".to_string(),
            name: "斑马开包".to_string(),
            description: None,
            config: BusinessCaseAiExtractionConfig {
                enabled: true,
                aliases: vec!["开包".to_string()],
                ..Default::default()
            },
            case_properties: BusinessCaseProperties::default(),
        },
        CopilotCaseTypeCatalogEntry {
            code: "alpha_case".to_string(),
            name: "阿尔法开包".to_string(),
            description: None,
            config: BusinessCaseAiExtractionConfig {
                enabled: true,
                aliases: vec!["开包".to_string()],
                ..Default::default()
            },
            case_properties: BusinessCaseProperties::default(),
        },
    ];

    let candidates = retrieve_candidate_case_types("有登机口开包吗", &catalog, 1);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].code, "baggage_case");

    let candidates_same = retrieve_candidate_case_types("只开包", &catalog, 5);
    let codes_same: Vec<String> = candidates_same.into_iter().map(|entry| entry.code).collect();
    assert_eq!(
        codes_same,
        vec![
            "alpha_case".to_string(),
            "baggage_case".to_string(),
            "zebra_case".to_string()
        ]
    );
}

// Mock definitions
use chrono::Utc;
use fms_domain::models::ai_copilot::{AiCopilotBatchStatus, AiCopilotBusinessCaseBatch};
use fms_domain::models::business_case::{BusinessCaseType, VisibilityScope};
use fms_domain::ports::ai_copilot_repository::BeginCommitResult;
use fms_domain::ports::business_case_repository::BusinessCaseTypeRepository;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::services::ai_admin_service::AiAdminService;
use crate::services::business_case_type_service::BusinessCaseTypeService;
use crate::services::business_case_workflow_service::WorkflowActor;
use crate::services::flight_service::FlightService;

pub(super) fn batch_access(actor: &str) -> AiCopilotBatchAccess {
    AiCopilotBatchAccess::for_actor_keys([actor])
}

pub(super) fn ops_batch_access() -> AiCopilotBatchAccess {
    AiCopilotBatchAccess::unrestricted()
}

#[derive(Default)]
pub(super) struct FakeAiCopilotBusinessCaseBatchRepository {
    batches: Arc<Mutex<HashMap<String, AiCopilotBusinessCaseBatch>>>,
}

#[async_trait::async_trait]
impl AiCopilotBusinessCaseBatchRepository for FakeAiCopilotBusinessCaseBatchRepository {
    async fn save(&self, batch: &AiCopilotBusinessCaseBatch) -> Result<(), DomainError> {
        self.batches
            .lock()
            .unwrap()
            .insert(batch.batch_id.clone(), batch.clone());
        Ok(())
    }
    async fn find_by_id(&self, batch_id: &str) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        Ok(self.batches.lock().unwrap().get(batch_id).cloned())
    }
    async fn list(
        &self,
        status: Option<AiCopilotBatchStatus>,
        workflow_dispatch_status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut items = self
            .batches
            .lock()
            .unwrap()
            .values()
            .filter(|batch| status.map(|s| batch.status == s).unwrap_or(true))
            .filter(|batch| {
                workflow_dispatch_status
                    .map(|status| batch.workflow_dispatch_status == status)
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
        Ok(items
            .into_iter()
            .skip(offset.max(0) as usize)
            .take(limit.clamp(1, 200) as usize)
            .collect())
    }

    async fn list_due_workflow_dispatch_retries(
        &self,
        limit: i64,
        max_attempts: i32,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError> {
        let now = Utc::now();
        let mut items = self
            .batches
            .lock()
            .unwrap()
            .values()
            .filter(|batch| batch.status == AiCopilotBatchStatus::Committed)
            .filter(|batch| batch.workflow_dispatch_status == "failed")
            .filter(|batch| batch.workflow_dispatch_request.is_some())
            .filter(|batch| batch.workflow_dispatch_attempts < max_attempts.max(1))
            .filter(|batch| {
                batch
                    .workflow_dispatch_next_retry_at
                    .map(|next_retry| next_retry <= now)
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|a, b| {
            a.workflow_dispatch_next_retry_at
                .unwrap_or(a.updated_at)
                .cmp(&b.workflow_dispatch_next_retry_at.unwrap_or(b.updated_at))
        });
        Ok(items.into_iter().take(limit.clamp(1, 200) as usize).collect())
    }

    async fn recover_stale_workflow_dispatch_pending(
        &self,
        stale_before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError> {
        let now = Utc::now();
        let mut guard = self.batches.lock().unwrap();
        let mut batch_ids = guard
            .values()
            .filter(|batch| batch.status == AiCopilotBatchStatus::Committed)
            .filter(|batch| batch.workflow_dispatch_status == "pending")
            .filter(|batch| batch.workflow_dispatch_request.is_some())
            .filter(|batch| batch.updated_at <= stale_before)
            .map(|batch| (batch.batch_id.clone(), batch.updated_at))
            .collect::<Vec<_>>();
        batch_ids.sort_by_key(|a| a.1);
        batch_ids.truncate(limit.clamp(1, 200) as usize);

        let mut recovered = Vec::with_capacity(batch_ids.len());
        for (batch_id, pending_updated_at) in batch_ids {
            if let Some(batch) = guard.get_mut(&batch_id) {
                if batch.status != AiCopilotBatchStatus::Committed
                    || batch.workflow_dispatch_status != "pending"
                    || batch.workflow_dispatch_request.is_none()
                    || batch.updated_at > stale_before
                {
                    continue;
                }
                batch.workflow_dispatch_status = "failed".to_string();
                batch.workflow_dispatch_error = Some(json!({
                    "stage": "workflow_dispatch_stale_pending",
                    "message": "workflow dispatch remained pending past stale threshold",
                    "pending_updated_at": pending_updated_at,
                    "stale_before": stale_before,
                    "recorded_at": now,
                }));
                batch.workflow_dispatch_next_retry_at = None;
                batch.updated_at = now;
                recovered.push(batch.clone());
            }
        }

        Ok(recovered)
    }

    async fn operational_metrics(
        &self,
        max_workflow_dispatch_attempts: i32,
        recent_error_limit: i64,
    ) -> Result<AiCopilotOperationalMetrics, DomainError> {
        let max_attempts = max_workflow_dispatch_attempts.max(1);
        let now = Utc::now();
        let mut batch_status = AiCopilotBatchStatusMetrics::default();
        let mut workflow_dispatch = AiCopilotWorkflowDispatchMetrics {
            max_attempts,
            ..Default::default()
        };
        let mut recent_errors = Vec::new();

        for batch in self.batches.lock().unwrap().values() {
            batch_status.total += 1;
            match batch.status {
                AiCopilotBatchStatus::Draft => batch_status.draft += 1,
                AiCopilotBatchStatus::Committing => batch_status.committing += 1,
                AiCopilotBatchStatus::Committed => batch_status.committed += 1,
                AiCopilotBatchStatus::Failed => batch_status.failed += 1,
                AiCopilotBatchStatus::FailedResolved => batch_status.failed_resolved += 1,
                AiCopilotBatchStatus::Expired => batch_status.expired += 1,
            }

            match batch.workflow_dispatch_status.as_str() {
                "pending" => workflow_dispatch.pending += 1,
                "succeeded" => workflow_dispatch.succeeded += 1,
                "failed" => workflow_dispatch.failed += 1,
                _ => workflow_dispatch.not_required += 1,
            }

            if batch.status == AiCopilotBatchStatus::Committed
                && batch.workflow_dispatch_status == "failed"
                && batch.workflow_dispatch_request.is_some()
                && batch.workflow_dispatch_attempts < max_attempts
                && batch
                    .workflow_dispatch_next_retry_at
                    .map(|next_retry| next_retry <= now)
                    .unwrap_or(true)
            {
                workflow_dispatch.retry_due += 1;
            }

            if batch.status == AiCopilotBatchStatus::Committed
                && batch.workflow_dispatch_status == "failed"
                && batch.workflow_dispatch_attempts >= max_attempts
            {
                workflow_dispatch.retry_exhausted += 1;
            }

            if batch.status == AiCopilotBatchStatus::Failed || batch.workflow_dispatch_status == "failed" {
                let error = batch.workflow_dispatch_error.as_ref().or(batch.commit_error.as_ref());
                recent_errors.push(AiCopilotOperationalError {
                    batch_id: batch.batch_id.clone(),
                    status: batch.status,
                    workflow_dispatch_status: batch.workflow_dispatch_status.clone(),
                    stage: error.and_then(extract_error_stage),
                    message: error.and_then(extract_error_message),
                    attempts: batch.workflow_dispatch_attempts,
                    next_retry_at: batch.workflow_dispatch_next_retry_at,
                    updated_at: batch.updated_at,
                });
            }
        }

        recent_errors.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
        recent_errors.truncate(recent_error_limit.clamp(1, 50) as usize);

        Ok(AiCopilotOperationalMetrics {
            generated_at: now,
            batch_status,
            workflow_dispatch,
            recent_errors,
        })
    }

    async fn try_begin_commit(&self, batch_id: &str) -> Result<BeginCommitResult, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        match guard.get_mut(batch_id) {
            None => Ok(BeginCommitResult::NotFound),
            Some(batch) => match batch.status {
                AiCopilotBatchStatus::Draft => {
                    let now = Utc::now();
                    batch.status = AiCopilotBatchStatus::Committing;
                    batch.commit_started_at = Some(now);
                    batch.commit_attempts += 1;
                    batch.commit_next_recovery_at = None;
                    batch.updated_at = now;
                    Ok(BeginCommitResult::Acquired(batch.clone()))
                }
                AiCopilotBatchStatus::Committed => Ok(BeginCommitResult::AlreadyCommitted(batch.clone())),
                AiCopilotBatchStatus::Committing
                | AiCopilotBatchStatus::Failed
                | AiCopilotBatchStatus::FailedResolved
                | AiCopilotBatchStatus::Expired => Ok(BeginCommitResult::Conflict(batch.clone())),
            },
        }
    }

    async fn try_begin_commit_with_request(
        &self,
        batch_id: &str,
        commit_request: &serde_json::Value,
        next_recovery_at: Option<DateTime<Utc>>,
    ) -> Result<BeginCommitResult, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        match guard.get_mut(batch_id) {
            None => Ok(BeginCommitResult::NotFound),
            Some(batch) => match batch.status {
                AiCopilotBatchStatus::Draft => {
                    let now = Utc::now();
                    batch.status = AiCopilotBatchStatus::Committing;
                    batch.commit_request = Some(commit_request.clone());
                    batch.created_action_case_ids = json!({});
                    batch.commit_error = None;
                    batch.commit_started_at = Some(now);
                    batch.commit_attempts += 1;
                    batch.commit_next_recovery_at = next_recovery_at;
                    batch.updated_at = now;
                    Ok(BeginCommitResult::Acquired(batch.clone()))
                }
                AiCopilotBatchStatus::Committed => Ok(BeginCommitResult::AlreadyCommitted(batch.clone())),
                AiCopilotBatchStatus::Committing
                | AiCopilotBatchStatus::Failed
                | AiCopilotBatchStatus::FailedResolved
                | AiCopilotBatchStatus::Expired => Ok(BeginCommitResult::Conflict(batch.clone())),
            },
        }
    }

    async fn record_created_action_case(
        &self,
        batch_id: &str,
        action_id: &str,
        case_id: &str,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let action_id = action_id.trim();
        let case_id = case_id.trim();
        if action_id.is_empty() || case_id.is_empty() {
            return Err(DomainError::ValidationError(
                "action_id and case_id are required".into(),
            ));
        }

        let mut guard = self.batches.lock().unwrap();
        let Some(batch) = guard.get_mut(batch_id) else {
            return Ok(None);
        };
        if batch.status != AiCopilotBatchStatus::Committing {
            return Ok(None);
        }

        if !batch.created_action_case_ids.is_object() {
            batch.created_action_case_ids = json!({});
        }
        if let Some(map) = batch.created_action_case_ids.as_object_mut() {
            if let Some(existing_case_id) = map.get(action_id).and_then(Value::as_str) {
                if existing_case_id != case_id {
                    return Err(DomainError::Conflict(format!(
                        "copilot action {action_id} already recorded case {existing_case_id}"
                    )));
                }
            } else {
                map.insert(action_id.to_string(), serde_json::Value::String(case_id.to_string()));
            }
        }
        batch.updated_at = Utc::now();
        Ok(Some(batch.clone()))
    }

    async fn recover_stale_committing(
        &self,
        stale_before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError> {
        let now = Utc::now();
        let mut guard = self.batches.lock().unwrap();
        let mut batch_ids = guard
            .values()
            .filter(|batch| batch.status == AiCopilotBatchStatus::Committing)
            .filter(|batch| {
                batch
                    .commit_started_at
                    .map(|started_at| started_at <= stale_before)
                    .unwrap_or(false)
            })
            .filter(|batch| {
                batch
                    .commit_next_recovery_at
                    .map(|next_recovery| next_recovery <= now)
                    .unwrap_or(true)
            })
            .map(|batch| {
                (
                    batch.batch_id.clone(),
                    batch
                        .commit_next_recovery_at
                        .or(batch.commit_started_at)
                        .unwrap_or(batch.updated_at),
                    batch.commit_started_at.unwrap_or(batch.updated_at),
                )
            })
            .collect::<Vec<_>>();
        batch_ids.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));
        batch_ids.truncate(limit.clamp(1, 200) as usize);

        let mut recovered = Vec::with_capacity(batch_ids.len());
        for (batch_id, _, _) in batch_ids {
            if let Some(batch) = guard.get_mut(&batch_id) {
                if batch.status != AiCopilotBatchStatus::Committing
                    || batch
                        .commit_started_at
                        .map(|started_at| started_at > stale_before)
                        .unwrap_or(true)
                    || batch
                        .commit_next_recovery_at
                        .map(|next_recovery| next_recovery > now)
                        .unwrap_or(false)
                {
                    continue;
                }
                let delay_seconds = 60_i64 * 2_i64.pow(batch.commit_attempts.clamp(0, 5) as u32);
                batch.commit_attempts += 1;
                batch.commit_next_recovery_at = Some(now + Duration::seconds(delay_seconds.min(3600)));
                batch.updated_at = now;
                recovered.push(batch.clone());
            }
        }

        Ok(recovered)
    }
    async fn mark_committed(
        &self,
        batch_id: &str,
        case_ids: &[String],
        notification_groups: &serde_json::Value,
        _idempotency_key: Option<&str>,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        if let Some(batch) = guard.get_mut(batch_id) {
            if batch.status != AiCopilotBatchStatus::Committing {
                return Ok(None);
            }
            batch.status = AiCopilotBatchStatus::Committed;
            batch.committed_case_ids = case_ids.to_vec();
            batch.notification_groups = notification_groups.clone();
            batch.commit_error = None;
            batch.commit_next_recovery_at = None;
            batch.committed_at = Some(Utc::now());
            return Ok(Some(batch.clone()));
        }
        Ok(None)
    }

    async fn mark_committed_with_workflow_dispatch_request(
        &self,
        batch_id: &str,
        case_ids: &[String],
        notification_groups: &serde_json::Value,
        idempotency_key: Option<&str>,
        workflow_dispatch_request: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut committed = self
            .mark_committed(batch_id, case_ids, notification_groups, idempotency_key)
            .await?;
        if let Some(batch) = committed.as_mut() {
            batch.workflow_dispatch_status = "pending".to_string();
            batch.workflow_dispatch_request = Some(workflow_dispatch_request.clone());
            batch.workflow_dispatch_error = None;
            batch.workflow_dispatch_next_retry_at = None;
            batch.updated_at = Utc::now();
            self.batches
                .lock()
                .unwrap()
                .insert(batch.batch_id.clone(), batch.clone());
        }
        Ok(committed)
    }

    async fn mark_commit_failed(
        &self,
        batch_id: &str,
        case_ids: &[String],
        error: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        if let Some(batch) = guard.get_mut(batch_id) {
            if batch.status != AiCopilotBatchStatus::Committing {
                return Ok(None);
            }
            batch.status = AiCopilotBatchStatus::Failed;
            batch.committed_case_ids = case_ids.to_vec();
            batch.commit_error = Some(error.clone());
            batch.commit_next_recovery_at = None;
            return Ok(Some(batch.clone()));
        }
        Ok(None)
    }

    async fn mark_workflow_dispatch_pending(
        &self,
        batch_id: &str,
        request: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        if let Some(batch) = guard.get_mut(batch_id) {
            batch.workflow_dispatch_status = "pending".to_string();
            batch.workflow_dispatch_request = Some(request.clone());
            batch.workflow_dispatch_error = None;
            batch.workflow_dispatch_next_retry_at = None;
            batch.updated_at = Utc::now();
            return Ok(Some(batch.clone()));
        }
        Ok(None)
    }

    async fn try_begin_workflow_dispatch_retry(
        &self,
        batch_id: &str,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        let Some(batch) = guard.get_mut(batch_id) else {
            return Ok(None);
        };
        if batch.status != AiCopilotBatchStatus::Committed
            || batch.workflow_dispatch_status != "failed"
            || batch.workflow_dispatch_request.is_none()
        {
            return Ok(None);
        }

        batch.workflow_dispatch_status = "pending".to_string();
        batch.workflow_dispatch_error = None;
        batch.workflow_dispatch_next_retry_at = None;
        batch.updated_at = Utc::now();
        Ok(Some(batch.clone()))
    }

    async fn mark_workflow_dispatch_failed(
        &self,
        batch_id: &str,
        error: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        if let Some(batch) = guard.get_mut(batch_id) {
            batch.workflow_dispatch_status = "failed".to_string();
            batch.workflow_dispatch_error = Some(error.clone());
            batch.workflow_dispatch_attempts += 1;
            batch.workflow_dispatch_next_retry_at = Some(Utc::now() + Duration::seconds(60));
            batch.updated_at = Utc::now();
            return Ok(Some(batch.clone()));
        }
        Ok(None)
    }

    async fn mark_workflow_dispatch_succeeded(
        &self,
        batch_id: &str,
        notification_groups: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        if let Some(batch) = guard.get_mut(batch_id) {
            batch.workflow_dispatch_status = "succeeded".to_string();
            batch.workflow_dispatch_error = None;
            batch.workflow_dispatch_attempts += 1;
            batch.workflow_dispatch_next_retry_at = None;
            batch.workflow_dispatched_at = Some(Utc::now());
            batch.notification_groups = notification_groups.clone();
            batch.updated_at = Utc::now();
            return Ok(Some(batch.clone()));
        }
        Ok(None)
    }

    async fn reset_commit_to_draft(&self, batch_id: &str) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        if let Some(batch) = guard.get_mut(batch_id) {
            if batch.status != AiCopilotBatchStatus::Committing {
                return Ok(None);
            }
            batch.status = AiCopilotBatchStatus::Draft;
            batch.committed_case_ids = vec![];
            batch.notification_groups = json!([]);
            batch.commit_request = None;
            batch.created_action_case_ids = json!({});
            batch.commit_error = None;
            batch.commit_started_at = None;
            batch.commit_attempts = 0;
            batch.commit_next_recovery_at = None;
            batch.committed_at = None;
            batch.workflow_dispatch_status = "not_required".to_string();
            batch.workflow_dispatch_request = None;
            batch.workflow_dispatch_error = None;
            batch.workflow_dispatch_attempts = 0;
            batch.workflow_dispatch_next_retry_at = None;
            batch.workflow_dispatched_at = None;
            return Ok(Some(batch.clone()));
        }
        Ok(None)
    }

    async fn reset_failed_to_draft(
        &self,
        batch_id: &str,
        resolution: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        if let Some(batch) = guard.get_mut(batch_id) {
            if batch.status != AiCopilotBatchStatus::Failed {
                return Ok(None);
            }
            batch.status = AiCopilotBatchStatus::Draft;
            batch.committed_case_ids = vec![];
            batch.notification_groups = json!([]);
            batch.commit_request = None;
            batch.created_action_case_ids = json!({});
            batch.commit_error = Some(resolution.clone());
            batch.commit_started_at = None;
            batch.commit_attempts = 0;
            batch.commit_next_recovery_at = None;
            batch.committed_at = None;
            batch.workflow_dispatch_status = "not_required".to_string();
            batch.workflow_dispatch_request = None;
            batch.workflow_dispatch_error = None;
            batch.workflow_dispatch_attempts = 0;
            batch.workflow_dispatch_next_retry_at = None;
            batch.workflow_dispatched_at = None;
            batch.updated_at = Utc::now();
            return Ok(Some(batch.clone()));
        }
        Ok(None)
    }

    async fn mark_failed_resolved(
        &self,
        batch_id: &str,
        resolution: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let mut guard = self.batches.lock().unwrap();
        if let Some(batch) = guard.get_mut(batch_id) {
            if batch.status != AiCopilotBatchStatus::Failed {
                return Ok(None);
            }
            batch.status = AiCopilotBatchStatus::FailedResolved;
            batch.commit_error = Some(resolution.clone());
            batch.updated_at = Utc::now();
            return Ok(Some(batch.clone()));
        }
        Ok(None)
    }
}

pub(super) struct FakeAiEntityConfigRepository;

#[async_trait::async_trait]
impl fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository for FakeAiEntityConfigRepository {
    async fn find_all(&self) -> Result<Vec<fms_domain::models::ai_entity_config::AiEntityConfigRecord>, DomainError> {
        Ok(vec![])
    }
    async fn find_by_id(
        &self,
        _id: &str,
    ) -> Result<Option<fms_domain::models::ai_entity_config::AiEntityConfigRecord>, DomainError> {
        Ok(Some(fms_domain::models::ai_entity_config::AiEntityConfigRecord {
            id: "flight-monitor-copilot".to_string(),
            config: serde_json::json!({
                "api_key": "dummy",
                "base_url": "http://localhost",
                "default_model": "gpt-4",
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }))
    }
    async fn save(
        &self,
        id: &str,
        config: &serde_json::Value,
    ) -> Result<fms_domain::models::ai_entity_config::AiEntityConfigRecord, DomainError> {
        Ok(fms_domain::models::ai_entity_config::AiEntityConfigRecord {
            id: id.to_string(),
            config: config.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
    async fn delete(&self, _id: &str) -> Result<bool, DomainError> {
        Ok(true)
    }
}

#[derive(Default)]
pub(super) struct FakeBusinessCaseTypeRepo {
    items: Arc<Mutex<HashMap<String, BusinessCaseType>>>,
}

#[async_trait::async_trait]
impl fms_domain::ports::business_case_repository::BusinessCaseTypeRepository for FakeBusinessCaseTypeRepo {
    async fn find_all(&self, active_only: bool) -> Result<Vec<BusinessCaseType>, DomainError> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .filter(|item| !active_only || item.is_active)
            .cloned()
            .collect())
    }

    async fn find_all_scoped(
        &self,
        active_only: bool,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<BusinessCaseType>, DomainError> {
        Ok(self
            .find_all(active_only)
            .await?
            .into_iter()
            .filter(|item| is_case_type_visible(item, viewer_department_id, viewer_department_name, include_common))
            .collect())
    }

    async fn find_by_code(&self, code: &str) -> Result<Option<BusinessCaseType>, DomainError> {
        Ok(self.items.lock().unwrap().get(code).cloned())
    }

    async fn find_by_code_scoped(
        &self,
        code: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        Ok(self
            .find_by_code(code)
            .await?
            .filter(|item| is_case_type_visible(item, viewer_department_id, viewer_department_name, include_common)))
    }

    async fn save(&self, entity: &BusinessCaseType) -> Result<BusinessCaseType, DomainError> {
        self.items.lock().unwrap().insert(entity.code.clone(), entity.clone());
        Ok(entity.clone())
    }

    async fn update_bpmn_xml(
        &self,
        _code: &str,
        _bpmn_xml: &str,
        _description: Option<&str>,
    ) -> Result<bool, DomainError> {
        Ok(true)
    }

    async fn update_status(&self, code: &str, is_active: bool) -> Result<bool, DomainError> {
        let mut items = self.items.lock().unwrap();
        let Some(item) = items.get_mut(code) else {
            return Ok(false);
        };
        item.is_active = is_active;
        Ok(true)
    }

    async fn update_ai_extraction_config(
        &self,
        code: &str,
        config: &serde_json::Value,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        let mut items = self.items.lock().unwrap();
        let Some(item) = items.get_mut(code) else {
            return Ok(None);
        };
        item.ai_extraction_config = config.clone();
        Ok(Some(item.clone()))
    }

    async fn update_case_properties(
        &self,
        code: &str,
        properties: &serde_json::Value,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        let mut items = self.items.lock().unwrap();
        let Some(item) = items.get_mut(code) else {
            return Ok(None);
        };
        item.case_properties = properties.clone();
        Ok(Some(item.clone()))
    }
}

pub(super) fn is_case_type_visible(
    item: &BusinessCaseType,
    viewer_department_id: Option<&str>,
    viewer_department_name: Option<&str>,
    include_common: bool,
) -> bool {
    match item.visibility_scope {
        VisibilityScope::Common => include_common,
        VisibilityScope::Department => {
            item.department_id.as_deref() == viewer_department_id
                || item.department_name_snapshot.as_deref() == viewer_department_name
        }
    }
}

pub(super) fn test_business_case_type(
    code: &str,
    name: &str,
    ai_extraction_config: serde_json::Value,
    case_properties: serde_json::Value,
) -> BusinessCaseType {
    BusinessCaseType {
        id: code.to_string(),
        code: code.to_string(),
        name: name.to_string(),
        bpmn_xml: None,
        description: None,
        is_active: true,
        visibility_scope: VisibilityScope::Common,
        department_id: None,
        department_name_snapshot: None,
        created_at: Some(Utc::now()),
        updated_at: Some(Utc::now()),
        ai_extraction_config,
        case_properties,
    }
}

pub(super) fn test_copilot_batch(
    batch_id: &str,
    created_by: &str,
    status: AiCopilotBatchStatus,
) -> AiCopilotBusinessCaseBatch {
    AiCopilotBusinessCaseBatch {
        batch_id: batch_id.to_string(),
        entity_id: "flight-monitor-copilot".to_string(),
        source_page: "flight_monitor".to_string(),
        transcript_summary: format!("summary for {batch_id}"),
        transcript_text: "sensitive transcript".to_string(),
        draft_actions: json!([{"action_id":"act_1"}]),
        status,
        created_by: created_by.to_string(),
        committed_case_ids: vec![],
        idempotency_key: None,
        notification_groups: json!([]),
        commit_request: None,
        created_action_case_ids: json!({}),
        commit_error: None,
        commit_started_at: None,
        commit_attempts: 0,
        commit_next_recovery_at: None,
        committed_at: None,
        workflow_dispatch_status: "not_required".to_string(),
        workflow_dispatch_request: None,
        workflow_dispatch_error: None,
        workflow_dispatch_attempts: 0,
        workflow_dispatch_next_retry_at: None,
        workflow_dispatched_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
    }
}
