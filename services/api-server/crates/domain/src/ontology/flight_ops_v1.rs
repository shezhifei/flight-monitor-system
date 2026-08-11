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

fn irreversible_compensation() -> Option<CompensationMetadata> {
    Some(CompensationMetadata {
        mode: "irreversible".to_string(),
        requires_approval: true,
        irreversible_fields: vec!["body".to_string()],
        inverse_action_name: None,
        before_snapshot_required: false,
        followup_action_name: None,
        followup_args: None,
    })
}

/// 只读动作统一定义（契约 §4.2：不创建 pending action，直接执行，响应带 evidence）。
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

pub fn build_flight_ops_v1_schema() -> OntologySchema {
    let mut objects = HashMap::new();

    // 1. Flight Object
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
    flight_actions.insert(
        "change_stand".to_string(),
        OntologyActionDef {
            name: "change_stand".to_string(),
            description: "Change the assigned stand for the flight".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "new_stand_id".to_string(),
                    OntologyActionParameter {
                        name: "new_stand_id".to_string(),
                        param_type: "String".to_string(),
                        description: "The ID of the new stand".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["new_stand_id"]}),
            required_permissions: vec!["flight:write".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![OntologyConstraint {
                constraint_type: "Precondition".to_string(),
                expression: "stand.is_available()".to_string(),
                description: "Target stand must be available".to_string(),
            }],
            execution_mapping: Some("DomainActionExecutor.Flight.change_stand".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
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

    flight_actions.insert(
        "search".to_string(),
        read_action(
            "search",
            "Search flights by flight number, status, origin, destination, date or open-anomaly flag (limit <= 200).",
            {
                let mut p = HashMap::new();
                p.insert("flight_no".to_string(), string_param("flight_no", "Flight number filter", false));
                p.insert("status".to_string(), string_param("status", "Status filter", false));
                p.insert("origin".to_string(), string_param("origin", "Origin airport code", false));
                p.insert("destination".to_string(), string_param("destination", "Destination airport code", false));
                p.insert("date".to_string(), string_param("date", "Operating date YYYY-MM-DD", false));
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
    stand_actions.insert(
        "reserve".to_string(),
        OntologyActionDef {
            name: "reserve".to_string(),
            description: "Reserve the stand for a flight".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "flight_id".to_string(),
                    OntologyActionParameter {
                        name: "flight_id".to_string(),
                        param_type: "String".to_string(),
                        description: "The flight to reserve for".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["flight_id"]}),
            required_permissions: vec!["dispatch:write".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![OntologyConstraint {
                constraint_type: "Precondition".to_string(),
                expression: "is_available == true".to_string(),
                description: "Stand must be available to reserve".to_string(),
            }],
            execution_mapping: Some("DomainActionExecutor.Stand.reserve".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );

    stand_actions.insert(
        "check_availability".to_string(),
        read_action(
            "check_availability",
            "Check stand availability in a time window, with conflicts and alternative suggestions.",
            {
                let mut p = HashMap::new();
                p.insert("stand_id".to_string(), string_param("stand_id", "Stand id or code", true));
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
    dispatch_order_actions.insert(
        "reassign".to_string(),
        OntologyActionDef {
            name: "reassign".to_string(),
            description: "Reassign the dispatch order to another team or user".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "assignee_id".to_string(),
                    OntologyActionParameter {
                        name: "assignee_id".to_string(),
                        param_type: "String".to_string(),
                        description: "Target team or user ID".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["assignee_id"]}),
            required_permissions: vec!["dispatch:admin".to_string()],
            risk_level: "high".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.DispatchOrder.reassign".to_string()),
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
                p.insert("severity".to_string(), string_param("severity", "Severity filter", false));
                p.insert("flight_id".to_string(), string_param("flight_id", "Flight filter", false));
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

    // 5. FlightLeg Object
    let mut flight_leg_fields = HashMap::new();
    flight_leg_fields.insert(
        "leg_id".to_string(),
        OntologyFieldDef {
            name: "leg_id".to_string(),
            field_type: "String".to_string(),
            description: "Unique identifier".to_string(),
            required: true,
        },
    );
    flight_leg_fields.insert(
        "direction".to_string(),
        OntologyFieldDef {
            name: "direction".to_string(),
            field_type: "String".to_string(),
            description: "Arrival or Departure".to_string(),
            required: true,
        },
    );

    objects.insert(
        "FlightLeg".to_string(),
        OntologyObjectDef {
            name: "FlightLeg".to_string(),
            description: "A single leg of a flight".to_string(),
            object_id_strategy: "leg_id".to_string(),
            fields: flight_leg_fields,
            relations: HashMap::new(),
            actions: HashMap::new(),
        },
    );

    // 6. Team Object
    let mut team_fields = HashMap::new();
    team_fields.insert(
        "team_id".to_string(),
        OntologyFieldDef {
            name: "team_id".to_string(),
            field_type: "String".to_string(),
            description: "Team ID".to_string(),
            required: true,
        },
    );
    team_fields.insert(
        "status".to_string(),
        OntologyFieldDef {
            name: "status".to_string(),
            field_type: "String".to_string(),
            description: "Team status".to_string(),
            required: true,
        },
    );

    let mut team_actions = HashMap::new();
    team_actions.insert(
        "assign_task".to_string(),
        OntologyActionDef {
            name: "assign_task".to_string(),
            description: "Assign a task to the team".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "task_id".to_string(),
                    OntologyActionParameter {
                        name: "task_id".to_string(),
                        param_type: "String".to_string(),
                        description: "Task ID".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["task_id"]}),
            required_permissions: vec!["dispatch:write".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Team.assign_task".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );

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

    // 9. Notification Object
    let mut notification_fields = HashMap::new();
    notification_fields.insert(
        "notification_id".to_string(),
        OntologyFieldDef {
            name: "notification_id".to_string(),
            field_type: "String".to_string(),
            description: "Notification ID".to_string(),
            required: true,
        },
    );

    let mut notification_actions = HashMap::new();
    notification_actions.insert(
        "send".to_string(),
        OntologyActionDef {
            name: "send".to_string(),
            description: "Send a governed notification".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                for name in ["user_id", "title", "body"] {
                    p.insert(
                        name.to_string(),
                        OntologyActionParameter {
                            name: name.to_string(),
                            param_type: "String".to_string(),
                            description: format!("Notification {name}"),
                            required: true,
                        },
                    );
                }
                p
            },
            parameters_schema: json!({"type": "object", "required": ["user_id", "title", "body"]}),
            required_permissions: vec!["notification:send".to_string()],
            risk_level: "medium".to_string(),
            approval_strategy: "require_approval".to_string(),
            approval_policy: "require_approval".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Notification.send".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: irreversible_compensation(),
        },
    );

    objects.insert(
        "Notification".to_string(),
        OntologyObjectDef {
            name: "Notification".to_string(),
            description: "System notification".to_string(),
            object_id_strategy: "notification_id".to_string(),
            fields: notification_fields,
            relations: HashMap::new(),
            actions: notification_actions,
        },
    );

    // 10. Todo Object
    let mut todo_fields = HashMap::new();
    todo_fields.insert(
        "todo_id".to_string(),
        OntologyFieldDef {
            name: "todo_id".to_string(),
            field_type: "String".to_string(),
            description: "Todo ID".to_string(),
            required: true,
        },
    );

    let mut todo_actions = HashMap::new();
    todo_actions.insert(
        "create".to_string(),
        OntologyActionDef {
            name: "create".to_string(),
            description: "Create a todo item".to_string(),
            category: "write".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "title".to_string(),
                    OntologyActionParameter {
                        name: "title".to_string(),
                        param_type: "String".to_string(),
                        description: "Todo title".to_string(),
                        required: true,
                    },
                );
                p
            },
            parameters_schema: json!({"type": "object", "required": ["title"]}),
            required_permissions: vec!["todo:write".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Todo.create".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: None,
        },
    );
    todo_actions.insert(
        "complete".to_string(),
        OntologyActionDef {
            name: "complete".to_string(),
            description: "Complete a todo item".to_string(),
            category: "write".to_string(),
            parameters: HashMap::new(),
            parameters_schema: json!({"type": "object", "required": []}),
            required_permissions: vec!["todo:write".to_string()],
            risk_level: "low".to_string(),
            approval_strategy: "auto_approve".to_string(),
            approval_policy: "auto_execute".to_string(),
            constraints: vec![],
            execution_mapping: Some("DomainActionExecutor.Todo.complete".to_string()),
            idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
            compensation: inverse_action_compensation("Todo.reopen", false),
        },
    );

    objects.insert(
        "Todo".to_string(),
        OntologyObjectDef {
            name: "Todo".to_string(),
            description: "User to-do item".to_string(),
            object_id_strategy: "todo_id".to_string(),
            fields: todo_fields,
            relations: HashMap::new(),
            actions: todo_actions,
        },
    );

    // 10.5 Report Object (只读汇报动作的宿主对象)
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
                p.insert("shift_start".to_string(), string_param("shift_start", "RFC3339 shift start", false));
                p.insert("shift_end".to_string(), string_param("shift_end", "RFC3339 shift end", false));
                p.insert("scope".to_string(), string_param("scope", "all|inbound|outbound", false));
                p.insert("department_id".to_string(), string_param("department_id", "Department filter", false));
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

        let change_stand = flight.actions.get("change_stand").expect("Flight.change_stand exists");
        assert_eq!(change_stand.category, "write");
        assert_eq!(change_stand.required_permissions, vec!["flight:write"]);
        assert_eq!(change_stand.risk_level, "medium");
        assert_eq!(change_stand.approval_policy, "require_approval");
        assert!(change_stand.parameters_schema["required"]
            .as_array()
            .expect("required array")
            .iter()
            .any(|item| item == "new_stand_id"));
        assert_eq!(
            change_stand.execution_mapping.as_deref(),
            Some("DomainActionExecutor.Flight.change_stand")
        );

        for (object_type, action_name) in [
            ("DispatchOrder", "recommend_replan"),
            ("DispatchOrder", "reassign"),
            ("DispatchOrder", "publish"),
            ("Anomaly", "acknowledge"),
            ("Anomaly", "escalate"),
            ("Notification", "send"),
            ("Todo", "create"),
            ("Todo", "complete"),
            ("BusinessCase", "create"),
            ("BusinessCase", "close_case"),
            ("Stand", "reserve"),
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
            assert_eq!(action.required_permissions, vec![permission], "{object_type}.{action_name}");
            assert!(
                action.execution_mapping.is_none(),
                "{object_type}.{action_name} 只读动作不得映射到 DomainActionExecutor"
            );
        }
    }
}
