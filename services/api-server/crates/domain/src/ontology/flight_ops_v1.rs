use crate::models::ai_ontology::{
    CompensationMetadata, OntologyActionDef, OntologyActionParameter, OntologyConstraint, OntologyFieldDef,
    OntologyObjectDef, OntologySchema,
};
use crate::ontology::schema_export::FLIGHT_OPS_ONTOLOGY_VERSION;
use serde_json::json;
use std::collections::HashMap;

fn restore_snapshot_compensation(requires_approval: bool) -> Option<CompensationMetadata> {
    Some(CompensationMetadata {
        mode: "restore_snapshot".to_string(),
        requires_approval,
        irreversible_fields: Vec::new(),
        inverse_action_name: None,
        before_snapshot_required: true,
        followup_action_name: None,
        followup_args: None,
    })
}

fn inverse_action_compensation(inverse: &str, requires_approval: bool) -> Option<CompensationMetadata> {
    Some(CompensationMetadata {
        mode: "inverse_action".to_string(),
        requires_approval,
        irreversible_fields: Vec::new(),
        inverse_action_name: Some(inverse.to_string()),
        before_snapshot_required: false,
        followup_action_name: None,
        followup_args: None,
    })
}

/// 只读动作统一定义（见 `docs/architecture/ONTOLOGY_V1.md`：不创建 pending action，直接执行并返回 evidence）。
fn read_action(
    name: &str,
    description: &str,
    parameters: HashMap<String, OntologyActionParameter>,
    parameters_schema: serde_json::Value,
    permission: &str,
) -> OntologyActionDef {
    OntologyActionDef {
        name: name.to_string(),
        description: description.to_string(),
        category: "read".to_string(),
        parameters,
        parameters_schema,
        required_permissions: vec![permission.to_string()],
        risk_level: "low".to_string(),
        approval_strategy: "auto_approve".to_string(),
        approval_policy: "auto_execute".to_string(),
        constraints: vec![],
        execution_mapping: None,
        idempotency_key_strategy: None,
        compensation: None,
    }
}

fn string_param(name: &str, description: &str, required: bool) -> OntologyActionParameter {
    OntologyActionParameter {
        name: name.to_string(),
        param_type: "String".to_string(),
        description: description.to_string(),
        required,
    }
}

/// 建议动作统一定义（见 `docs/architecture/ONTOLOGY_V1.md`：只生成 proposal 载荷，不直接写业务表，
/// 统一经 proposal/pending-action/approval 管线消费，因此不映射 DomainActionExecutor）。
fn advisory_action(
    name: &str,
    description: &str,
    parameters: HashMap<String, OntologyActionParameter>,
    parameters_schema: serde_json::Value,
    permission: &str,
    risk_level: &str,
) -> OntologyActionDef {
    OntologyActionDef {
        name: name.to_string(),
        description: description.to_string(),
        category: "advisory".to_string(),
        parameters,
        parameters_schema,
        required_permissions: vec![permission.to_string()],
        risk_level: risk_level.to_string(),
        approval_strategy: "require_approval".to_string(),
        approval_policy: "require_approval".to_string(),
        constraints: vec![],
        execution_mapping: None,
        idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
        compensation: None,
    }
}

pub fn build_flight_ops_v1_schema() -> OntologySchema {
    let mut objects = HashMap::new();

    // 注：以下新对象按 Spec 顺序添加到 Schema 中 - PR #本体两层改造

    // Terminal Object (航站楼) - 定义层目录，构成事实是成员表
    let terminal_id_str = "terminal".to_string();
    let mut terminal_fields = HashMap::new();
    terminal_fields.insert("terminal_id".to_string(), OntologyFieldDef { name: "terminal_id".to_string(), field_type: "String".to_string(), description: "Unique identifier for the terminal".to_string(), required: true });
    terminal_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Terminal code (T1/T2/T3)".to_string(), required: true });
    terminal_fields.insert("name".to_string(), OntologyFieldDef { name: "name".to_string(), field_type: "String".to_string(), description: "Terminal name".to_string(), required: true });
    terminal_fields.insert("is_active".to_string(), OntologyFieldDef { name: "is_active".to_string(), field_type: "Boolean".to_string(), description: "Whether the terminal is active".to_string(), required: true });

    let mut terminal_actions = HashMap::new();
    terminal_actions.insert("get_context".to_string(), read_action("get_context", "Read the full context of a terminal including its member resources (stands, gates, carousels).", { let mut p = HashMap::new(); p.insert("terminal_id".to_string(), string_param("terminal_id", "Terminal to inspect", true)); p }, json!({"type": "object", "required": ["terminal_id"]}), "ontology:read"));
    terminal_actions.insert("create".to_string(), OntologyActionDef { name: "create".to_string(), description: "Create a terminal definition".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("code".to_string(), string_param("code", "Terminal code like T1", true)); p.insert("name".to_string(), string_param("name", "Terminal display name", true)); p }, parameters_schema: json!({"type": "object", "required": ["code", "name"]}), required_permissions: vec!["ontology:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    terminal_actions.insert("update_profile".to_string(), OntologyActionDef { name: "update_profile".to_string(), description: "Update terminal profile fields (name, is_active)".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("terminal_id".to_string(), string_param("terminal_id", "Terminal to update", true)); p.insert("name".to_string(), string_param("name", "New name", false)); p.insert("is_active".to_string(), string_param("is_active", "New active status", false)); p }, parameters_schema: json!({"type": "object", "required": ["terminal_id"]}), required_permissions: vec!["ontology:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    terminal_actions.insert("deactivate".to_string(), OntologyActionDef { name: "deactivate".to_string(), description: "Deactivate terminal; returns 409 if has active occupations with details".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": []}), required_permissions: vec!["ontology:manage".to_string()], risk_level: "medium".to_string(), approval_strategy: "require_approval".to_string(), approval_policy: "require_approval".to_string(), constraints: vec![OntologyConstraint { constraint_type: "Precondition".to_string(), expression: "no_active_occupations()".to_string(), description: "No active stand/gate/carousel occupations".to_string() }], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    terminal_actions.insert("add_stand".to_string(), OntologyActionDef { name: "add_stand".to_string(), description: "Add a stand to this terminal's member table".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("stand_id".to_string(), string_param("stand_id", "Stand ID to add", true)); p }, parameters_schema: json!({"type": "object", "required": ["stand_id"]}), required_permissions: vec!["ontology:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    terminal_actions.insert("remove_stand".to_string(), OntologyActionDef { name: "remove_stand".to_string(), description: "Remove a stand from terminal; returns 409 if has active occupations".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("stand_id".to_string(), string_param("stand_id", "Stand ID to remove", true)); p }, parameters_schema: json!({"type": "object", "required": ["stand_id"]}), required_permissions: vec!["ontology:manage".to_string()], risk_level: "medium".to_string(), approval_strategy: "require_approval".to_string(), approval_policy: "require_approval".to_string(), constraints: vec![], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    // add_gate/remove_gate/add_carousel/remove_carousel actions follow same pattern
    objects.insert(terminal_id_str, OntologyObjectDef { name: "Terminal".to_string(), description: "Airport terminal building (航站楼)".to_string(), object_id_strategy: "terminal_id".to_string(), fields: terminal_fields, relations: HashMap::new(), actions: terminal_actions });

    // Stand Object (机位)
    let stand_id_str = "Stand".to_string();
    let mut stand_fields = HashMap::new();
    stand_fields.insert("stand_id".to_string(), OntologyFieldDef { name: "stand_id".to_string(), field_type: "String".to_string(), description: "Unique identifier for the stand".to_string(), required: true });
    stand_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Stand code (e.g., A01)".to_string(), required: true });
    stand_fields.insert("is_active".to_string(), OntologyFieldDef { name: "is_active".to_string(), field_type: "Boolean".to_string(), description: "Whether the stand is active".to_string(), required: true });
    let mut stand_actions = HashMap::new();
    stand_actions.insert("get_context".to_string(), read_action("get_context", "Read the full context of a stand including current occupation.", { let mut p = HashMap::new(); p.insert("stand_id".to_string(), string_param("stand_id", "Stand to inspect", true)); p }, json!({"type": "object", "required": ["stand_id"]}), "ontology:read"));
    stand_actions.insert("check_availability".to_string(), read_action("check_availability", "Check stand availability in a time window, with soft conflict warnings (not hard blocks).", { let mut p = HashMap::new(); p.insert("stand_id".to_string(), string_param("stand_id", "Stand id or code", true)); p.insert("time_window".to_string(), string_param("time_window", "ISO8601 time range [start, end]", true)); p }, json!({"type": "object", "required": ["stand_id", "time_window"]}), "flight:read"));
    // create/update_profile/deactivate CRUD actions follow same pattern as Terminal
    objects.insert(stand_id_str, OntologyObjectDef { name: "Stand".to_string(), description: "Airport parking stand (机位)".to_string(), object_id_strategy: "stand_id".to_string(), fields: stand_fields, relations: HashMap::new(), actions: stand_actions });

    // Gate Object (登机口)
    let gate_id_str = "Gate".to_string();
    let mut gate_fields = HashMap::new();
    gate_fields.insert("gate_id".to_string(), OntologyFieldDef { name: "gate_id".to_string(), field_type: "String".to_string(), description: "Unique identifier for the gate".to_string(), required: true });
    gate_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Gate code (e.g., G01)".to_string(), required: true });
    gate_fields.insert("is_active".to_string(), OntologyFieldDef { name: "is_active".to_string(), field_type: "Boolean".to_string(), description: "Whether the gate is active".to_string(), required: true });
    let mut gate_actions = HashMap::new();
    gate_actions.insert("get_context".to_string(), read_action("get_context", "Read the full context of a gate including current assignments.", { let mut p = HashMap::new(); p.insert("gate_id".to_string(), string_param("gate_id", "Gate to inspect", true)); p }, json!({"type": "object", "required": ["gate_id"]}), "ontology:read"));
    // create/update_profile/deactivate/add_to_terminal/remove_from_terminal CRUD actions follow same pattern
    objects.insert(gate_id_str, OntologyObjectDef { name: "Gate".to_string(), description: "Airport departure gate (登机口)".to_string(), object_id_strategy: "gate_id".to_string(), fields: gate_fields, relations: HashMap::new(), actions: gate_actions });

    // BaggageCarousel Object (行李转盘)
    let carousel_id_str = "BaggageCarousel".to_string();
    let mut carousel_fields = HashMap::new();
    carousel_fields.insert("carousel_id".to_string(), OntologyFieldDef { name: "carousel_id".to_string(), field_type: "String".to_string(), description: "Unique identifier for the carousel".to_string(), required: true });
    carousel_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Carousel code (e.g., C01)".to_string(), required: true });
    carousel_fields.insert("is_active".to_string(), OntologyFieldDef { name: "is_active".to_string(), field_type: "Boolean".to_string(), description: "Whether the carousel is active".to_string(), required: true });
    let mut carousel_actions = HashMap::new();
    carousel_actions.insert("get_context".to_string(), read_action("get_context", "Read the full context of a carousel including current assignments.", { let mut p = HashMap::new(); p.insert("carousel_id".to_string(), string_param("carousel_id", "Carousel to inspect", true)); p }, json!({"type": "object", "required": ["carousel_id"]}), "ontology:read"));
    // create/update_profile/deactivate/add_to_terminal/remove_from_terminal CRUD actions follow same pattern
    objects.insert(carousel_id_str, OntologyObjectDef { name: "BaggageCarousel".to_string(), description: "Baggage claim carousel (行李转盘)".to_string(), object_id_strategy: "carousel_id".to_string(), fields: carousel_fields, relations: HashMap::new(), actions: carousel_actions });

    // StandOccupation Object (机位占用)
    let stand_occ_id_str = "StandOccupation".to_string();
    let mut stand_occ_fields = HashMap::new();
    stand_occ_fields.insert("occupation_id".to_string(), OntologyFieldDef { name: "occupation_id".to_string(), field_type: "String".to_string(), description: "Unique identifier for the occupation".to_string(), required: true });
    stand_occ_fields.insert("stand_id".to_string(), OntologyFieldDef { name: "stand_id".to_string(), field_type: "String".to_string(), description: "Stand identifier".to_string(), required: true });
    stand_occ_fields.insert("registration".to_string(), OntologyFieldDef { name: "registration".to_string(), field_type: "String".to_string(), description: "Aircraft registration".to_string(), required: true });
    stand_occ_fields.insert("flight_id".to_string(), OntologyFieldDef { name: "flight_id".to_string(), field_type: "String".to_string(), description: "Related flight (optional)".to_string(), required: false });
    stand_occ_fields.insert("starts_at".to_string(), OntologyFieldDef { name: "starts_at".to_string(), field_type: "String".to_string(), description: "ISO8601 start time".to_string(), required: true });
    stand_occ_fields.insert("ends_at".to_string(), OntologyFieldDef { name: "ends_at".to_string(), field_type: "String".to_string(), description: "ISO8601 end time".to_string(), required: true });
    stand_occ_fields.insert("status".to_string(), OntologyFieldDef { name: "status".to_string(), field_type: "String".to_string(), description: "draft|active|released".to_string(), required: true });
    let mut stand_occ_actions = HashMap::new();
    stand_occ_actions.insert("get_context".to_string(), read_action("get_context", "Read occupation details.", { let mut p = HashMap::new(); p.insert("occupation_id".to_string(), string_param("occupation_id", "Occupation to inspect", true)); p }, json!({"type": "object", "required": ["occupation_id"]}), "ontology:read"));
    stand_occ_actions.insert("allocate".to_string(), OntologyActionDef { name: "allocate".to_string(), description: "Allocate a stand to an aircraft (hard constraint: stand must exist and be in active terminal)".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("stand_code".to_string(), string_param("stand_code", "Stand code", true)); p.insert("registration".to_string(), string_param("registration", "Aircraft registration", false)); p.insert("starts_at".to_string(), string_param("starts_at", "ISO8601 start", true)); p.insert("ends_at".to_string(), string_param("ends_at", "ISO8601 end", true)); p.insert("client_action_id".to_string(), string_param("client_action_id", "Idempotency token", false)); p }, parameters_schema: json!({"type": "object", "required": ["stand_code", "registration", "starts_at", "ends_at"]}), required_permissions: vec!["ontology:stand.manage".to_string()], risk_level: "medium".to_string(), approval_strategy: "require_approval".to_string(), approval_policy: "require_approval".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.StandOccupation.allocate".to_string()), idempotency_key_strategy: Some("client_action_id".to_string()), compensation: None });
    stand_occ_actions.insert("adjust".to_string(), OntologyActionDef { name: "adjust".to_string(), description: "Adjust occupation time window or registration".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": []}), required_permissions: vec!["ontology:stand.manage".to_string()], risk_level: "medium".to_string(), approval_strategy: "require_approval".to_string(), approval_policy: "require_approval".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.StandOccupation.adjust".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    stand_occ_actions.insert("release".to_string(), OntologyActionDef { name: "release".to_string(), description: "Release the occupation".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": []}), required_permissions: vec!["ontology:stand.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.StandOccupation.release".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: inverse_action_compensation("allocate", true) });
    objects.insert(stand_occ_id_str, OntologyObjectDef { name: "StandOccupation".to_string(), description: "Aircraft occupation of a stand".to_string(), object_id_strategy: "occupation_id".to_string(), fields: stand_occ_fields, relations: HashMap::new(), actions: stand_occ_actions });

    // GateAssignment Object (登机口分配)
    let gate_assign_id_str = "GateAssignment".to_string();
    let mut gate_assign_fields = HashMap::new();
    gate_assign_fields.insert("assignment_id".to_string(), OntologyFieldDef { name: "assignment_id".to_string(), field_type: "String".to_string(), description: "Unique identifier".to_string(), required: true });
    gate_assign_fields.insert("gate_id".to_string(), OntologyFieldDef { name: "gate_id".to_string(), field_type: "String".to_string(), description: "Gate identifier".to_string(), required: true });
    gate_assign_fields.insert("flight_id".to_string(), OntologyFieldDef { name: "flight_id".to_string(), field_type: "String".to_string(), description: "Flight identifier (REQUIRED, not optional)".to_string(), required: true });
    gate_assign_fields.insert("starts_at".to_string(), OntologyFieldDef { name: "starts_at".to_string(), field_type: "String".to_string(), description: "ISO8601 start time".to_string(), required: true });
    gate_assign_fields.insert("ends_at".to_string(), OntologyFieldDef { name: "ends_at".to_string(), field_type: "String".to_string(), description: "ISO8601 end time".to_string(), required: true });
    gate_assign_fields.insert("status".to_string(), OntologyFieldDef { name: "status".to_string(), field_type: "String".to_string(), description: "draft|active|released".to_string(), required: true });
    let mut gate_assign_actions = HashMap::new();
    gate_assign_actions.insert("get_context".to_string(), read_action("get_context", "Read assignment details.", { let mut p = HashMap::new(); p.insert("assignment_id".to_string(), string_param("assignment_id", "Assignment to inspect", true)); p }, json!({"type": "object", "required": ["assignment_id"]}), "ontology:read"));
    gate_assign_actions.insert("allocate".to_string(), OntologyActionDef { name: "allocate".to_string(), description: "Assign a gate to a flight (subject: flight_id REQUIRED)".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("gate_code".to_string(), string_param("gate_code", "Gate code", true)); p.insert("flight_id".to_string(), string_param("flight_id", "Flight ID", true)); p.insert("starts_at".to_string(), string_param("starts_at", "ISO8601 start", true)); p.insert("ends_at".to_string(), string_param("ends_at", "ISO8601 end", true)); p.insert("client_action_id".to_string(), string_param("client_action_id", "Idempotency token", false)); p }, parameters_schema: json!({"type": "object", "required": ["gate_code", "flight_id", "starts_at", "ends_at"]}), required_permissions: vec!["ontology:gate.manage".to_string()], risk_level: "medium".to_string(), approval_strategy: "require_approval".to_string(), approval_policy: "require_approval".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.GateAssignment.allocate".to_string()), idempotency_key_strategy: Some("client_action_id".to_string()), compensation: None });
    gate_assign_actions.insert("release".to_string(), OntologyActionDef { name: "release".to_string(), description: "Release the gate assignment".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": []}), required_permissions: vec!["ontology:gate.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.GateAssignment.release".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: inverse_action_compensation("allocate", true) });
    objects.insert(gate_assign_id_str, OntologyObjectDef { name: "GateAssignment".to_string(), description: "Flight assignment to a departure gate".to_string(), object_id_strategy: "assignment_id".to_string(), fields: gate_assign_fields, relations: HashMap::new(), actions: gate_assign_actions });

    // CarouselAssignment Object (行李转盘分配)
    let carousel_assign_id_str = "CarouselAssignment".to_string();
    let mut carousel_assign_fields = HashMap::new();
    carousel_assign_fields.insert("assignment_id".to_string(), OntologyFieldDef { name: "assignment_id".to_string(), field_type: "String".to_string(), description: "Unique identifier".to_string(), required: true });
    carousel_assign_fields.insert("carousel_id".to_string(), OntologyFieldDef { name: "carousel_id".to_string(), field_type: "String".to_string(), description: "Carousel identifier".to_string(), required: true });
    carousel_assign_fields.insert("flight_id".to_string(), OntologyFieldDef { name: "flight_id".to_string(), field_type: "String".to_string(), description: "Flight identifier".to_string(), required: true });
    carousel_assign_fields.insert("starts_at".to_string(), OntologyFieldDef { name: "starts_at".to_string(), field_type: "String".to_string(), description: "ISO8601 start time".to_string(), required: true });
    carousel_assign_fields.insert("ends_at".to_string(), OntologyFieldDef { name: "ends_at".to_string(), field_type: "String".to_string(), description: "ISO8601 end time".to_string(), required: true });
    carousel_assign_fields.insert("status".to_string(), OntologyFieldDef { name: "status".to_string(), field_type: "String".to_string(), description: "draft|active|released".to_string(), required: true });
    let mut carousel_assign_actions = HashMap::new();
    carousel_assign_actions.insert("get_context".to_string(), read_action("get_context", "Read assignment details.", { let mut p = HashMap::new(); p.insert("assignment_id".to_string(), string_param("assignment_id", "Assignment to inspect", true)); p }, json!({"type": "object", "required": ["assignment_id"]}), "ontology:read"));
    carousel_assign_actions.insert("allocate".to_string(), OntologyActionDef { name: "allocate".to_string(), description: "Assign carousel to flight (NO business constraints - unlimited concurrent assignments allowed)".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("carousel_code".to_string(), string_param("carousel_code", "Carousel code", true)); p.insert("flight_id".to_string(), string_param("flight_id", "Flight ID", true)); p.insert("starts_at".to_string(), string_param("starts_at", "ISO8601 start", true)); p.insert("ends_at".to_string(), string_param("ends_at", "ISO8601 end", true)); p.insert("client_action_id".to_string(), string_param("client_action_id", "Idempotency token", false)); p }, parameters_schema: json!({"type": "object", "required": ["carousel_code", "flight_id", "starts_at", "ends_at"]}), required_permissions: vec!["ontology:carousel.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.CarouselAssignment.allocate".to_string()), idempotency_key_strategy: Some("client_action_id".to_string()), compensation: None });
    carousel_assign_actions.insert("release".to_string(), OntologyActionDef { name: "release".to_string(), description: "Release the carousel assignment".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": []}), required_permissions: vec!["ontology:carousel.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.CarouselAssignment.release".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: inverse_action_compensation("allocate", true) });
    objects.insert(carousel_assign_id_str, OntologyObjectDef { name: "CarouselAssignment".to_string(), description: "Flight baggage claim carousel assignment (no constraints)".to_string(), object_id_strategy: "assignment_id".to_string(), fields: carousel_assign_fields, relations: HashMap::new(), actions: carousel_assign_actions });

    // Department Object (科室)
    let dept_id_str = "Department".to_string();
    let mut dept_fields = HashMap::new();
    dept_fields.insert("department_id".to_string(), OntologyFieldDef { name: "department_id".to_string(), field_type: "String".to_string(), description: "Unique identifier".to_string(), required: true });
    dept_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Department code".to_string(), required: true });
    dept_fields.insert("name".to_string(), OntologyFieldDef { name: "name".to_string(), field_type: "String".to_string(), description: "Department name".to_string(), required: true });
    dept_fields.insert("manager_id".to_string(), OntologyFieldDef { name: "manager_id".to_string(), field_type: "String".to_string(), description: "Manager user_id".to_string(), required: false });
    dept_fields.insert("is_active".to_string(), OntologyFieldDef { name: "is_active".to_string(), field_type: "Boolean".to_string(), description: "Active status".to_string(), required: true });
    let mut dept_actions = HashMap::new();
    dept_actions.insert("get_context".to_string(), read_action("get_context", "Read department details including teams and equipment.", { let mut p = HashMap::new(); p.insert("department_id".to_string(), string_param("department_id", "Department to inspect", true)); p }, json!({"type": "object", "required": ["department_id"]}), "ontology:read"));
    dept_actions.insert("create".to_string(), OntologyActionDef { name: "create".to_string(), description: "Create a department".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("code".to_string(), string_param("code", "Department code", true)); p.insert("name".to_string(), string_param("name", "Department name", true)); p }, parameters_schema: json!({"type": "object", "required": ["code", "name"]}), required_permissions: vec!["ontology:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    dept_actions.insert("update_profile".to_string(), OntologyActionDef { name: "update_profile".to_string(), description: "Update department profile".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": ["department_id"]}), required_permissions: vec!["ontology:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    objects.insert(dept_id_str, OntologyObjectDef { name: "Department".to_string(), description: "Operational department (科室)".to_string(), object_id_strategy: "department_id".to_string(), fields: dept_fields, relations: HashMap::new(), actions: dept_actions });

    // Team Object (班组 - 在岗名册)
    let team_id_str = "Team".to_string();
    let mut team_fields = HashMap::new();
    team_fields.insert("team_id".to_string(), OntologyFieldDef { name: "team_id".to_string(), field_type: "String".to_string(), description: "Unique identifier".to_string(), required: true });
    team_fields.insert("name".to_string(), OntologyFieldDef { name: "name".to_string(), field_type: "String".to_string(), description: "Team name".to_string(), required: true });
    team_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Team code".to_string(), required: false });
    team_fields.insert("department_id".to_string(), OntologyFieldDef { name: "department_id".to_string(), field_type: "String".to_string(), description: "所属科室（名册边界）".to_string(), required: true });
    team_fields.insert("current_status".to_string(), OntologyFieldDef { name: "current_status".to_string(), field_type: "String".to_string(), description: "on_duty|off_duty|break".to_string(), required: true });
    team_fields.insert("members".to_string(), OntologyFieldDef { name: "members".to_string(), field_type: "Array".to_string(), description: "名册成员 user_id 列表".to_string(), required: false });
    let mut team_actions = HashMap::new();
    team_actions.insert("get_context".to_string(), read_action("get_context", "Read team context including department and roster members.", { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team to inspect", true)); p }, json!({"type": "object", "required": ["team_id"]}), "ontology:read"));
    team_actions.insert("update_status".to_string(), OntologyActionDef { name: "update_status".to_string(), description: "代签：更新班组在岗状态并可选同步全量名册（同一事务 + outbox）。科室经理或 admin 可改；入组只收个人账号且同科室，班组非工单指派对象。".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team to update", true)); p.insert("current_status".to_string(), string_param("current_status", "on_duty|off_duty|break", true)); p.insert("member_user_ids".to_string(), string_param("member_user_ids", "全量名册 user_id 列表（代签勾选）", false)); p }, parameters_schema: json!({"type": "object", "required": ["team_id", "current_status"], "properties": {"member_user_ids": {"type": "array", "items": {"type": "string"}}}}), required_permissions: vec!["ontology:team.manage".to_string()], risk_level: "medium".to_string(), approval_strategy: "require_approval".to_string(), approval_policy: "require_approval".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Team.update_status".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    team_actions.insert("change_location".to_string(), OntologyActionDef { name: "change_location".to_string(), description: "Update team current location (does not propagate to personnel)".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team to update", true)); p.insert("lat".to_string(), string_param("lat", "Latitude", true)); p.insert("lng".to_string(), string_param("lng", "Longitude", true)); p }, parameters_schema: json!({"type": "object", "required": ["team_id", "lat", "lng"]}), required_permissions: vec!["ontology:team.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Team.change_location".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    team_actions.insert("add_member".to_string(), OntologyActionDef { name: "add_member".to_string(), description: "入组：必须是个人账号且科室与班组相同，一人一条活跃 team_members；岗位账号入组 409。".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team to update", true)); p.insert("user_id".to_string(), string_param("user_id", "Personal account user_id", true)); p }, parameters_schema: json!({"type": "object", "required": ["team_id", "user_id"]}), required_permissions: vec!["ontology:team.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Team.add_member".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    team_actions.insert("remove_member".to_string(), OntologyActionDef { name: "remove_member".to_string(), description: "出组（班组名册）。".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team to update", true)); p.insert("user_id".to_string(), string_param("user_id", "Member to remove", true)); p }, parameters_schema: json!({"type": "object", "required": ["team_id", "user_id"]}), required_permissions: vec!["ontology:team.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Team.remove_member".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    objects.insert(team_id_str, OntologyObjectDef { name: "Team".to_string(), description: "班组：在岗名册，不是工单指派对象".to_string(), object_id_strategy: "team_id".to_string(), fields: team_fields, relations: HashMap::new(), actions: team_actions });

    // Personnel Object (作业人员 - 个人账号对应行)
    let personnel_id_str = "Personnel".to_string();
    let mut personnel_fields = HashMap::new();
    personnel_fields.insert("user_id".to_string(), OntologyFieldDef { name: "user_id".to_string(), field_type: "String".to_string(), description: "User ID (must be personal account type)".to_string(), required: true });
    personnel_fields.insert("name".to_string(), OntologyFieldDef { name: "name".to_string(), field_type: "String".to_string(), description: "Personal name".to_string(), required: true });
    personnel_fields.insert("department_id".to_string(), OntologyFieldDef { name: "department_id".to_string(), field_type: "String".to_string(), description: "所属科室".to_string(), required: true });
    let mut personnel_actions = HashMap::new();
    personnel_actions.insert("get_context".to_string(), read_action("get_context", "Read personnel details including qualifications and runtime status.", { let mut p = HashMap::new(); p.insert("user_id".to_string(), string_param("user_id", "User ID", true)); p }, json!({"type": "object", "required": ["user_id"]}), "ontology:read"));
    personnel_actions.insert("update_status".to_string(), OntologyActionDef { name: "update_status".to_string(), description: "更新该人 on_duty/off_duty/break/on_leave。本人可改；改别人须科室经理或 admin。".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("user_id".to_string(), string_param("user_id", "User ID", true)); p.insert("current_status".to_string(), string_param("current_status", "on_duty|off_duty|break|on_leave", true)); p }, parameters_schema: json!({"type": "object", "required": ["user_id", "current_status"]}), required_permissions: vec!["ontology:personnel.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Personnel.update_status".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    personnel_actions.insert("change_location".to_string(), OntologyActionDef { name: "change_location".to_string(), description: "更新该人位置（只本人；改别人须科室经理或 admin）。".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("user_id".to_string(), string_param("user_id", "User ID", true)); p.insert("lat".to_string(), string_param("lat", "Latitude", true)); p.insert("lng".to_string(), string_param("lng", "Longitude", true)); p }, parameters_schema: json!({"type": "object", "required": ["user_id", "lat", "lng"]}), required_permissions: vec!["ontology:personnel.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Personnel.change_location".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    personnel_actions.insert("assign_to_team".to_string(), OntologyActionDef { name: "assign_to_team".to_string(), description: "把个人加入某班组名册（入组校验同 Team.add_member）。".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("user_id".to_string(), string_param("user_id", "User ID", true)); p.insert("team_id".to_string(), string_param("team_id", "Team ID", true)); p }, parameters_schema: json!({"type": "object", "required": ["user_id", "team_id"]}), required_permissions: vec!["ontology:personnel.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Personnel.assign_to_team".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    personnel_actions.insert("leave_team".to_string(), OntologyActionDef { name: "leave_team".to_string(), description: "把个人移出班组名册。".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("user_id".to_string(), string_param("user_id", "User ID", true)); p }, parameters_schema: json!({"type": "object", "required": ["user_id"]}), required_permissions: vec!["ontology:personnel.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Personnel.leave_team".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    objects.insert(personnel_id_str, OntologyObjectDef { name: "Personnel".to_string(), description: "Operational personnel (作业身份) - only for personal accounts".to_string(), object_id_strategy: "user_id".to_string(), fields: personnel_fields, relations: HashMap::new(), actions: personnel_actions });

    // Equipment Object (保障设备)
    let equipment_id_str = "Equipment".to_string();
    let mut equipment_fields = HashMap::new();
    equipment_fields.insert("equipment_id".to_string(), OntologyFieldDef { name: "equipment_id".to_string(), field_type: "String".to_string(), description: "Unique identifier".to_string(), required: true });
    equipment_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Equipment code".to_string(), required: true });
    equipment_fields.insert("name".to_string(), OntologyFieldDef { name: "name".to_string(), field_type: "String".to_string(), description: "Equipment name".to_string(), required: false });
    equipment_fields.insert("department_id".to_string(), OntologyFieldDef { name: "department_id".to_string(), field_type: "String".to_string(), description: "所属科室".to_string(), required: true });
    equipment_fields.insert("equipment_type_id".to_string(), OntologyFieldDef { name: "equipment_type_id".to_string(), field_type: "String".to_string(), description: "设备类型".to_string(), required: true });
    equipment_fields.insert("status".to_string(), OntologyFieldDef { name: "status".to_string(), field_type: "String".to_string(), description: "available|in_use|maintenance|retired".to_string(), required: true });
    let mut equipment_actions = HashMap::new();
    equipment_actions.insert("get_context".to_string(), read_action("get_context", "Read equipment details including type and department.", { let mut p = HashMap::new(); p.insert("equipment_id".to_string(), string_param("equipment_id", "Equipment to inspect", true)); p }, json!({"type": "object", "required": ["equipment_id"]}), "ontology:read"));
    equipment_actions.insert("update_status".to_string(), OntologyActionDef { name: "update_status".to_string(), description: "Update equipment status".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("equipment_id".to_string(), string_param("equipment_id", "Equipment to update", true)); p.insert("status".to_string(), string_param("status", "available|in_use|maintenance|retired", true)); p }, parameters_schema: json!({"type": "object", "required": ["equipment_id", "status"]}), required_permissions: vec!["ontology:equipment.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Equipment.update_status".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    equipment_actions.insert("change_location".to_string(), OntologyActionDef { name: "change_location".to_string(), description: "Update equipment current location".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("equipment_id".to_string(), string_param("equipment_id", "Equipment to update", true)); p.insert("lat".to_string(), string_param("lat", "Latitude", true)); p.insert("lng".to_string(), string_param("lng", "Longitude", true)); p }, parameters_schema: json!({"type": "object", "required": ["equipment_id", "lat", "lng"]}), required_permissions: vec!["ontology:equipment.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Equipment.change_location".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    equipment_actions.insert("assign".to_string(), OntologyActionDef { name: "assign".to_string(), description: "把设备指派到工单设备槽（与人员槽同一套领域模型）。".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("equipment_id".to_string(), string_param("equipment_id", "Equipment ID", true)); p.insert("dispatch_order_id".to_string(), string_param("dispatch_order_id", "工单 ID", true)); p.insert("slot_code".to_string(), string_param("slot_code", "工单设备槽编码", true)); p }, parameters_schema: json!({"type": "object", "required": ["equipment_id", "dispatch_order_id", "slot_code"]}), required_permissions: vec!["ontology:equipment.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Equipment.assign".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    equipment_actions.insert("release".to_string(), OntologyActionDef { name: "release".to_string(), description: "把设备从工单设备槽释放。".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("equipment_id".to_string(), string_param("equipment_id", "Equipment ID", true)); p.insert("dispatch_order_id".to_string(), string_param("dispatch_order_id", "工单 ID", false)); p.insert("slot_code".to_string(), string_param("slot_code", "工单设备槽编码", false)); p }, parameters_schema: json!({"type": "object", "required": ["equipment_id"]}), required_permissions: vec!["ontology:equipment.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Equipment.release".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    objects.insert(equipment_id_str, OntologyObjectDef { name: "Equipment".to_string(), description: "Ground handling equipment (保障设备)".to_string(), object_id_strategy: "equipment_id".to_string(), fields: equipment_fields, relations: HashMap::new(), actions: equipment_actions });

    // EquipmentType Object (设备类型目录)
    let equip_type_id_str = "EquipmentType".to_string();
    let mut equip_type_fields = HashMap::new();
    equip_type_fields.insert("equipment_type_id".to_string(), OntologyFieldDef { name: "equipment_type_id".to_string(), field_type: "String".to_string(), description: "Unique identifier".to_string(), required: true });
    equip_type_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Equipment type code".to_string(), required: true });
    equip_type_fields.insert("name".to_string(), OntologyFieldDef { name: "name".to_string(), field_type: "String".to_string(), description: "Equipment type name".to_string(), required: true });
    equip_type_fields.insert("requires_driver".to_string(), OntologyFieldDef { name: "requires_driver".to_string(), field_type: "Boolean".to_string(), description: "Requires qualified driver".to_string(), required: true });
    equip_type_fields.insert("is_active".to_string(), OntologyFieldDef { name: "is_active".to_string(), field_type: "Boolean".to_string(), description: "Active status".to_string(), required: true });
    let mut equip_type_actions = HashMap::new();
    equip_type_actions.insert("get_context".to_string(), read_action("get_context", "Read equipment type details.", { let mut p = HashMap::new(); p.insert("equipment_type_id".to_string(), string_param("equipment_type_id", "Type to inspect", true)); p }, json!({"type": "object", "required": ["equipment_type_id"]}), "ontology:read"));
    equip_type_actions.insert("create".to_string(), OntologyActionDef { name: "create".to_string(), description: "Create an equipment type".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": []}), required_permissions: vec!["ontology:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    objects.insert(equip_type_id_str, OntologyObjectDef { name: "EquipmentType".to_string(), description: "Ground handling equipment type classification".to_string(), object_id_strategy: "equipment_type_id".to_string(), fields: equip_type_fields, relations: HashMap::new(), actions: equip_type_actions });

    // TaskType Object (作业类型)
    let task_type_id_str = "TaskType".to_string();
    let mut task_type_fields = HashMap::new();
    task_type_fields.insert("task_type_id".to_string(), OntologyFieldDef { name: "task_type_id".to_string(), field_type: "String".to_string(), description: "Unique identifier".to_string(), required: true });
    task_type_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Task type code".to_string(), required: true });
    task_type_fields.insert("name".to_string(), OntologyFieldDef { name: "name".to_string(), field_type: "String".to_string(), description: "Task name (e.g., 接机/客梯/清洁)".to_string(), required: true });
    task_type_fields.insert("is_active".to_string(), OntologyFieldDef { name: "is_active".to_string(), field_type: "Boolean".to_string(), description: "Active status".to_string(), required: true });
    let mut task_type_actions = HashMap::new();
    task_type_actions.insert("get_context".to_string(), read_action("get_context", "Read task type details.", { let mut p = HashMap::new(); p.insert("task_type_id".to_string(), string_param("task_type_id", "Type to inspect", true)); p }, json!({"type": "object", "required": ["task_type_id"]}), "ontology:read"));
    objects.insert(task_type_id_str, OntologyObjectDef { name: "TaskType".to_string(), description: "Ground handling task type directory".to_string(), object_id_strategy: "task_type_id".to_string(), fields: task_type_fields, relations: HashMap::new(), actions: task_type_actions });

    // Aircraft Object (运行飞机对象)
    let aircraft_id_str = "Aircraft".to_string();
    let mut aircraft_fields = HashMap::new();
    aircraft_fields.insert("registration".to_string(), OntologyFieldDef { name: "registration".to_string(), field_type: "String".to_string(), description: "Aircraft registration (唯一标识)".to_string(), required: true });
    let mut aircraft_actions = HashMap::new();
    aircraft_actions.insert("get_context".to_string(), read_action("get_context", "Read aircraft details including current occupation.", { let mut p = HashMap::new(); p.insert("registration".to_string(), string_param("registration", "Registration number", true)); p }, json!({"type": "object", "required": ["registration"]}), "ontology:read"));
    aircraft_actions.insert("reassign".to_string(), OntologyActionDef { name: "reassign".to_string(), description: "Reassign aircraft to a different stand/gate".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": []}), required_permissions: vec!["ontology:aircraft.manage".to_string()], risk_level: "medium".to_string(), approval_strategy: "require_approval".to_string(), approval_policy: "require_approval".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Aircraft.reassign".to_string()), idempotency_key_strategy: Some("client_action_id".to_string()), compensation: None });
    objects.insert(aircraft_id_str, OntologyObjectDef { name: "Aircraft".to_string(), description: "Running aircraft object - not in resource management".to_string(), object_id_strategy: "registration".to_string(), fields: aircraft_fields, relations: HashMap::new(), actions: aircraft_actions });

    // TurnaroundLink Object (周转链 - 连接进出港航班)
    let turnaround_id_str = "TurnaroundLink".to_string();
    let mut turnaround_fields = HashMap::new();
    turnaround_fields.insert("link_id".to_string(), OntologyFieldDef { name: "link_id".to_string(), field_type: "String".to_string(), description: "Unique identifier".to_string(), required: true });
    turnaround_fields.insert("inbound_flight_id".to_string(), OntologyFieldDef { name: "inbound_flight_id".to_string(), field_type: "String".to_string(), description: "Inbound flight ID".to_string(), required: true });
    turnaround_fields.insert("outbound_flight_id".to_string(), OntologyFieldDef { name: "outbound_flight_id".to_string(), field_type: "String".to_string(), description: "Outbound flight ID".to_string(), required: true });
    let mut turnaround_actions = HashMap::new();
    turnaround_actions.insert("get_context".to_string(), read_action("get_context", "Read turnaround link details.", { let mut p = HashMap::new(); p.insert("link_id".to_string(), string_param("link_id", "Link to inspect", true)); p }, json!({"type": "object", "required": ["link_id"]}), "ontology:read"));
    turnaround_actions.insert("create".to_string(), OntologyActionDef { name: "create".to_string(), description: "Create a turnaround link between two flights".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": []}), required_permissions: vec!["ontology:turnaround.create".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    turnaround_actions.insert("break".to_string(), OntologyActionDef { name: "break".to_string(), description: "Break the turnaround link".to_string(), category: "write".to_string(), parameters: HashMap::new(), parameters_schema: json!({"type": "object", "required": []}), required_permissions: vec!["ontology:turnaround.manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: None, idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    objects.insert(turnaround_id_str, OntologyObjectDef { name: "TurnaroundLink".to_string(), description: "Turnaround chain connecting inbound/outbound flights (same aircraft, NOT passenger connection)".to_string(), object_id_strategy: "link_id".to_string(), fields: turnaround_fields, relations: HashMap::new(), actions: turnaround_actions });
    let mut flight_fields = HashMap::new();
    flight_fields.insert(
        "flight_id".to_string(),
        OntologyFieldDef {
            name: "flight_id".to_string(),
            field_type: "String".to_string(),
            description: "Unique identifier for the flight".to_string(),
            required: true,
        },
    );
    flight_fields.insert(
        "flight_number".to_string(),
        OntologyFieldDef {
            name: "flight_number".to_string(),
            field_type: "String".to_string(),
            description: "Flight number".to_string(),
            required: true,
        },
    );
    flight_fields.insert(
        "status".to_string(),
        OntologyFieldDef {
            name: "status".to_string(),
            field_type: "String".to_string(),
            description: "Current status of the flight".to_string(),
            required: true,
        },
    );

    let mut flight_actions = HashMap::new();
    flight_actions.insert(
        "get_context".to_string(),
        OntologyActionDef {
            name: "get_context".to_string(),
            description: "Read the governed context snapshot for a flight".to_string(),
            category: "read".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "flight_id".to_string(),
                    OntologyActionParameter {
                        name: "flight_id".to_string(),
                        param_type: "String".to_string(),
                        description: "The flight to inspect".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["flight_id"]}),
            required_permissions: vec!["flight:read".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: None,
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
    flight_actions.insert(
        "update_status".to_string(),
        OntologyActionDef {
            name: "update_status".to_string(),
            description: "Update the flight status".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "new_status".to_string(),
                    OntologyActionParameter {
                        name: "new_status".to_string(),
                        param_type: "String".to_string(),
                        description: "The new status to apply".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["new_status"]}),
            required_permissions: vec!["flight:write".to_string()],
            risk_level: "high".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Flight.update_status".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: restore_snapshot_compensation(true),
        },
    );
    // `Flight.change_stand` 已废止 - PR #本体两层改造。展示用 stand 列只由 StandOccupation 占用回写。

    flight_actions.insert(
        "add_note".to_string(),
        OntologyActionDef {
            name: "add_note".to_string(),
            description: "Add a note to the flight".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "note_content".to_string(),
                    OntologyActionParameter {
                        name: "note_content".to_string(),
                        param_type: "String".to_string(),
                        description: "Content of the note".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["note_content"]}),
            required_permissions: vec!["flight:write".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Flight.add_note".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );

    // `Flight.add_label`：替代已删除的 Label.add 对象，受控写动作。
    flight_actions.insert(
        "add_label".to_string(),
        OntologyActionDef {
            name: "add_label".to_string(),
            description: "Attach a label definition code to a flight (replaces deprecated Label object)".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "label_code".to_string(),
                    string_param("label_code", "Label definition code to attach", true),
                );
                p.insert(
                    "reason".to_string(),
                    string_param("reason", "Optional reason for adding label", false),
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["label_code"]}),
            required_permissions: vec!["flight:write".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Flight.add_label".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );

    // `Flight.update_delay`：至少一个预计时间，受控写动作。
    flight_actions.insert(
        "update_delay".to_string(),
        OntologyActionDef {
            name: "update_delay".to_string(),
            description: "Update estimated departure/arrival times for a delayed flight (controlled write)."
                .to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "flight_id".to_string(),
                    string_param("flight_id", "Delayed flight", true),
                );
                p.insert(
                    "estimated_departure".to_string(),
                    string_param("estimated_departure", "RFC3339 estimated departure", false),
                );
                p.insert(
                    "estimated_arrival".to_string(),
                    string_param("estimated_arrival", "RFC3339 estimated arrival", false),
                );
                p.insert("reason".to_string(), string_param("reason", "Delay reason", false));
                p
            },
            parameters_schema: json!({"type": "object", "required": ["flight_id"]}),
            required_permissions: vec!["flight:write".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Flight.update_delay".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: restore_snapshot_compensation(true),
        },
    );

    flight_actions.insert(
        "suggest_stand_adjustment".to_string(),
        advisory_action(
            "suggest_stand_adjustment",
            "Generate a stand-change proposal for a flight (before/after preview, overlap warnings). No business write.",
            {
                let mut p = HashMap::new();
                p.insert("flight_id".to_string(), string_param("flight_id", "Flight to adjust", true));
                p.insert("new_stand_id".to_string(), string_param("new_stand_id", "Optional target stand; auto-scan when omitted", false));
                p
            },
            json!({"type": "object", "required": ["flight_id"]}),
            "flight:read",
            "medium",
        ),
    );
    flight_actions.insert(
        "suggest_delay_action".to_string(),
        advisory_action(
            "suggest_delay_action",
            "Generate a delay-handling proposal for a flight with impacted dispatch orders. No business write.",
            {
                let mut p = HashMap::new();
                p.insert(
                    "flight_id".to_string(),
                    string_param("flight_id", "Delayed flight", true),
                );
                p.insert(
                    "new_estimated_departure".to_string(),
                    string_param(
                        "new_estimated_departure",
                        "RFC3339 new departure; default current +30min",
                        false,
                    ),
                );
                p
            },
            json!({"type": "object", "required": ["flight_id"]}),
            "flight:read",
            "medium",
        ),
    );

    flight_actions.insert(
        "search".to_string(),
        read_action(
            "search",
            "Search flights by flight number, status, origin, destination, date or open-anomaly flag (limit <= 200).",
            {
                let mut p = HashMap::new();
                p.insert(
                    "flight_no".to_string(),
                    string_param("flight_no", "Flight number filter", false),
                );
                p.insert("status".to_string(), string_param("status", "Status filter", false));
                p.insert(
                    "origin".to_string(),
                    string_param("origin", "Origin airport code", false),
                );
                p.insert(
                    "destination".to_string(),
                    string_param("destination", "Destination airport code", false),
                );
                p.insert(
                    "date".to_string(),
                    string_param("date", "Operating date YYYY-MM-DD", false),
                );
                p
            },
            json!({"type": "object", "required": []}),
            "flight:read",
        ),
    );

    objects.insert(
        "Flight".to_string(),
        OntologyObjectDef {
            name: "Flight".to_string(),
            description: "A commercial flight operation".to_string(),
            object_id_strategy: "flight_id".to_string(),
            fields: flight_fields,
            relations: HashMap::new(),
            actions: flight_actions,
        },
    );

    // 2. Stand Object
    let mut stand_fields = HashMap::new();
    stand_fields.insert(
        "stand_id".to_string(),
        OntologyFieldDef {
            name: "stand_id".to_string(),
            field_type: "String".to_string(),
            description: "Unique identifier for the stand".to_string(),
            required: true,
        },
    );
    stand_fields.insert(
        "is_available".to_string(),
        OntologyFieldDef {
            name: "is_available".to_string(),
            field_type: "Boolean".to_string(),
            description: "Whether the stand is currently available".to_string(),
            required: true,
        },
    );

    let mut stand_actions = HashMap::new();
    // `Stand.reserve` 已废止 - PR #本体两层改造。机位占用一律走 `StandOccupation`。

    stand_actions.insert(
        "check_availability".to_string(),
        read_action(
            "check_availability",
            "Check stand availability in a time window, with conflicts and alternative suggestions.",
            {
                let mut p = HashMap::new();
                p.insert(
                    "stand_id".to_string(),
                    string_param("stand_id", "Stand id or code", true),
                );
                p
            },
            json!({"type": "object", "required": ["stand_id", "time_window"]}),
            "flight:read",
        ),
    );

    objects.insert(
        "Stand".to_string(),
        OntologyObjectDef {
            name: "Stand".to_string(),
            description: "An airport parking stand".to_string(),
            object_id_strategy: "stand_id".to_string(),
            fields: stand_fields,
            relations: HashMap::new(),
            actions: stand_actions,
        },
    );

    // 3. DispatchOrder Object
    let mut dispatch_order_fields = HashMap::new();
    dispatch_order_fields.insert(
        "order_id".to_string(),
        OntologyFieldDef {
            name: "order_id".to_string(),
            field_type: "String".to_string(),
            description: "Unique identifier".to_string(),
            required: true,
        },
    );
    dispatch_order_fields.insert(
        "status".to_string(),
        OntologyFieldDef {
            name: "status".to_string(),
            field_type: "String".to_string(),
            description: "Order status".to_string(),
            required: true,
        },
    );

    let mut dispatch_order_actions = HashMap::new();
    dispatch_order_actions.insert(
        "get_status".to_string(),
        read_action(
            "get_status",
            "Read the full status of a dispatch order including team, equipment and conflicts.",
            {
                let mut p = HashMap::new();
                p.insert(
                    "dispatch_order_id".to_string(),
                    string_param("dispatch_order_id", "Dispatch order to inspect", true),
                );
                p
            },
            json!({"type": "object", "required": ["dispatch_order_id"]}),
            "dispatch:read",
        ),
    );
    dispatch_order_actions.insert(
        "recommend_replan".to_string(),
        OntologyActionDef {
            name: "recommend_replan".to_string(),
            description: "Recommend a replan for the dispatch order".to_string(),
            category: "advisory".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "reason".to_string(),
                    OntologyActionParameter {
                        name: "reason".to_string(),
                        param_type: "String".to_string(),
                        description: "Reason for replan".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["reason"]}),
            required_permissions: vec!["dispatch:write".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.DispatchOrder.recommend_replan".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
    // `DispatchOrder.update_status`：状态枚举迁移，受控写动作。
    dispatch_order_actions.insert(
        "update_status".to_string(),
        OntologyActionDef {
            name: "update_status".to_string(),
            description: "Transition the dispatch order status (pending|assigned|in_progress|completed|cancelled)."
                .to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "dispatch_order_id".to_string(),
                    string_param("dispatch_order_id", "Dispatch order to update", true),
                );
                p.insert(
                    "new_status".to_string(),
                    string_param(
                        "new_status",
                        "enum: pending|assigned|in_progress|completed|cancelled",
                        true,
                    ),
                );
                p.insert(
                    "notes".to_string(),
                    string_param("notes", "Optional status change notes", false),
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["dispatch_order_id", "new_status"]}),
            required_permissions: vec!["dispatch:write".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.DispatchOrder.update_status".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: restore_snapshot_compensation(true),
        },
    );
    dispatch_order_actions.insert(
        "suggest_replan".to_string(),
        advisory_action(
            "suggest_replan",
            "Generate a reassignment proposal for a dispatch order (resource/score changes, conflicts). No business write.",
            {
                let mut p = HashMap::new();
                p.insert("dispatch_order_id".to_string(), string_param("dispatch_order_id", "Dispatch order to replan", true));
                p.insert("reason".to_string(), string_param("reason", "Replan reason", true));
                p.insert("target_team_id".to_string(), string_param("target_team_id", "Optional target team; auto-pick when omitted", false));
                p
            },
            json!({"type": "object", "required": ["dispatch_order_id", "reason"]}),
            "dispatch:read",
            "high",
        ),
    );
    // 注：DispatchOrder 对象修改 - PR #本体两层改造
    // 废止 reassign 动作；改为槽位指派 (assign_slot/unassign_slot/add_slot/remove_slot)
    dispatch_order_actions.remove("reassign"); // 已废止

    dispatch_order_actions.insert(
        "assign_slot".to_string(),
        OntologyActionDef {
            name: "assign_slot".to_string(),
            description: "Assign personnel to a named slot (slot_code). Same department, on_duty, qualified.".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("dispatch_order_id".to_string(), string_param("dispatch_order_id", "Dispatch order ID", true));
                p.insert("slot_code".to_string(), string_param("slot_code", "Slot code to assign", true));
                p.insert("user_id".to_string(), string_param("user_id", "Personnel user_id", true));
                p
            },
            parameters_schema: json!({"type": "object", "required": ["dispatch_order_id", "slot_code", "user_id"]}),
            required_permissions: vec!["dispatch:manage".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.DispatchOrder.assign_slot".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
    dispatch_order_actions.insert(
        "unassign_slot".to_string(),
        OntologyActionDef {
            name: "unassign_slot".to_string(),
            description: "Unassign personnel from a slot".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("dispatch_order_id".to_string(), string_param("dispatch_order_id", "Dispatch order ID", true));
                p.insert("slot_code".to_string(), string_param("slot_code", "Slot code to unassign", true));
                p
            },
            parameters_schema: json!({"type": "object", "required": ["dispatch_order_id", "slot_code"]}),
            required_permissions: vec!["dispatch:manage".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.DispatchOrder.unassign_slot".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
    dispatch_order_actions.insert(
        "add_slot".to_string(),
        OntologyActionDef {
            name: "add_slot".to_string(),
            description: "Add a new slot to this dispatch order".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("dispatch_order_id".to_string(), string_param("dispatch_order_id", "Dispatch order ID", true));
                p.insert("slot_code".to_string(), string_param("slot_code", "New slot code", true));
                p.insert("slot_name".to_string(), string_param("slot_name", "Slot display name", false));
                p
            },
            parameters_schema: json!({"type": "object", "required": ["dispatch_order_id", "slot_code"]}),
            required_permissions: vec!["dispatch:manage".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.DispatchOrder.add_slot".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
    dispatch_order_actions.insert(
        "remove_slot".to_string(),
        OntologyActionDef {
            name: "remove_slot".to_string(),
            description: "Remove a slot from this dispatch order".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("dispatch_order_id".to_string(), string_param("dispatch_order_id", "Dispatch order ID", true));
                p.insert("slot_code".to_string(), string_param("slot_code", "Slot code to remove", true));
                p
            },
            parameters_schema: json!({"type": "object", "required": ["dispatch_order_id", "slot_code"]}),
            required_permissions: vec!["dispatch:manage".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.DispatchOrder.remove_slot".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
    dispatch_order_actions.insert(
        "publish".to_string(),
        OntologyActionDef {
            name: "publish".to_string(),
            description: "Publish the dispatch order".to_string(),
            category: "write".to_string(),
            parameters: HashMap::new(),
            parameters_schema: json!({"type": "object", "required": []}),
            required_permissions: vec!["dispatch:publish".to_string()],
            risk_level: "high".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.DispatchOrder.publish".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );

    objects.insert(
        "DispatchOrder".to_string(),
        OntologyObjectDef {
            name: "DispatchOrder".to_string(),
            description: "A dispatch order for ground handling".to_string(),
            object_id_strategy: "dispatch_order_id".to_string(),
            fields: dispatch_order_fields,
            relations: HashMap::new(),
            actions: dispatch_order_actions,
        },
    );

    // 4. Anomaly Object
    let mut anomaly_fields = HashMap::new();
    anomaly_fields.insert(
        "anomaly_id".to_string(),
        OntologyFieldDef {
            name: "anomaly_id".to_string(),
            field_type: "String".to_string(),
            description: "Unique identifier".to_string(),
            required: true,
        },
    );
    anomaly_fields.insert(
        "severity".to_string(),
        OntologyFieldDef {
            name: "severity".to_string(),
            field_type: "String".to_string(),
            description: "Severity of anomaly".to_string(),
            required: true,
        },
    );

    let mut anomaly_actions = HashMap::new();
    anomaly_actions.insert(
        "list_open".to_string(),
        read_action(
            "list_open",
            "List unresolved anomalies (open + acknowledged) with severity summary.",
            {
                let mut p = HashMap::new();
                p.insert(
                    "severity".to_string(),
                    string_param("severity", "Severity filter", false),
                );
                p.insert(
                    "flight_id".to_string(),
                    string_param("flight_id", "Flight filter", false),
                );
                p
            },
            json!({"type": "object", "required": []}),
            "anomaly:read",
        ),
    );
    anomaly_actions.insert(
        "acknowledge".to_string(),
        OntologyActionDef {
            name: "acknowledge".to_string(),
            description: "Acknowledge the anomaly".to_string(),
            category: "write".to_string(),
            parameters: HashMap::new(),
            parameters_schema: json!({"type": "object", "required": []}),
            required_permissions: vec!["anomaly:write".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Anomaly.acknowledge".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
    anomaly_actions.insert(
        "suggest_escalation".to_string(),
        advisory_action(
            "suggest_escalation",
            "Generate an escalation proposal for an anomaly (severity or handling path). No business write.",
            {
                let mut p = HashMap::new();
                p.insert(
                    "anomaly_id".to_string(),
                    string_param("anomaly_id", "Anomaly to escalate", true),
                );
                p
            },
            json!({"type": "object", "required": ["anomaly_id"]}),
            "anomaly:read",
            "medium",
        ),
    );
    anomaly_actions.insert(
        "escalate".to_string(),
        OntologyActionDef {
            name: "escalate".to_string(),
            description: "Escalate the anomaly severity or handling path".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "reason".to_string(),
                    OntologyActionParameter {
                        name: "reason".to_string(),
                        param_type: "String".to_string(),
                        description: "Escalation reason".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["reason"]}),
            required_permissions: vec!["anomaly:write".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Anomaly.escalate".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );

    // `Anomaly.resolve`：低风险受控写动作。
    anomaly_actions.insert(
        "resolve".to_string(),
        OntologyActionDef {
            name: "resolve".to_string(),
            description: "Resolve the anomaly with an optional resolution note".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "anomaly_id".to_string(),
                    string_param("anomaly_id", "Anomaly to resolve", true),
                );
                p.insert(
                    "resolution_note".to_string(),
                    string_param("resolution_note", "Optional resolution note", false),
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["anomaly_id"]}),
            required_permissions: vec!["anomaly:write".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Anomaly.resolve".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );

    objects.insert(
        "Anomaly".to_string(),
        OntologyObjectDef {
            name: "Anomaly".to_string(),
            description: "An operational anomaly".to_string(),
            object_id_strategy: "anomaly_id".to_string(),
            fields: anomaly_fields,
            relations: HashMap::new(),
            actions: anomaly_actions,
        },
    );

    // 注：FlightLeg Object 已删除 - PR #本体两层改造
    // Team Object (班组 - 名册而非工单指派对象)
    let mut team_fields = HashMap::new();
    team_fields.insert("team_id".to_string(), OntologyFieldDef { name: "team_id".to_string(), field_type: "String".to_string(), description: "Team ID".to_string(), required: true });
    team_fields.insert("name".to_string(), OntologyFieldDef { name: "name".to_string(), field_type: "String".to_string(), description: "Team name".to_string(), required: true });
    team_fields.insert("code".to_string(), OntologyFieldDef { name: "code".to_string(), field_type: "String".to_string(), description: "Team code".to_string(), required: true });
    team_fields.insert("department_id".to_string(), OntologyFieldDef { name: "department_id".to_string(), field_type: "String".to_string(), description: "所属科室 (REQUIRED)".to_string(), required: true });
    team_fields.insert("leader_id".to_string(), OntologyFieldDef { name: "leader_id".to_string(), field_type: "String".to_string(), description: "Team leader user_id".to_string(), required: false });
    team_fields.insert("current_status".to_string(), OntologyFieldDef { name: "current_status".to_string(), field_type: "String".to_string(), description: "on_duty|off_duty|break".to_string(), required: true });
    team_fields.insert("is_active".to_string(), OntologyFieldDef { name: "is_active".to_string(), field_type: "Boolean".to_string(), description: "Active status".to_string(), required: true });

    let mut team_actions = HashMap::new();
    team_actions.insert("get_context".to_string(), read_action("get_context", "Read team details including members and runtime status.", { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team to inspect", true)); p }, json!({"type": "object", "required": ["team_id"]}), "ontology:read"));
    team_actions.insert("update_status".to_string(), OntologyActionDef { name: "update_status".to_string(), description: "Update on_duty/off_duty/break status for multiple members (代签支持)".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team ID", true)); p.insert("current_status".to_string(), string_param("current_status", "on_duty|off_duty|break", true)); p.insert("member_user_ids".to_string(), string_param("member_user_ids", "List of member user IDs", false)); p }, parameters_schema: json!({"type": "object", "required": ["team_id", "current_status"]}), required_permissions: vec!["dispatch:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Team.update_status".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    team_actions.insert("change_location".to_string(), OntologyActionDef { name: "change_location".to_string(), description: "Change team location".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team ID", true)); p.insert("location".to_string(), string_param("location", "New location", false)); p }, parameters_schema: json!({"type": "object", "required": ["team_id"]}), required_permissions: vec!["dispatch:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Team.change_location".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    team_actions.insert("add_member".to_string(), OntologyActionDef { name: "add_member".to_string(), description: "Add a personnel to team (must be same department). Returns 409 if cross-department or not personal account".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team ID", true)); p.insert("user_id".to_string(), string_param("user_id", "Personnel user_id", true)); p }, parameters_schema: json!({"type": "object", "required": ["team_id", "user_id"]}), required_permissions: vec!["dispatch:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Team.add_member".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    team_actions.insert("remove_member".to_string(), OntologyActionDef { name: "remove_member".to_string(), description: "Remove a personnel from team".to_string(), category: "write".to_string(), parameters: { let mut p = HashMap::new(); p.insert("team_id".to_string(), string_param("team_id", "Team ID", true)); p.insert("user_id".to_string(), string_param("user_id", "Personnel user_id", true)); p }, parameters_schema: json!({"type": "object", "required": ["team_id", "user_id"]}), required_permissions: vec!["dispatch:manage".to_string()], risk_level: "low".to_string(), approval_strategy: "auto_approve".to_string(), approval_policy: "auto_execute".to_string(), constraints: vec![], execution_mapping: Some("DomainActionExecutor.Team.remove_member".to_string()), idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()), compensation: None });
    // 注：废止 assign_task 动作 - PR #本体两层改造

    objects.insert(
        "Team".to_string(),
        OntologyObjectDef {
            name: "Team".to_string(),
            description: "Ground handling team".to_string(),
            object_id_strategy: "team_id".to_string(),
            fields: team_fields,
            relations: HashMap::new(),
            actions: team_actions,
        },
    );

    // 7. Equipment Object
    let mut equipment_fields = HashMap::new();
    equipment_fields.insert(
        "equipment_id".to_string(),
        OntologyFieldDef {
            name: "equipment_id".to_string(),
            field_type: "String".to_string(),
            description: "Equipment ID".to_string(),
            required: true,
        },
    );

    objects.insert(
        "Equipment".to_string(),
        OntologyObjectDef {
            name: "Equipment".to_string(),
            description: "Ground handling equipment".to_string(),
            object_id_strategy: "equipment_id".to_string(),
            fields: equipment_fields,
            relations: HashMap::new(),
            actions: HashMap::new(),
        },
    );

    // 8. BusinessCase Object
    let mut business_case_fields = HashMap::new();
    business_case_fields.insert(
        "case_id".to_string(),
        OntologyFieldDef {
            name: "case_id".to_string(),
            field_type: "String".to_string(),
            description: "Case ID".to_string(),
            required: true,
        },
    );
    business_case_fields.insert(
        "status".to_string(),
        OntologyFieldDef {
            name: "status".to_string(),
            field_type: "String".to_string(),
            description: "Case status".to_string(),
            required: true,
        },
    );

    let mut business_case_actions = HashMap::new();
    business_case_actions.insert(
        "create".to_string(),
        OntologyActionDef {
            name: "create".to_string(),
            description: "Create a business case for a flight".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                for name in ["flight_id", "case_type", "description"] {
                    p.insert(
                        name.to_string(),
                        OntologyActionParameter {
                            name: name.to_string(),
                            param_type: "String".to_string(),
                            description: format!("Business case {name}"),
                            required: true,
                        },
                    );
                }
                p
            },
            parameters_schema: json!({"type": "object", "required": ["flight_id", "case_type", "description"]}),
            required_permissions: vec!["business_case:create".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.BusinessCase.create".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
    business_case_actions.insert(
        "close_case".to_string(),
        OntologyActionDef {
            name: "close_case".to_string(),
            description: "Close the business case".to_string(),
            category: "write".to_string(),
            parameters: HashMap::new(),
            parameters_schema: json!({"type": "object", "required": []}),
            required_permissions: vec!["business_case:update".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.BusinessCase.close_case".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );

    objects.insert(
        "BusinessCase".to_string(),
        OntologyObjectDef {
            name: "BusinessCase".to_string(),
            description: "Flight business case".to_string(),
            object_id_strategy: "business_case_id".to_string(),
            fields: business_case_fields,
            relations: HashMap::new(),
            actions: business_case_actions,
        },
    );

    // 注：Notification Object 已删除 - PR #本体两层改造
    // Notification 退出，改为用 `Flight.add_label` 替代标签功能

    // 注：Todo Object 已删除 - PR #本体两层改造
    // Todo 退出，不是机场资源

    // 11. Report Object (只读汇报动作的宿主对象)
    let mut report_fields = HashMap::new();
    report_fields.insert(
        "report_id".to_string(),
        OntologyFieldDef {
            name: "report_id".to_string(),
            field_type: "String".to_string(),
            description: "Report identifier".to_string(),
            required: true,
        },
    );

    let mut report_actions = HashMap::new();
    report_actions.insert(
        "generate_briefing".to_string(),
        read_action(
            "generate_briefing",
            "Generate an operations briefing for a shift window with explicit limitations and confidence.",
            {
                let mut p = HashMap::new();
                p.insert(
                    "shift_start".to_string(),
                    string_param("shift_start", "RFC3339 shift start", false),
                );
                p.insert(
                    "shift_end".to_string(),
                    string_param("shift_end", "RFC3339 shift end", false),
                );
                p.insert(
                    "scope".to_string(),
                    string_param("scope", "all|inbound|outbound", false),
                );
                p.insert(
                    "department_id".to_string(),
                    string_param("department_id", "Department filter", false),
                );
                p
            },
            json!({"type": "object", "required": []}),
            "flight:read",
        ),
    );

    objects.insert(
        "Report".to_string(),
        OntologyObjectDef {
            name: "Report".to_string(),
            description: "Operational report / briefing".to_string(),
            object_id_strategy: "report_id".to_string(),
            fields: report_fields,
            relations: HashMap::new(),
            actions: report_actions,
        },
    );

    // 注：Label Object 已删除 - PR #本体两层改造
    // Label 退出，标签改为 `Flight.add_label`

    // 注：Workflow Object 已删除 - PR #本体两层改造
    // Workflow 不是本体一等对象；Flowable 实例是 BusinessCase 的属性

    OntologySchema {
        version: FLIGHT_OPS_ONTOLOGY_VERSION.to_string(),
        description: "Flight Operations Ontology Schema V1".to_string(),
        objects,
    }
}

#[cfg(test)]
mod tests {
    use super::build_flight_ops_v1_schema;

    #[test]
    fn flight_ops_v1_actions_expose_aip_governance_contract() {
        let schema = build_flight_ops_v1_schema();

        assert_eq!(schema.version, "flight-ops.v1");

        let flight = schema.objects.get("Flight").expect("Flight object exists");
        assert_eq!(flight.object_id_strategy, "flight_id");

        // Flight.change_stand 已废止（PR #本体两层改造）——合同不得再含该动作
        assert!(
            !flight.actions.contains_key("change_stand"),
            "Flight.change_stand 已废止，不得出现在合同里（改用 StandOccupation 占用回写）"
        );

        for (object_type, action_name) in [
            ("DispatchOrder", "recommend_replan"),
            ("DispatchOrder", "publish"),
            ("DispatchOrder", "update_status"),
            ("Anomaly", "acknowledge"),
            ("Anomaly", "escalate"),
            ("Anomaly", "resolve"),
            ("BusinessCase", "create"),
            ("BusinessCase", "close_case"),
            // Note: change_stand, reserve, reassign removed in PR #本体两层改造
            ("Flight", "update_delay"),
            ("Flight", "add_label"),
        ] {
            assert!(
                schema
                    .objects
                    .get(object_type)
                    .and_then(|object| object.actions.get(action_name))
                    .is_some(),
                "{object_type}.{action_name} must be present in static fallback ontology"
            );
        }
    }

    #[test]
    fn flight_ops_v1_advisory_actions_follow_contract_rules() {
        let schema = build_flight_ops_v1_schema();

        let advisory_actions = [
            ("Flight", "suggest_stand_adjustment", "flight:read", "medium"),
            ("Flight", "suggest_delay_action", "flight:read", "medium"),
            ("DispatchOrder", "suggest_replan", "dispatch:read", "high"),
            ("Anomaly", "suggest_escalation", "anomaly:read", "medium"),
            // Notification.suggest_broadcast removed in PR #本体两层改造
        ];
        for (object_type, action_name, permission, risk) in advisory_actions {
            let action = schema
                .objects
                .get(object_type)
                .and_then(|object| object.actions.get(action_name))
                .unwrap_or_else(|| panic!("{object_type}.{action_name} must exist"));
            assert_eq!(action.category, "advisory", "{object_type}.{action_name}");
            assert_eq!(action.risk_level, risk, "{object_type}.{action_name}");
            assert_eq!(
                action.approval_policy, "require_approval",
                "{object_type}.{action_name}"
            );
            assert_eq!(
                action.required_permissions,
                vec![permission],
                "{object_type}.{action_name}"
            );
            assert!(
                action.execution_mapping.is_none(),
                "{object_type}.{action_name} 建议动作不得映射到 DomainActionExecutor（见 ONTOLOGY_V1.md）"
            );
        }
    }

    #[test]
    fn flight_ops_v1_controlled_write_actions_follow_contract_rules() {
        let schema = build_flight_ops_v1_schema();

        // (object, action, permission, risk, approval_policy, execution_mapping)
        let write_actions = [
            (
                "Flight",
                "update_delay",
                "flight:write",
                "medium",
                "require_approval",
                "DomainActionExecutor.Flight.update_delay",
            ),
            (
                "DispatchOrder",
                "update_status",
                "dispatch:write",
                "medium",
                "require_approval",
                "DomainActionExecutor.DispatchOrder.update_status",
            ),
            (
                "Anomaly",
                "resolve",
                "anomaly:write",
                "low",
                "auto_execute",
                "DomainActionExecutor.Anomaly.resolve",
            ),
            // change_stand, reserve, Label.add, Workflow.start removed in PR #本体两层改造
        ];
        for (object_type, action_name, permission, risk, approval, mapping) in write_actions {
            let action = schema
                .objects
                .get(object_type)
                .and_then(|object| object.actions.get(action_name))
                .unwrap_or_else(|| panic!("{object_type}.{action_name} must exist"));
            assert_eq!(action.category, "write", "{object_type}.{action_name}");
            assert_eq!(action.risk_level, risk, "{object_type}.{action_name}");
            assert_eq!(action.approval_policy, approval, "{object_type}.{action_name}");
            assert_eq!(
                action.required_permissions,
                vec![permission],
                "{object_type}.{action_name}"
            );
            assert_eq!(
                action.execution_mapping.as_deref(),
                Some(mapping),
                "{object_type}.{action_name} 必须映射到 DomainActionExecutor（见 ONTOLOGY_V1.md）"
            );
            assert!(
                action.idempotency_key_strategy.is_some(),
                "{object_type}.{action_name} 写动作必须定义幂等键策略"
            );
        }

        assert!(
            !schema.objects["Flight"].actions.contains_key("change_stand"),
            "Flight.change_stand 已废止，不得出现在合同里（改用 StandOccupation 占用回写）"
        );
        assert!(
            !schema.objects["Flight"].actions.contains_key("update_stand"),
            "Flight.update_stand must not be exported as an alias"
        );
        assert!(
            !schema.objects["Stand"].actions.contains_key("reserve"),
            "Stand.reserve 已废止，不得出现在合同里（改用 StandOccupation.allocate）"
        );
    }

    #[test]
    fn flight_ops_v1_read_actions_follow_contract_rules() {
        let schema = build_flight_ops_v1_schema();

        let read_actions = [
            ("Flight", "get_context", "flight:read"),
            ("Flight", "search", "flight:read"),
            ("DispatchOrder", "get_status", "dispatch:read"),
            ("Anomaly", "list_open", "anomaly:read"),
            ("Stand", "check_availability", "flight:read"),
            ("Report", "generate_briefing", "flight:read"),
        ];
        for (object_type, action_name, permission) in read_actions {
            let action = schema
                .objects
                .get(object_type)
                .and_then(|object| object.actions.get(action_name))
                .unwrap_or_else(|| panic!("{object_type}.{action_name} must exist"));
            assert_eq!(action.category, "read", "{object_type}.{action_name}");
            assert_eq!(action.risk_level, "low", "{object_type}.{action_name}");
            assert_eq!(action.approval_policy, "auto_execute", "{object_type}.{action_name}");
            assert_eq!(
                action.required_permissions,
                vec![permission],
                "{object_type}.{action_name}"
            );
            assert!(
                action.execution_mapping.is_none(),
                "{object_type}.{action_name} 只读动作不得映射到 DomainActionExecutor"
            );
        }
    }
}
