use fms_domain::models::dispatch::{
    Department, DepartmentQualificationCatalog, DepartmentQualificationLevel, DepartmentRuleStatus,
    DepartmentTaskTypeRequirementVersion, DispatchPublicationState, Equipment, EquipmentStatus, EquipmentType,
    FlightGenerationRule, GenerationAdjustmentRule, LegScope, MemberRole, PublishTriggerMode, QualificationGrant,
    QualificationGrantStatus, ShiftInstance, ShiftTemplate, Stand, TaskType, TaskTypeCrewSlotRequirement,
    TaskTypeEquipmentRequirement, Team, TeamMember, TeamStatus, TeamType, TemporaryTaskTemplate,
    PersonnelStatus, TurnaroundConstraintMode, TurnaroundContinuityRule,
};
use serde_json::Value;

use crate::schemas::dispatch_schemas::{
    DepartmentQualificationCatalogResponse, DepartmentQualificationLevelResponse, DepartmentResponse,
    DepartmentTaskTypeRequirementVersionResponse, EquipmentResponse, EquipmentTypeResponse,
    FlightGenerationRuleResponse, GenerationAdjustmentRuleResponse, PositionSchema, QualificationGrantResponse,
    ShiftInstanceResponse, ShiftTemplateResponse, StandResponse, TaskTypeCrewSlotRequirementSchema,
    TaskTypeEquipmentRequirementSchema, TaskTypeResponse, TeamMemberResponse, TeamResponse, TeamTypeResponse,
    TemporaryTaskTemplateResponse, TurnaroundContinuityRuleSchema, TurnaroundSlotPairSchema,
};

pub fn to_department_response(department: Department) -> DepartmentResponse {
    DepartmentResponse {
        id: department.id,
        name: department.name,
        code: department.code,
        description: department.description,
        manager_id: department.manager_id,
        terminal: department.terminal,
        created_at: department.created_at,
        updated_at: department.updated_at,
        is_active: department.is_active,
    }
}

pub fn to_team_type_response(team_type: TeamType) -> TeamTypeResponse {
    TeamTypeResponse {
        id: team_type.id,
        name: team_type.name,
        department_id: team_type.department_id,
        code: team_type.code,
        description: team_type.description,
        color: team_type.color,
        is_driver_type: team_type.is_driver_type,
        task_types: team_type.task_types,
        created_at: team_type.created_at,
        is_active: team_type.is_active,
    }
}

pub fn to_member_response(member: TeamMember) -> TeamMemberResponse {
    TeamMemberResponse {
        id: member.id,
        team_id: member.team_id,
        user_id: member.user_id,
        role: member_role_value(member.role).to_string(),
        can_drive: member.can_drive,
        joined_at: member.joined_at,
        is_active: member.is_active,
        username: member.username,
        user_display_name: member.user_display_name,
    }
}

pub fn to_team_response(team: Team) -> TeamResponse {
    let current_position = match (team.current_position_lat, team.current_position_lng) {
        (Some(lat), Some(lng)) => Some(PositionSchema { lat, lng }),
        _ => None,
    };
    let members = team.members;
    TeamResponse {
        id: team.id,
        name: team.name,
        department_id: team.department_id,
        team_type_id: team.team_type_id,
        code: team.code,
        leader_id: team.leader_id,
        current_status: team_status_value(team.current_status).to_string(),
        current_position,
        current_stand_id: team.current_stand_id,
        last_position_update: team.last_position_update,
        created_at: team.created_at,
        is_active: team.is_active,
        member_count: members.len() as i32,
        members: members.into_iter().map(to_member_response).collect(),
    }
}

pub fn to_equipment_type_response(item: EquipmentType) -> EquipmentTypeResponse {
    EquipmentTypeResponse {
        id: item.id,
        name: item.name,
        code: item.code,
        category: item.category,
        requires_driver: item.requires_driver,
        icon: item.icon,
        description: item.description,
        created_at: item.created_at,
        is_active: item.is_active,
    }
}

pub fn to_equipment_response(item: Equipment) -> EquipmentResponse {
    let current_position = match (item.current_position_lat, item.current_position_lng) {
        (Some(lat), Some(lng)) => Some(PositionSchema { lat, lng }),
        _ => None,
    };
    let equipment_type_name = item.equipment_type.as_ref().map(|value| value.name.clone());

    EquipmentResponse {
        id: item.id,
        code: item.code,
        equipment_type_id: item.equipment_type_id,
        department_id: item.department_id,
        name: item.name,
        license_plate: item.license_plate,
        status: equipment_status_value(item.status).to_string(),
        current_position,
        current_stand_id: item.current_stand_id,
        last_position_update: item.last_position_update,
        next_maintenance_date: item.next_maintenance_date,
        current_dispatch_id: item.current_dispatch_id,
        created_at: item.created_at,
        is_active: item.is_active,
        equipment_type_name,
    }
}

pub fn to_stand_response(item: Stand) -> StandResponse {
    StandResponse {
        id: item.id,
        code: item.code,
        name: item.name,
        terminal: item.terminal,
        area: item.area,
        position: PositionSchema {
            lat: item.position_lat,
            lng: item.position_lng,
        },
        stand_type: item.stand_type,
        size_category: item.size_category,
        is_active: item.is_active,
    }
}

pub fn to_task_type_response(item: TaskType) -> TaskTypeResponse {
    TaskTypeResponse {
        id: item.id,
        code: item.code,
        name: item.name,
        default_department_id: item.default_department_id,
        category: item.category,
        sequence_order: item.sequence_order,
        default_duration_minutes: item.default_duration_minutes,
        trigger_offset_minutes: item.trigger_offset_minutes,
        trigger_type: item.trigger_type,
        description: item.description,
        is_active: item.is_active,
    }
}

pub fn to_department_qualification_response(
    item: DepartmentQualificationCatalog,
) -> DepartmentQualificationCatalogResponse {
    DepartmentQualificationCatalogResponse {
        id: item.id,
        department_id: item.department_id,
        qualification_code: item.qualification_code,
        qualification_name: item.qualification_name,
        description: item.description,
        is_active: item.is_active,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub fn to_department_qualification_level_response(
    item: DepartmentQualificationLevel,
) -> DepartmentQualificationLevelResponse {
    DepartmentQualificationLevelResponse {
        id: item.id,
        department_id: item.department_id,
        qualification_code: item.qualification_code,
        level_code: item.level_code,
        level_name: item.level_name,
        level_rank: item.level_rank,
        covered_level_codes: item.covered_level_codes,
        is_active: item.is_active,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub fn to_qualification_grant_response(item: QualificationGrant) -> QualificationGrantResponse {
    QualificationGrantResponse {
        id: item.id,
        user_id: item.user_id,
        department_id: item.department_id,
        qualification_code: item.qualification_code,
        level_code: item.level_code,
        valid_from: item.valid_from,
        valid_to: item.valid_to,
        status: qualification_grant_status_value(item.status).to_string(),
        source_team_id: item.source_team_id,
        metadata: item.metadata,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub fn to_task_type_requirement_version_response(
    item: DepartmentTaskTypeRequirementVersion,
) -> DepartmentTaskTypeRequirementVersionResponse {
    DepartmentTaskTypeRequirementVersionResponse {
        id: item.id,
        department_id: item.department_id,
        task_type: item.task_type,
        version_no: item.version_no,
        status: department_rule_status_value(item.status).to_string(),
        requirements: item
            .requirements
            .clone()
            .into_iter()
            .map(to_task_type_crew_slot_requirement_schema)
            .collect(),
        crew_requirements: item
            .crew_requirements
            .into_iter()
            .map(to_task_type_crew_slot_requirement_schema)
            .collect(),
        equipment_requirements: item
            .equipment_requirements
            .into_iter()
            .map(to_task_type_equipment_requirement_schema)
            .collect(),
        turnaround_continuity_rules: item
            .turnaround_continuity_rules
            .into_iter()
            .map(to_turnaround_continuity_rule_schema)
            .collect(),
        notes: item.notes,
        published_at: item.published_at,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub fn to_task_type_crew_slot_requirement_schema(
    item: TaskTypeCrewSlotRequirement,
) -> TaskTypeCrewSlotRequirementSchema {
    TaskTypeCrewSlotRequirementSchema {
        slot_code: item.slot_code,
        qualification_code: item.qualification_code,
        min_level_code: item.min_level_code,
        required_count: item.required_count,
        must_be_distinct: item.must_be_distinct,
        exclusive_group: item.exclusive_group,
        remarks: item.remarks,
    }
}

pub fn to_task_type_equipment_requirement_schema(
    item: TaskTypeEquipmentRequirement,
) -> TaskTypeEquipmentRequirementSchema {
    TaskTypeEquipmentRequirementSchema {
        slot_code: item.slot_code,
        equipment_type_id: item.equipment_type_id,
        equipment_type_code: item.equipment_type_code,
        required_count: item.required_count,
        must_be_distinct: item.must_be_distinct,
        requires_driver: item.requires_driver,
        driver_qualification_code: item.driver_qualification_code,
        driver_min_level_code: item.driver_min_level_code,
        remarks: item.remarks,
    }
}

pub fn to_turnaround_continuity_rule_schema(item: TurnaroundContinuityRule) -> TurnaroundContinuityRuleSchema {
    TurnaroundContinuityRuleSchema {
        enabled: item.enabled,
        counterpart_leg_scope: leg_scope_value(item.counterpart_leg_scope).to_string(),
        counterpart_task_type: item.counterpart_task_type,
        slot_pairs: item
            .slot_pairs
            .into_iter()
            .map(|pair| TurnaroundSlotPairSchema {
                inbound_slot_code: pair.inbound_slot_code,
                outbound_slot_code: pair.outbound_slot_code,
            })
            .collect(),
        constraint_mode: turnaround_constraint_mode_value(item.constraint_mode).to_string(),
        tight_threshold_minutes: item.tight_threshold_minutes,
        relax_threshold_minutes: item.relax_threshold_minutes,
        flight_filters: item.flight_filters,
        aircraft_type_filters: item.aircraft_type_filters,
        notes: item.notes,
    }
}

pub fn to_generation_rule_response(item: FlightGenerationRule) -> FlightGenerationRuleResponse {
    FlightGenerationRuleResponse {
        id: item.id,
        department_id: item.department_id,
        task_type: item.task_type,
        leg_scope: leg_scope_value(item.leg_scope).to_string(),
        version_no: item.version_no,
        status: department_rule_status_value(item.status).to_string(),
        rule_name: item.rule_name,
        conditions: item.conditions,
        generation_anchor_type: item.generation_anchor_type,
        start_offset_minutes: item.start_offset_minutes,
        completion_time_mode: item.completion_time_mode,
        completion_anchor_type: item.completion_anchor_type,
        completion_offset_minutes: item.completion_offset_minutes,
        duration_minutes: item.duration_minutes,
        start_flex_minutes: item.start_flex_minutes,
        duration_by_crew_size: item.duration_by_crew_size,
        completion_warning_lead_minutes: item.completion_warning_lead_minutes,
        publication_state: publication_state_value(item.publication_state).to_string(),
        publish_trigger_mode: publish_trigger_mode_value(item.publish_trigger_mode).to_string(),
        publish_at: item.publish_at,
        publish_offset_minutes: item.publish_offset_minutes,
        publish_event_code: item.publish_event_code,
        notes: item.notes,
        published_at: item.published_at,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub fn to_adjustment_rule_response(item: GenerationAdjustmentRule) -> GenerationAdjustmentRuleResponse {
    GenerationAdjustmentRuleResponse {
        id: item.id,
        department_id: item.department_id,
        task_type: item.task_type,
        version_no: item.version_no,
        status: department_rule_status_value(item.status).to_string(),
        rule_name: item.rule_name,
        conditions: item.conditions,
        actions: item.actions,
        notes: item.notes,
        published_at: item.published_at,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub fn to_temporary_task_template_response(item: TemporaryTaskTemplate) -> TemporaryTaskTemplateResponse {
    TemporaryTaskTemplateResponse {
        id: item.id,
        department_id: item.department_id,
        template_code: item.template_code,
        template_name: item.template_name,
        task_type: item.task_type,
        crew_requirements: item
            .crew_requirements
            .into_iter()
            .map(to_task_type_crew_slot_requirement_schema)
            .collect(),
        equipment_requirements: item
            .equipment_requirements
            .into_iter()
            .map(to_task_type_equipment_requirement_schema)
            .collect(),
        notes: item.notes,
        is_active: item.is_active,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub fn to_shift_template_response(item: ShiftTemplate) -> ShiftTemplateResponse {
    ShiftTemplateResponse {
        id: item.id,
        name: item.name,
        resource_type: item.resource_type,
        resource_id: item.resource_id,
        terminal: item.terminal,
        start_time_local: item.start_time_local,
        end_time_local: item.end_time_local,
        weekdays: item.weekdays,
        max_continuous_minutes: item.max_continuous_minutes,
        min_rest_minutes: item.min_rest_minutes,
        enabled: item.enabled,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub fn to_shift_instance_response(item: ShiftInstance) -> ShiftInstanceResponse {
    ShiftInstanceResponse {
        id: item.id,
        template_id: item.template_id,
        resource_type: item.resource_type,
        resource_id: item.resource_id,
        terminal: item.terminal,
        start_time: item.start_time,
        end_time: item.end_time,
        status: item.status,
        max_continuous_minutes: item.max_continuous_minutes,
        min_rest_minutes: item.min_rest_minutes,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub fn team_status_value(status: TeamStatus) -> &'static str {
    match status {
        TeamStatus::OnDuty => "on_duty",
        TeamStatus::OffDuty => "off_duty",
        TeamStatus::Break => "break",
    }
}

pub fn member_role_value(role: MemberRole) -> &'static str {
    match role {
        MemberRole::Leader => "leader",
        MemberRole::Member => "member",
        MemberRole::Driver => "driver",
    }
}

pub fn department_rule_status_value(status: DepartmentRuleStatus) -> &'static str {
    match status {
        DepartmentRuleStatus::Draft => "draft",
        DepartmentRuleStatus::Published => "published",
        DepartmentRuleStatus::Archived => "archived",
    }
}

pub fn leg_scope_value(scope: LegScope) -> &'static str {
    match scope {
        LegScope::Inbound => "inbound",
        LegScope::Outbound => "outbound",
        LegScope::None => "none",
    }
}

pub fn publication_state_value(state: DispatchPublicationState) -> &'static str {
    match state {
        DispatchPublicationState::Prepublished => "prepublished",
        DispatchPublicationState::Published => "published",
        DispatchPublicationState::Cancelled => "cancelled",
    }
}

pub fn publish_trigger_mode_value(mode: PublishTriggerMode) -> &'static str {
    match mode {
        PublishTriggerMode::Time => "time",
        PublishTriggerMode::Event => "event",
        PublishTriggerMode::Either => "either",
        PublishTriggerMode::BothRequired => "both_required",
    }
}

pub fn turnaround_constraint_mode_value(mode: TurnaroundConstraintMode) -> &'static str {
    match mode {
        TurnaroundConstraintMode::SamePerson => "same_person",
        TurnaroundConstraintMode::SoftPreferSamePerson => "soft_prefer_same_person",
        TurnaroundConstraintMode::HandoverRequired => "handover_required",
        TurnaroundConstraintMode::Disabled => "disabled",
    }
}

pub fn qualification_grant_status_value(status: QualificationGrantStatus) -> &'static str {
    match status {
        QualificationGrantStatus::Active => "active",
        QualificationGrantStatus::Expired => "expired",
        QualificationGrantStatus::Suspended => "suspended",
    }
}

pub fn equipment_status_value(status: EquipmentStatus) -> &'static str {
    match status {
        EquipmentStatus::Available => "available",
        EquipmentStatus::InUse => "in_use",
        EquipmentStatus::Maintenance => "maintenance",
        EquipmentStatus::Retired => "retired",
    }
}

pub fn personnel_status_value(status: PersonnelStatus) -> &'static str {
    match status {
        PersonnelStatus::OnDuty => "on_duty",
        PersonnelStatus::OffDuty => "off_duty",
        PersonnelStatus::Break => "break",
        PersonnelStatus::OnLeave => "on_leave",
    }
}

pub fn parse_personnel_status(value: &str) -> Result<PersonnelStatus, fms_domain::error::DomainError> {
    match value.trim() {
        "on_duty" => Ok(PersonnelStatus::OnDuty),
        "off_duty" => Ok(PersonnelStatus::OffDuty),
        "break" => Ok(PersonnelStatus::Break),
        "on_leave" => Ok(PersonnelStatus::OnLeave),
        _ => Err(fms_domain::error::DomainError::ValidationError(
            "status must be one of: on_duty, off_duty, break, on_leave".into(),
        )),
    }
}

pub fn extract_department_id_from_body(body: &Value) -> Result<String, fms_domain::error::DomainError> {
    body.get("department_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| fms_domain::error::DomainError::ValidationError("缺少 department_id".into()))
}

pub fn normalize_status_filters(raw: Option<&str>) -> Vec<&str> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect()
}

pub fn parse_comma_separated_ids(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn build_dispatch_timeline_envelope_bytes(payload: &Value) -> Vec<u8> {
    let mut out = Vec::new();

    encode_string_field(1, &string_value(payload.get("view_mode")), &mut out);
    encode_bool_field(2, bool_value(payload.get("is_admin")), &mut out);
    encode_string_field(3, &string_value(payload.get("window_start")), &mut out);
    encode_string_field(4, &string_value(payload.get("window_end")), &mut out);
    encode_string_field(5, &string_value(payload.get("generated_at")), &mut out);

    if let Some(status_counts) = payload.get("status_counts").and_then(Value::as_object) {
        for (status, count) in status_counts {
            let mut message = Vec::new();
            encode_string_field(1, status, &mut message);
            encode_i32_field(2, int_value(Some(count)), &mut message);
            encode_message_field(6, &message, &mut out);
        }
    }

    if let Some(status_orders) = payload.get("status_orders").and_then(Value::as_object) {
        for (status, orders) in status_orders {
            let mut bucket = Vec::new();
            encode_string_field(1, status, &mut bucket);
            for order in array_values(Some(orders)) {
                let mut entry = Vec::new();
                encode_string_field(1, &string_from_object(order, &["order_id"]), &mut entry);
                encode_string_field(2, &string_from_object(order, &["flight_id"]), &mut entry);
                encode_string_field(3, &string_from_object(order, &["flight_no"]), &mut entry);
                encode_string_field(4, &string_from_object(order, &["task_type_name"]), &mut entry);
                encode_string_field(5, &string_from_object(order, &["status"]), &mut entry);
                encode_string_field(6, &string_from_object(order, &["label"]), &mut entry);
                encode_string_field(7, &string_from_object(order, &["start_time"]), &mut entry);
                encode_string_field(8, &string_from_object(order, &["end_time"]), &mut entry);
                encode_string_field(9, &string_from_object(order, &["focus_item_id"]), &mut entry);
                encode_message_field(2, &entry, &mut bucket);
            }
            encode_message_field(7, &bucket, &mut out);
        }
    }

    for lane in array_values(payload.get("lanes")) {
        let mut message = Vec::new();
        encode_string_field(1, &string_from_object(lane, &["id"]), &mut message);
        encode_string_field(2, &string_from_object(lane, &["label"]), &mut message);
        encode_i32_field(3, int_from_object(lane, &["index"]), &mut message);
        encode_i32_field(
            4,
            int_from_object_with_default(lane, &["subtrack_count"], 1),
            &mut message,
        );
        encode_i32_field(5, int_from_object(lane, &["item_count"]), &mut message);
        encode_string_field(6, &string_from_object(lane, &["resource_type"]), &mut message);
        encode_string_field(7, &string_from_object(lane, &["resource_id"]), &mut message);
        encode_string_field(8, &string_from_object(lane, &["resource_label"]), &mut message);
        encode_message_field(8, &message, &mut out);
    }

    for item in array_values(payload.get("items")) {
        let mut message = Vec::new();
        encode_string_field(1, &string_from_object(item, &["id"]), &mut message);
        encode_string_field(2, &string_from_object(item, &["order_id"]), &mut message);
        encode_string_field(3, &string_from_object(item, &["flight_id"]), &mut message);
        encode_string_field(4, &string_from_object(item, &["flight_no"]), &mut message);
        encode_string_field(5, &string_from_object(item, &["task_type"]), &mut message);
        encode_string_field(6, &string_from_object(item, &["task_type_name"]), &mut message);
        encode_string_field(7, &string_from_object(item, &["status"]), &mut message);
        encode_string_field(8, &string_from_object(item, &["start_time"]), &mut message);
        encode_string_field(9, &string_from_object(item, &["end_time"]), &mut message);
        encode_string_field(10, &string_from_object(item, &["lane_id"]), &mut message);
        encode_string_field(11, &string_from_object(item, &["lane_label"]), &mut message);
        encode_i32_field(12, int_from_object(item, &["lane_index"]), &mut message);
        encode_i32_field(13, int_from_object(item, &["lane_subtrack"]), &mut message);
        encode_i32_field(
            14,
            int_from_object_with_default(item, &["lane_subtrack_count"], 1),
            &mut message,
        );
        encode_string_field(15, &string_from_object(item, &["team_id"]), &mut message);
        encode_string_field(16, &string_from_object(item, &["team_name"]), &mut message);
        encode_string_field(17, &string_from_object(item, &["individual_user_id"]), &mut message);
        encode_string_field(18, &string_from_object(item, &["individual_username"]), &mut message);
        encode_string_field(19, &string_from_object(item, &["stand_id"]), &mut message);
        encode_string_field(20, &string_from_object(item, &["stand_code"]), &mut message);
        encode_string_field(21, &string_from_object(item, &["terminal"]), &mut message);
        encode_string_field(22, &string_from_object(item, &["source"]), &mut message);
        encode_string_field(23, &string_from_object(item, &["dispatch_type"]), &mut message);

        for member in array_values(item.get("members")) {
            encode_message_field(24, &encode_dispatch_profile(member), &mut message);
        }
        for equipment in array_values(item.get("equipments")) {
            encode_message_field(25, &encode_dispatch_profile(equipment), &mut message);
        }
        for member_name in string_list_value(item.get("member_names")) {
            encode_string_field(26, &member_name, &mut message);
        }
        for equipment_code in string_list_value(item.get("equipment_codes")) {
            encode_string_field(27, &equipment_code, &mut message);
        }

        encode_string_field(28, &string_from_object(item, &["label"]), &mut message);
        encode_bool_field(
            29,
            item.get("is_flight_summary").and_then(Value::as_bool).unwrap_or(false),
            &mut message,
        );
        for order_id in string_list_value(item.get("related_order_ids")) {
            encode_string_field(30, &order_id, &mut message);
        }
        for related_order in json_string_list(item.get("related_orders")) {
            encode_string_field(31, &related_order, &mut message);
        }

        encode_string_field(32, &string_from_object(item, &["focus_user_id"]), &mut message);
        encode_string_field(33, &string_from_object(item, &["focus_user_name"]), &mut message);
        encode_string_field(34, &string_from_object(item, &["focus_equipment_id"]), &mut message);
        encode_string_field(35, &string_from_object(item, &["focus_equipment_code"]), &mut message);
        encode_string_field(
            36,
            &string_from_object(item, &["department_rule_version"]),
            &mut message,
        );
        encode_string_field(
            37,
            &json_string_value(item.get("crew_requirement_snapshot")),
            &mut message,
        );
        encode_string_field(38, &json_string_value(item.get("task_crew")), &mut message);
        encode_string_field(39, &json_string_value(item.get("qualification_gap")), &mut message);
        encode_message_field(9, &message, &mut out);
    }

    out
}

fn encode_dispatch_profile(profile: &Value) -> Vec<u8> {
    let mut message = Vec::new();
    encode_string_field(
        1,
        &string_from_object(profile, &["id", "user_id", "equipment_id"]),
        &mut message,
    );
    encode_string_field(
        2,
        &string_from_object(profile, &["name", "username", "code"]),
        &mut message,
    );
    message
}

fn encode_message_field(field_number: u32, payload: &[u8], out: &mut Vec<u8>) {
    if payload.is_empty() {
        return;
    }
    encode_key(field_number, 2, out);
    encode_varint(payload.len() as u64, out);
    out.extend_from_slice(payload);
}

fn encode_string_field(field_number: u32, value: &str, out: &mut Vec<u8>) {
    if value.is_empty() {
        return;
    }
    encode_key(field_number, 2, out);
    encode_varint(value.len() as u64, out);
    out.extend_from_slice(value.as_bytes());
}

fn encode_bool_field(field_number: u32, value: bool, out: &mut Vec<u8>) {
    if !value {
        return;
    }
    encode_key(field_number, 0, out);
    encode_varint(u64::from(value), out);
}

fn encode_i32_field(field_number: u32, value: i32, out: &mut Vec<u8>) {
    if value == 0 {
        return;
    }
    encode_key(field_number, 0, out);
    encode_varint(value as u64, out);
}

fn encode_key(field_number: u32, wire_type: u8, out: &mut Vec<u8>) {
    encode_varint(((field_number << 3) | u32::from(wire_type)) as u64, out);
}

fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn array_values(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(other) => vec![other],
        None => Vec::new(),
    }
}

fn bool_value(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_i64().unwrap_or(0) != 0,
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
        _ => false,
    }
}

fn int_value(value: Option<&Value>) -> i32 {
    int_value_with_default(value, 0)
}

fn int_value_with_default(value: Option<&Value>, default: i32) -> i32 {
    match value {
        Some(Value::Number(number)) => number
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(default),
        Some(Value::String(text)) if !text.is_empty() => text.parse::<i32>().unwrap_or(default),
        Some(Value::Bool(flag)) => i32::from(*flag),
        _ => default,
    }
}

fn int_from_object(value: &Value, keys: &[&str]) -> i32 {
    int_value(first_object_value(value, keys))
}

fn int_from_object_with_default(value: &Value, keys: &[&str], default: i32) -> i32 {
    int_value_with_default(first_object_value(value, keys), default)
}

fn string_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::Null) | None => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(flag)) => {
            if *flag {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        _ => String::new(),
    }
}

fn string_from_object(value: &Value, keys: &[&str]) -> String {
    string_value(first_object_value(value, keys))
}

fn first_object_value<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(found) = object.get(*key) {
            return Some(found);
        }
    }
    None
}

fn string_list_value(value: Option<&Value>) -> Vec<String> {
    array_values(value)
        .into_iter()
        .filter_map(|item| {
            let text = string_value(Some(item));
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .collect()
}

fn json_string_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::Null) | None => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn json_string_list(value: Option<&Value>) -> Vec<String> {
    array_values(value)
        .into_iter()
        .filter_map(|item| {
            let text = json_string_value(Some(item));
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .collect()
}
