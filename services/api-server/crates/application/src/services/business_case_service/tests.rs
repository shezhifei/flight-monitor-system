use super::{BusinessCaseService, BusinessCaseUpdatePayload, NoMentionAudience};
use crate::types::NoopBusinessCaseEventPublisher;
use chrono::Utc;
use fms_domain::error::DomainError;
use fms_domain::models::business_case::{
    BusinessCaseAppendEntry, BusinessCaseType, FlightBusinessCase, VisibilityScope,
};
use fms_domain::ports::business_case_repository::{BusinessCaseRepository, BusinessCaseTypeRepository};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::services::business_case_type_service::BusinessCaseTypeService;

#[derive(Default)]
struct FakeBusinessCaseRepo {
    cases: Mutex<HashMap<String, FlightBusinessCase>>,
}

#[derive(Default)]
struct FakeBusinessCaseTypeRepo {
    items: Mutex<HashMap<String, BusinessCaseType>>,
}

#[async_trait::async_trait]
impl BusinessCaseRepository for FakeBusinessCaseRepo {
    async fn save(&self, case: &FlightBusinessCase) -> Result<(), DomainError> {
        self.cases
            .lock()
            .expect("lock cases")
            .insert(case.case_id.clone(), case.clone());
        Ok(())
    }

    async fn find_by_id(&self, case_id: &str) -> Result<Option<FlightBusinessCase>, DomainError> {
        Ok(self.cases.lock().expect("lock cases").get(case_id).cloned())
    }

    async fn find_by_id_scoped(
        &self,
        case_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        Ok(self
            .find_by_id(case_id)
            .await?
            .filter(|item| is_case_visible(item, viewer_department_id, viewer_department_name, include_common)))
    }

    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .cases
            .lock()
            .expect("lock cases")
            .values()
            .filter(|item| item.flight_id == flight_id)
            .cloned()
            .collect())
    }

    async fn find_by_flight_scoped(
        &self,
        flight_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .find_by_flight(flight_id)
            .await?
            .into_iter()
            .filter(|item| is_case_visible(item, viewer_department_id, viewer_department_name, include_common))
            .collect())
    }

    async fn find_by_flight_ids(&self, flight_ids: &[String]) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .cases
            .lock()
            .expect("lock cases")
            .values()
            .filter(|item| flight_ids.iter().any(|flight_id| flight_id == &item.flight_id))
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
            .expect("lock cases")
            .values()
            .filter(|item| {
                is_copilot_voice_case(item, batch_id)
                    && context_string(item, "copilot_action_id")
                        .as_deref()
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
            .expect("lock cases")
            .values()
            .filter(|item| is_copilot_voice_case(item, batch_id))
            .cloned()
            .collect::<Vec<_>>();
        cases.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.case_id.cmp(&right.case_id))
        });
        Ok(cases)
    }

    async fn find_by_flight_ids_scoped(
        &self,
        flight_ids: &[String],
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .find_by_flight_ids(flight_ids)
            .await?
            .into_iter()
            .filter(|item| is_case_visible(item, viewer_department_id, viewer_department_name, include_common))
            .collect())
    }

    async fn find_all(
        &self,
        status: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .cases
            .lock()
            .expect("lock cases")
            .values()
            .filter(|item| status.map(|value| item.status == value).unwrap_or(true))
            .cloned()
            .collect())
    }

    async fn find_all_scoped(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        self.find_filtered_scoped(
            None,
            None,
            status,
            viewer_department_id,
            viewer_department_name,
            include_common,
            Some(limit),
            Some(offset),
        )
        .await
    }

    async fn find_filtered(
        &self,
        flight_id: Option<&str>,
        case_type: Option<&str>,
        status: Option<&str>,
        _limit: Option<i64>,
        _offset: Option<i64>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .cases
            .lock()
            .expect("lock cases")
            .values()
            .filter(|item| flight_id.map(|value| item.flight_id == value).unwrap_or(true))
            .filter(|item| case_type.map(|value| item.case_type == value).unwrap_or(true))
            .filter(|item| status.map(|value| item.status == value).unwrap_or(true))
            .cloned()
            .collect())
    }

    async fn find_filtered_scoped(
        &self,
        flight_id: Option<&str>,
        case_type: Option<&str>,
        status: Option<&str>,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
        _limit: Option<i64>,
        _offset: Option<i64>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(self
            .find_filtered(flight_id, case_type, status, None, None)
            .await?
            .into_iter()
            .filter(|item| is_case_visible(item, viewer_department_id, viewer_department_name, include_common))
            .collect())
    }

    async fn update_case(&self, case: &FlightBusinessCase) -> Result<bool, DomainError> {
        let mut cases = self.cases.lock().expect("lock cases");
        if cases.contains_key(&case.case_id) {
            cases.insert(case.case_id.clone(), case.clone());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn update_status(&self, case_id: &str, status: &str, actor: &str) -> Result<bool, DomainError> {
        let mut cases = self.cases.lock().expect("lock cases");
        let Some(case) = cases.get_mut(case_id) else {
            return Ok(false);
        };
        if case.status == status {
            return Ok(false);
        }
        case.status = status.to_string();
        case.updated_by = actor.to_string();
        Ok(true)
    }

    async fn insert_append(&self, append: &BusinessCaseAppendEntry) -> Result<BusinessCaseAppendEntry, DomainError> {
        Ok(append.clone())
    }

    async fn insert_append_once(
        &self,
        append: &BusinessCaseAppendEntry,
    ) -> Result<(BusinessCaseAppendEntry, bool), DomainError> {
        Ok((append.clone(), true))
    }

    async fn find_append_by_id(&self, _append_id: &str) -> Result<Option<BusinessCaseAppendEntry>, DomainError> {
        Ok(None)
    }

    async fn update_append_metadata(
        &self,
        _append_id: &str,
        _metadata: serde_json::Value,
    ) -> Result<bool, DomainError> {
        Ok(true)
    }

    async fn delete(&self, case_id: &str) -> Result<bool, DomainError> {
        Ok(self.cases.lock().expect("lock cases").remove(case_id).is_some())
    }
}

#[async_trait::async_trait]
impl BusinessCaseTypeRepository for FakeBusinessCaseTypeRepo {
    async fn find_all(&self, active_only: bool) -> Result<Vec<BusinessCaseType>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("lock case types")
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
        Ok(self.items.lock().expect("lock case types").get(code).cloned())
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
        self.items
            .lock()
            .expect("lock case types")
            .insert(entity.code.clone(), entity.clone());
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
        let mut items = self.items.lock().expect("lock case types");
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
        let mut items = self.items.lock().expect("lock case types");
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
        let mut items = self.items.lock().expect("lock case types");
        let Some(item) = items.get_mut(code) else {
            return Ok(None);
        };
        item.case_properties = properties.clone();
        Ok(Some(item.clone()))
    }
}

fn build_case(case_id: &str) -> FlightBusinessCase {
    FlightBusinessCase {
        case_id: case_id.to_string(),
        case_type: "generic_case".to_string(),
        case_type_name: Some("通用事项".to_string()),
        flight_id: "flight-1".to_string(),
        flight_no: "CZ1234".to_string(),
        created_at: Utc::now(),
        created_by: "creator".to_string(),
        updated_by: "creator".to_string(),
        description: "old-desc".to_string(),
        status: "PENDING".to_string(),
        stand: Some("S1".to_string()),
        gate: Some("G8".to_string()),
        visibility_scope: VisibilityScope::Department,
        department_id: Some("ops-1".to_string()),
        department_name_snapshot: Some("ops".to_string()),
        finished_at: None,
        cancelled_at: None,
        log: vec![],
        context: HashMap::from([("bound_leg_type".to_string(), serde_json::json!("outbound"))]),
        workflow_receipt: None,
        terminal_metadata: None,
        append_count: 0,
        latest_append: None,
        append_entries: vec![],
    }
}

fn copilot_case(
    case_id: &str,
    batch_id: &str,
    action_id: &str,
    created_at: chrono::DateTime<Utc>,
) -> FlightBusinessCase {
    let mut case = build_case(case_id);
    case.created_at = created_at;
    case.context = HashMap::from([
        ("source".to_string(), serde_json::json!("ai_copilot_voice")),
        ("copilot_batch_id".to_string(), serde_json::json!(batch_id)),
        ("copilot_action_id".to_string(), serde_json::json!(action_id)),
    ]);
    case
}

fn context_string(case: &FlightBusinessCase, key: &str) -> Option<String> {
    case.context
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn is_copilot_voice_case(case: &FlightBusinessCase, batch_id: &str) -> bool {
    context_string(case, "source")
        .as_deref()
        .is_some_and(|value| value == "ai_copilot_voice")
        && context_string(case, "copilot_batch_id")
            .as_deref()
            .is_some_and(|value| value == batch_id)
}

fn build_case_type(
    code: &str,
    visibility_scope: VisibilityScope,
    department_id: Option<&str>,
    department_name_snapshot: Option<&str>,
) -> BusinessCaseType {
    BusinessCaseType {
        id: format!("type-{code}"),
        code: code.to_string(),
        name: format!("事项类型 {code}"),
        bpmn_xml: None,
        description: None,
        is_active: true,
        visibility_scope,
        department_id: department_id.map(str::to_string),
        department_name_snapshot: department_name_snapshot.map(str::to_string),
        created_at: Some(Utc::now()),
        updated_at: Some(Utc::now()),
        ai_extraction_config: serde_json::Value::Null,
        case_properties: serde_json::json!({}),
    }
}

fn is_case_type_visible(
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

fn is_case_visible(
    item: &FlightBusinessCase,
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

#[tokio::test]
async fn create_uses_explicit_status_when_provided() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    let service = BusinessCaseService::new(
        repo,
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );

    let case = service
        .create(
            "gate_check",
            "flight-1",
            "CZ1234",
            "desc",
            HashMap::new(),
            Some("INITIAL"),
            "tester",
        )
        .await
        .expect("create case");

    assert_eq!(case.status, "INITIAL");
    assert_eq!(case.created_by, "tester");
    assert_eq!(case.updated_by, "tester");
}

#[tokio::test]
async fn create_falls_back_to_context_status_when_explicit_missing() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    let service = BusinessCaseService::new(
        repo,
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );
    let context = HashMap::from([("status".to_string(), serde_json::json!("processing"))]);

    let case = service
        .create("gate_check", "flight-1", "CZ1234", "desc", context, None, "tester")
        .await
        .expect("create case");

    assert_eq!(case.status, "PROCESSING");
}

#[tokio::test]
async fn create_rejects_invalid_status() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    let service = BusinessCaseService::new(
        repo,
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );

    let error = service
        .create(
            "gate_check",
            "flight-1",
            "CZ1234",
            "desc",
            HashMap::new(),
            Some("ARCHIVED"),
            "tester",
        )
        .await
        .expect_err("invalid status should fail");

    assert!(matches!(error, DomainError::ValidationError(_)));
}

#[tokio::test]
async fn fake_repo_finds_copilot_case_by_batch_and_action() {
    let repo = FakeBusinessCaseRepo::default();
    let now = Utc::now();
    repo.save(&copilot_case("case-1", "batch-1", "action-1", now))
        .await
        .unwrap();
    repo.save(&copilot_case(
        "case-2-later",
        "batch-1",
        "action-2",
        now + chrono::Duration::seconds(5),
    ))
    .await
    .unwrap();
    repo.save(&copilot_case("case-2-b", "batch-1", "action-2", now))
        .await
        .unwrap();
    repo.save(&copilot_case("case-2-a", "batch-1", "action-2", now))
        .await
        .unwrap();
    repo.save(&copilot_case("case-3", "batch-2", "action-1", now))
        .await
        .unwrap();

    let found = repo.find_by_copilot_batch_action("batch-1", "action-2").await.unwrap();

    assert_eq!(found.map(|case| case.case_id), Some("case-2-a".to_string()));
}

#[tokio::test]
async fn fake_repo_lists_copilot_batch_cases_in_stable_created_order() {
    let repo = FakeBusinessCaseRepo::default();
    let now = Utc::now();
    repo.save(&copilot_case(
        "case-later",
        "batch-1",
        "action-2",
        now + chrono::Duration::seconds(5),
    ))
    .await
    .unwrap();
    repo.save(&copilot_case("case-earlier-b", "batch-1", "action-1", now))
        .await
        .unwrap();
    repo.save(&copilot_case("case-earlier-a", "batch-1", "action-1", now))
        .await
        .unwrap();
    repo.save(&copilot_case("case-other-batch", "batch-2", "action-1", now))
        .await
        .unwrap();

    let cases = repo.list_by_copilot_batch("batch-1").await.unwrap();

    assert_eq!(
        cases.into_iter().map(|case| case.case_id).collect::<Vec<_>>(),
        vec![
            "case-earlier-a".to_string(),
            "case-earlier-b".to_string(),
            "case-later".to_string(),
        ]
    );
}

#[tokio::test]
async fn list_filtered_for_viewer_returns_all_business_cases() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    let service = BusinessCaseService::new(
        repo.clone(),
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );

    repo.save(&FlightBusinessCase {
        visibility_scope: VisibilityScope::Common,
        department_id: None,
        department_name_snapshot: None,
        ..build_case("common-case")
    })
    .await
    .expect("save common case");

    repo.save(&build_case("dept-case")).await.expect("save department case");

    repo.save(&FlightBusinessCase {
        department_id: Some("other-1".to_string()),
        department_name_snapshot: Some("other".to_string()),
        ..build_case("other-case")
    })
    .await
    .expect("save other department case");

    let items = service
        .list_filtered_for_viewer(None, None, None, Some("ops-1"), Some("ops"))
        .await
        .expect("list filtered");

    let ids = items.into_iter().map(|item| item.case_id).collect::<Vec<_>>();
    assert!(ids.contains(&"common-case".to_string()));
    assert!(ids.contains(&"dept-case".to_string()));
    assert!(ids.contains(&"other-case".to_string()));
}

#[tokio::test]
async fn get_accessible_returns_other_department_case() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    let service = BusinessCaseService::new(
        repo.clone(),
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );
    repo.save(&FlightBusinessCase {
        department_id: Some("other-1".to_string()),
        department_name_snapshot: Some("other".to_string()),
        ..build_case("other-case")
    })
    .await
    .expect("save other department case");

    let item = service
        .get_accessible("other-case", Some("ops-1"), Some("ops"))
        .await
        .expect("load case");

    assert_eq!(item.expect("case should be globally visible").case_id, "other-case");
}

#[tokio::test]
async fn create_for_viewer_uses_visible_common_case_type_source() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    let case_type_repo = Arc::new(FakeBusinessCaseTypeRepo::default());
    case_type_repo
        .save(&build_case_type("common_type", VisibilityScope::Common, None, None))
        .await
        .expect("save common case type");
    let mut service = BusinessCaseService::new(
        repo,
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );
    service.set_business_case_type_service(Arc::new(BusinessCaseTypeService::new(case_type_repo)));

    let case = service
        .create_for_viewer(
            "common_type",
            "flight-1",
            "CZ1234",
            "desc",
            HashMap::new(),
            Some("INITIAL"),
            "tester",
            VisibilityScope::Department,
            Some("ops-1"),
            Some("ops"),
        )
        .await
        .expect("create common case");

    assert_eq!(case.visibility_scope, VisibilityScope::Common);
    assert_eq!(case.department_id, None);
    assert_eq!(case.department_name_snapshot, None);
}

#[tokio::test]
async fn create_for_viewer_uses_visible_department_case_type_source() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    let case_type_repo = Arc::new(FakeBusinessCaseTypeRepo::default());
    case_type_repo
        .save(&build_case_type(
            "dept_type",
            VisibilityScope::Department,
            Some("ops-1"),
            Some("ops"),
        ))
        .await
        .expect("save department case type");
    let mut service = BusinessCaseService::new(
        repo,
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );
    service.set_business_case_type_service(Arc::new(BusinessCaseTypeService::new(case_type_repo)));

    let case = service
        .create_for_viewer(
            "dept_type",
            "flight-1",
            "CZ1234",
            "desc",
            HashMap::new(),
            Some("INITIAL"),
            "tester",
            VisibilityScope::Common,
            Some("ops-1"),
            Some("ops"),
        )
        .await
        .expect("create department case");

    assert_eq!(case.visibility_scope, VisibilityScope::Department);
    assert_eq!(case.department_id.as_deref(), Some("ops-1"));
    assert_eq!(case.department_name_snapshot.as_deref(), Some("ops"));
}

#[tokio::test]
async fn create_for_viewer_rejects_other_department_case_type() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    let case_type_repo = Arc::new(FakeBusinessCaseTypeRepo::default());
    case_type_repo
        .save(&build_case_type(
            "other_dept_type",
            VisibilityScope::Department,
            Some("other-1"),
            Some("other"),
        ))
        .await
        .expect("save other department case type");
    let mut service = BusinessCaseService::new(
        repo,
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );
    service.set_business_case_type_service(Arc::new(BusinessCaseTypeService::new(case_type_repo)));

    let error = service
        .create_for_viewer(
            "other_dept_type",
            "flight-1",
            "CZ1234",
            "desc",
            HashMap::new(),
            Some("INITIAL"),
            "tester",
            VisibilityScope::Department,
            Some("ops-1"),
            Some("ops"),
        )
        .await
        .expect_err("other department type should not be creatable");

    assert!(matches!(error, DomainError::ValidationError(_)));
}

#[tokio::test]
async fn list_filtered_enriches_case_type_name() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    let case_type_repo = Arc::new(FakeBusinessCaseTypeRepo::default());
    case_type_repo
        .save(&BusinessCaseType {
            id: "type-1".to_string(),
            code: "generic_case".to_string(),
            name: "行李复核".to_string(),
            bpmn_xml: None,
            description: None,
            is_active: true,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::Value::Null,
            case_properties: serde_json::json!({}),
        })
        .await
        .expect("save case type");

    let mut service = BusinessCaseService::new(
        repo.clone(),
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );
    service.set_business_case_type_service(Arc::new(BusinessCaseTypeService::new(case_type_repo)));

    repo.save(&FlightBusinessCase {
        case_type_name: None,
        visibility_scope: VisibilityScope::Common,
        department_id: None,
        department_name_snapshot: None,
        ..build_case("case-1")
    })
    .await
    .expect("seed case");

    let items = service.list_filtered(None, None, None).await.expect("list filtered");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].case_type_name.as_deref(), Some("行李复核"));
}

#[tokio::test]
async fn update_status_only_changes_status_and_updated_by() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    repo.save(&build_case("case-1")).await.expect("seed case");
    let service = BusinessCaseService::new(
        repo.clone(),
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );

    let updated = service
        .update_status("case-1", "success", "editor")
        .await
        .expect("update status");
    assert!(updated);

    let case = repo
        .find_by_id("case-1")
        .await
        .expect("load case")
        .expect("case exists");
    assert_eq!(case.status, "SUCCESS");
    assert_eq!(case.updated_by, "editor");
    assert_eq!(case.description, "old-desc");
    assert_eq!(case.case_type, "generic_case");
    assert_eq!(case.context.get("bound_leg_type"), Some(&serde_json::json!("outbound")));
    assert_eq!(case.stand.as_deref(), Some("S1"));
    assert_eq!(case.gate.as_deref(), Some("G8"));
}

#[tokio::test]
async fn update_case_rejects_invalid_status() {
    let repo = Arc::new(FakeBusinessCaseRepo::default());
    repo.save(&build_case("case-2")).await.expect("seed case");
    let service = BusinessCaseService::new(
        repo,
        Arc::new(NoopBusinessCaseEventPublisher),
        Arc::new(NoMentionAudience),
    );

    let error = service
        .update_case(
            "case-2",
            BusinessCaseUpdatePayload {
                status: Some("archived".to_string()),
                ..BusinessCaseUpdatePayload::default()
            },
            "editor",
        )
        .await
        .expect_err("invalid status should fail");

    assert!(matches!(error, DomainError::ValidationError(_)));
}
