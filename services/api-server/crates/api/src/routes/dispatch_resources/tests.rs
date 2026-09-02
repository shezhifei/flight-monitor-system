use crate::middleware::jwt::JwtSecret;
use crate::routes::dispatch_resources::configure;
use actix_web::{body::to_bytes, http::StatusCode, test, web, App};
use chrono::Utc;
use fms_application::schemas::auth_schemas::TokenData;
use fms_application::services::dispatch_rule_service::DispatchRuleService;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    Department, DepartmentQualificationCatalog, DepartmentQualificationLevel, DepartmentRuleStatus,
    DepartmentTaskTypeRequirementVersion, DispatchPublicationState, FlightGenerationRule, GenerationAdjustmentRule,
    LegScope, PublishTriggerMode, QualificationGrant, TemporaryTaskTemplate,
};
use fms_domain::ports::dispatch_repository::{
    DepartmentQualificationRepository, DepartmentRepository, DepartmentTaskTypeRequirementRepository,
    FlightGenerationRuleRepository, GenerationAdjustmentRuleRepository, QualificationGrantRepository,
    TemporaryTaskTemplateRepository,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use std::sync::{Arc, Mutex};

fn bearer_token() -> String {
    encode(
        &Header::default(),
        &TokenData {
            sub: Some("user-1".to_string()),
            email: None,
            username: Some("tester".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: vec!["dispatch:manage".to_string()],
            department: Some("ops".to_string()),
            department_id: Some("ops-1".to_string()),
            pv: Some(1),
            iat: Some(Utc::now().timestamp()),
            exp: Some((Utc::now() + chrono::Duration::hours(1)).timestamp()),
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        },
        &EncodingKey::from_secret(b"test-secret"),
    )
    .expect("test jwt should encode")
}

#[derive(Clone)]
struct FakeDepartmentRepo;

#[async_trait::async_trait]
impl DepartmentRepository for FakeDepartmentRepo {
    async fn save(&self, dept: &Department) -> Result<Department, DomainError> {
        Ok(dept.clone())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Department>, DomainError> {
        Ok((id == "dept-1").then(|| Department {
            id: "dept-1".to_string(),
            name: "Ops".to_string(),
            code: Some("OPS".to_string()),
            description: None,
            manager_id: None,
            terminal: None,
            created_at: None,
            updated_at: None,
            is_active: true,
            attributes: serde_json::json!({}),
        }))
    }

    async fn find_by_name(&self, _name: &str) -> Result<Option<Department>, DomainError> {
        unimplemented!()
    }

    async fn find_all(
        &self,
        _include_inactive: bool,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<Department>, DomainError> {
        unimplemented!()
    }

    async fn has_dependencies(&self, _department_id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }

    async fn delete_permanently(&self, _department_id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
}

#[derive(Clone)]
struct FakeQualificationRepo;

#[async_trait::async_trait]
impl DepartmentQualificationRepository for FakeQualificationRepo {
    async fn save_catalog(
        &self,
        _catalog: &DepartmentQualificationCatalog,
    ) -> Result<DepartmentQualificationCatalog, DomainError> {
        unimplemented!()
    }

    async fn list_catalogs(
        &self,
        _department_id: &str,
        _include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationCatalog>, DomainError> {
        unimplemented!()
    }

    async fn save_level(
        &self,
        _level: &DepartmentQualificationLevel,
    ) -> Result<DepartmentQualificationLevel, DomainError> {
        unimplemented!()
    }

    async fn list_levels(
        &self,
        _department_id: &str,
        _qualification_code: Option<&str>,
        _include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationLevel>, DomainError> {
        unimplemented!()
    }
}

#[derive(Clone)]
struct FakeQualificationGrantRepo;

#[async_trait::async_trait]
impl QualificationGrantRepository for FakeQualificationGrantRepo {
    async fn save(&self, _grant: &QualificationGrant) -> Result<QualificationGrant, DomainError> {
        unimplemented!()
    }

    async fn find_by_department(
        &self,
        _department_id: &str,
        _at_time: Option<chrono::DateTime<chrono::Utc>>,
        _user_ids: &[String],
        _include_inactive: bool,
    ) -> Result<Vec<QualificationGrant>, DomainError> {
        unimplemented!()
    }
}

#[derive(Clone)]
struct FakeTaskTypeRequirementRepo;

#[async_trait::async_trait]
impl DepartmentTaskTypeRequirementRepository for FakeTaskTypeRequirementRepo {
    async fn next_version_no(&self, _department_id: &str, _task_type: &str) -> Result<i32, DomainError> {
        unimplemented!()
    }

    async fn save(
        &self,
        _version: &DepartmentTaskTypeRequirementVersion,
    ) -> Result<DepartmentTaskTypeRequirementVersion, DomainError> {
        unimplemented!()
    }

    async fn list_versions(
        &self,
        _department_id: &str,
        _task_type: Option<&str>,
        _status: Option<&str>,
    ) -> Result<Vec<DepartmentTaskTypeRequirementVersion>, DomainError> {
        unimplemented!()
    }

    async fn find_by_id(&self, _version_id: &str) -> Result<Option<DepartmentTaskTypeRequirementVersion>, DomainError> {
        unimplemented!()
    }

    async fn find_latest_draft(
        &self,
        _department_id: &str,
        _task_type: &str,
    ) -> Result<Option<DepartmentTaskTypeRequirementVersion>, DomainError> {
        Ok(None)
    }

    async fn find_published(
        &self,
        _department_id: &str,
        _task_type: &str,
    ) -> Result<Option<DepartmentTaskTypeRequirementVersion>, DomainError> {
        Ok(None)
    }

    async fn archive_published(&self, _department_id: &str, _task_type: &str) -> Result<i64, DomainError> {
        unimplemented!()
    }
}

#[derive(Clone)]
struct FakeGenerationRuleRepo {
    rule: Arc<Mutex<Option<FlightGenerationRule>>>,
}

#[async_trait::async_trait]
impl FlightGenerationRuleRepository for FakeGenerationRuleRepo {
    async fn next_version_no(
        &self,
        _department_id: &str,
        _task_type: &str,
        _leg_scope: &str,
    ) -> Result<i32, DomainError> {
        unimplemented!()
    }

    async fn save(&self, rule: &FlightGenerationRule) -> Result<FlightGenerationRule, DomainError> {
        *self.rule.lock().expect("lock rule") = Some(rule.clone());
        Ok(rule.clone())
    }

    async fn save_replacing_published(
        &self,
        rule: &FlightGenerationRule,
        _previous_rule_id: &str,
    ) -> Result<FlightGenerationRule, DomainError> {
        self.save(rule).await
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<FlightGenerationRule>, DomainError> {
        Ok(self
            .rule
            .lock()
            .expect("lock rule")
            .clone()
            .filter(|rule| rule.id == id))
    }

    async fn list_rules(
        &self,
        _department_id: &str,
        _status: Option<&str>,
    ) -> Result<Vec<FlightGenerationRule>, DomainError> {
        unimplemented!()
    }
}

#[derive(Clone)]
struct FakeAdjustmentRuleRepo;

#[async_trait::async_trait]
impl GenerationAdjustmentRuleRepository for FakeAdjustmentRuleRepo {
    async fn next_version_no(&self, _department_id: &str, _task_type: &str) -> Result<i32, DomainError> {
        unimplemented!()
    }

    async fn save(&self, _rule: &GenerationAdjustmentRule) -> Result<GenerationAdjustmentRule, DomainError> {
        unimplemented!()
    }

    async fn save_replacing_published(
        &self,
        _rule: &GenerationAdjustmentRule,
        _previous_rule_id: &str,
    ) -> Result<GenerationAdjustmentRule, DomainError> {
        unimplemented!()
    }

    async fn find_by_id(&self, _id: &str) -> Result<Option<GenerationAdjustmentRule>, DomainError> {
        Ok(None)
    }

    async fn list_rules(
        &self,
        _department_id: &str,
        _status: Option<&str>,
    ) -> Result<Vec<GenerationAdjustmentRule>, DomainError> {
        unimplemented!()
    }
}

#[derive(Clone)]
struct FakeTemporaryTaskTemplateRepo;

#[async_trait::async_trait]
impl TemporaryTaskTemplateRepository for FakeTemporaryTaskTemplateRepo {
    async fn save(&self, _template: &TemporaryTaskTemplate) -> Result<TemporaryTaskTemplate, DomainError> {
        unimplemented!()
    }

    async fn find_by_code(
        &self,
        _department_id: &str,
        _template_code: &str,
    ) -> Result<Option<TemporaryTaskTemplate>, DomainError> {
        unimplemented!()
    }

    async fn list_templates(
        &self,
        _department_id: &str,
        _include_inactive: bool,
    ) -> Result<Vec<TemporaryTaskTemplate>, DomainError> {
        unimplemented!()
    }
}

fn build_dispatch_rule_service(rule: FlightGenerationRule) -> Arc<DispatchRuleService> {
    Arc::new(DispatchRuleService::new(
        Arc::new(FakeDepartmentRepo),
        Arc::new(FakeQualificationRepo),
        Arc::new(FakeQualificationGrantRepo),
        Arc::new(FakeTaskTypeRequirementRepo),
        Arc::new(FakeGenerationRuleRepo {
            rule: Arc::new(Mutex::new(Some(rule))),
        }),
        Arc::new(FakeAdjustmentRuleRepo),
        Arc::new(FakeTemporaryTaskTemplateRepo),
    ))
}

#[actix_web::test]
async fn delete_generation_rule_returns_python_style_success_envelope() {
    let rule = FlightGenerationRule {
        id: "gen-1".to_string(),
        department_id: "dept-1".to_string(),
        task_type: "service".to_string(),
        leg_scope: LegScope::Inbound,
        version_no: 1,
        status: DepartmentRuleStatus::Published,
        rule_name: Some("Inbound service".to_string()),
        conditions: std::collections::HashMap::new(),
        generation_anchor_type: "arrival".to_string(),
        start_offset_minutes: -15,
        completion_time_mode: "start_plus_duration".to_string(),
        completion_anchor_type: None,
        completion_offset_minutes: None,
        completion_warning_lead_minutes: None,
        duration_minutes: Some(30),
        start_flex_minutes: Some(20),
        duration_by_crew_size: None,
        publication_state: DispatchPublicationState::Prepublished,
        publish_trigger_mode: PublishTriggerMode::Time,
        publish_at: None,
        publish_offset_minutes: None,
        publish_event_code: None,
        notes: None,
        published_at: Some(Utc::now()),
        created_at: None,
        updated_at: None,
    };

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(build_dispatch_rule_service(rule)))
            .configure(configure),
    )
    .await;

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v2/dispatch/rules/departments/dept-1/flight-generation-rules/gen-1/delete")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["success"], true);
    assert_eq!(payload["data"]["message"], "触发规则已删除");
}
