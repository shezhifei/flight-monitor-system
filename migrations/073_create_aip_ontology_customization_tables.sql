-- AIP (Artificial Intelligence Platform) Ontology Customization Tables
-- These tables store customizable AI module components that can be managed via CRUD


-- ---------------------------------------------------------------------------
-- Ontology Object Definitions - Define object types (Flight, Stand, Team, etc.)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS aip_ontology_objects (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL UNIQUE,
    plural_name VARCHAR(128),
    description TEXT,
    is_abstract BOOLEAN NOT NULL DEFAULT FALSE,
    properties JSONB NOT NULL DEFAULT '[]'::jsonb,
    relationships JSONB NOT NULL DEFAULT '[]'::jsonb,
    actions JSONB NOT NULL DEFAULT '[]'::jsonb,
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_aip_ontology_objects_name ON aip_ontology_objects(name);
CREATE INDEX idx_aip_ontology_objects_active ON aip_ontology_objects(is_active);

COMMENT ON TABLE aip_ontology_objects IS 'Ontology对象类型定义 - 定义Flight、Stand、Team等对象的属性和关系';
COMMENT ON COLUMN aip_ontology_objects.properties IS '属性定义数组: [{name, type, required, description, enum_values, reference_object, default}]';
COMMENT ON COLUMN aip_ontology_objects.relationships IS '关系定义数组: [{name, target_object, cardinality, description, inverse}]';
COMMENT ON COLUMN aip_ontology_objects.actions IS '动作名称数组: [action_name1, action_name2]';

-- ---------------------------------------------------------------------------
-- Ontology Action Definitions - Define actions on objects
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS aip_ontology_actions (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    object_type VARCHAR(128) NOT NULL,
    description TEXT,
    category VARCHAR(32) NOT NULL DEFAULT 'mutation',
    parameters JSONB NOT NULL DEFAULT '[]'::jsonb,
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    risk_level VARCHAR(16) NOT NULL DEFAULT 'NORMAL',
    constraint_rules JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_aip_ontology_actions_object_action UNIQUE (object_type, name)
);

CREATE INDEX idx_aip_ontology_actions_object ON aip_ontology_actions(object_type);
CREATE INDEX idx_aip_ontology_actions_risk ON aip_ontology_actions(risk_level);
CREATE INDEX idx_aip_ontology_actions_active ON aip_ontology_actions(is_active);

COMMENT ON TABLE aip_ontology_actions IS 'Ontology动作定义 - 定义对象可执行的操作';
COMMENT ON COLUMN aip_ontology_actions.parameters IS '参数定义: [{name, type, required, description, enum_values}]';
COMMENT ON COLUMN aip_ontology_actions.constraint_rules IS '约束规则: [{type, expression, error_message}]';
COMMENT ON COLUMN aip_ontology_actions.risk_level IS '风险等级: LOW, NORMAL, MEDIUM, HIGH, CRITICAL';

-- ---------------------------------------------------------------------------
-- Object Policies (ACL) - Fine-grained permission control
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS aip_object_policies (
    id VARCHAR(64) PRIMARY KEY,
    object_type VARCHAR(128) NOT NULL,
    object_id VARCHAR(255),
    principal_type VARCHAR(32) NOT NULL DEFAULT 'user',
    principal_id VARCHAR(255) NOT NULL,
    permission VARCHAR(32) NOT NULL,
    granted BOOLEAN NOT NULL DEFAULT TRUE,
    conditions JSONB,
    description TEXT,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_aip_object_policies_principal_object_permission UNIQUE (principal_type, principal_id, object_type, object_id, permission)
);

CREATE INDEX idx_aip_object_policies_principal ON aip_object_policies(principal_type, principal_id);
CREATE INDEX idx_aip_object_policies_object ON aip_object_policies(object_type, object_id);
CREATE INDEX idx_aip_object_policies_permission ON aip_object_policies(permission);
CREATE INDEX idx_aip_object_policies_expires ON aip_object_policies(expires_at) WHERE expires_at IS NOT NULL;

COMMENT ON TABLE aip_object_policies IS '对象级ACL策略 - 控制用户对对象的访问权限';
COMMENT ON COLUMN aip_object_policies.conditions IS '条件表达式: {attribute: value} - 只有满足条件时才生效';

-- ---------------------------------------------------------------------------
-- AIP Function Registry - Maps Ontology actions to LLM-callable functions
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS aip_functions (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL UNIQUE,
    category VARCHAR(32) NOT NULL DEFAULT 'object_action',
    object_type VARCHAR(128) NOT NULL,
    action_name VARCHAR(128) NOT NULL,
    description TEXT,
    parameters_schema JSONB NOT NULL DEFAULT '{}'::jsonb,
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    risk_level VARCHAR(16) NOT NULL DEFAULT 'NORMAL',
    permission_required VARCHAR(255),
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    examples JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_aip_functions_name ON aip_functions(name);
CREATE INDEX idx_aip_functions_object ON aip_functions(object_type, action_name);
CREATE INDEX idx_aip_functions_category ON aip_functions(category);
CREATE INDEX idx_aip_functions_active ON aip_functions(is_active);

COMMENT ON TABLE aip_functions IS 'AIP函数注册表 - 将Ontology动作映射为LLM可调用的函数';
COMMENT ON COLUMN aip_functions.parameters_schema IS 'OpenAI function calling格式: {type, properties: {}, required: []}';

-- ---------------------------------------------------------------------------
-- Tool Mappings - Legacy tool to AIP object mappings
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS aip_tool_mappings (
    id VARCHAR(64) PRIMARY KEY,
    tool_name VARCHAR(128) NOT NULL UNIQUE,
    object_type VARCHAR(128) NOT NULL,
    action_name VARCHAR(128) NOT NULL,
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    risk_level VARCHAR(16) NOT NULL DEFAULT 'NORMAL',
    migration_status VARCHAR(32) NOT NULL DEFAULT 'not_started',
    custom_handler TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_aip_tool_mappings_tool ON aip_tool_mappings(tool_name);
CREATE INDEX idx_aip_tool_mappings_object ON aip_tool_mappings(object_type);
CREATE INDEX idx_aip_tool_mappings_status ON aip_tool_mappings(migration_status);

COMMENT ON TABLE aip_tool_mappings IS '工具映射配置 - 将Legacy工具映射到AIP对象和动作';
COMMENT ON COLUMN aip_tool_mappings.migration_status IS '迁移状态: not_started, in_progress, completed';

-- ---------------------------------------------------------------------------
-- Constraint Definitions - Business rules for operations
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS aip_constraints (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    object_type VARCHAR(128) NOT NULL,
    action_name VARCHAR(128),
    constraint_type VARCHAR(32) NOT NULL,
    expression TEXT NOT NULL,
    error_message TEXT,
    severity VARCHAR(16) NOT NULL DEFAULT 'ERROR',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_aip_constraints_object_action_name UNIQUE (object_type, action_name, name)
);

CREATE INDEX idx_aip_constraints_object ON aip_constraints(object_type, action_name);
CREATE INDEX idx_aip_constraints_type ON aip_constraints(constraint_type);
CREATE INDEX idx_aip_constraints_active ON aip_constraints(is_active);

COMMENT ON TABLE aip_constraints IS '业务约束定义 - 定义对象操作需满足的业务规则';
COMMENT ON COLUMN aip_constraints.constraint_type IS '约束类型: validation, business_rule, capacity, availability';
COMMENT ON COLUMN aip_constraints.expression IS '约束表达式: 如 "stand.capacity > 0", "team.available == true"';

-- ---------------------------------------------------------------------------
-- Seed default data for AIP Ontology
-- ---------------------------------------------------------------------------

-- Insert default ontology objects
INSERT INTO aip_ontology_objects (id, name, plural_name, description, properties, relationships, actions, tags)
VALUES
    ('obj_flight', 'Flight', 'Flights', '航班对象 - 管理系统中的航班实体',
     '[{"name": "flight_number", "type": "string", "required": true, "description": "航班号"}, {"name": "stand", "type": "string", "description": "停机位"}, {"name": "status", "type": "string", "enum_values": ["scheduled", "arrived", "departed", "cancelled"], "description": "航班状态"}, {"name": "eta", "type": "datetime", "description": "预计到达时间"}, {"name": "etd", "type": "datetime", "description": "预计起飞时间"}]',
     '[{"name": "stand", "target_object": "Stand", "cardinality": "one", "description": "分配的停机位"}, {"name": "team_assignments", "target_object": "Team", "cardinality": "many", "description": "分配的班组"}]',
     '["change_stand", "delay_flight", "assign_team", "update_status", "mark_arrived", "mark_departed"]',
     '["core", "flight"]'),
    ('obj_stand', 'Stand', 'Stands', '停机位对象 - 管理机坪停机位',
     '[{"name": "stand_id", "type": "string", "required": true, "description": "停机位编号"}, {"name": "status", "type": "string", "enum_values": ["available", "occupied", "reserved", "closed"], "description": "状态"}, {"name": "capacity", "type": "integer", "description": "容量"}]',
     '[{"name": "current_flight", "target_object": "Flight", "cardinality": "one", "description": "当前停靠的航班"}]',
     '["occupy", "release", "reserve", "close", "update_status"]',
     '["core", "resource"]'),
    ('obj_team', 'Team', 'Teams', '班组对象 - 管理工作人员班组',
     '[{"name": "team_id", "type": "string", "required": true, "description": "班组编号"}, {"name": "status", "type": "string", "enum_values": ["available", "busy", "off_duty"], "description": "状态"}, {"name": "location", "type": "string", "description": "当前位置"}]',
     '[{"name": "assigned_flights", "target_object": "Flight", "cardinality": "many", "description": "分配的航班"}]',
     '["assign_flight", "update_status", "change_location"]',
     '["core", "resource"]'),
    ('obj_anomaly', 'Anomaly', 'Anomalies', '异常对象 - 管理系统中的异常事件',
     '[{"name": "anomaly_type", "type": "string", "required": true, "description": "异常类型"}, {"name": "severity", "type": "string", "enum_values": ["low", "medium", "high", "critical"], "description": "严重程度"}, {"name": "status", "type": "string", "enum_values": ["open", "acknowledged", "resolved", "escalated"], "description": "状态"}]',
     '[{"name": "related_flight", "target_object": "Flight", "cardinality": "one", "description": "关联航班"}, {"name": "assigned_team", "target_object": "Team", "cardinality": "one", "description": "负责班组"}]',
     '["acknowledge", "assign_team", "resolve", "escalate"]',
     '["alert", "incident"]'),
    ('obj_todo', 'Todo', 'Todos', '待办事项对象 - 管理系统中的任务',
     '[{"name": "title", "type": "string", "required": true, "description": "标题"}, {"name": "priority", "type": "string", "enum_values": ["low", "medium", "high", "urgent"], "description": "优先级"}, {"name": "status", "type": "string", "enum_values": ["pending", "in_progress", "completed", "cancelled"], "description": "状态"}]',
     '[{"name": "assignee", "target_object": "Team", "cardinality": "one", "description": "负责人"}, {"name": "related_flight", "target_object": "Flight", "cardinality": "one", "description": "关联航班"}]',
     '["create", "complete", "assign", "update_status"]',
     '["task", "workflow"]')
ON CONFLICT (name) DO NOTHING;

-- Insert default ontology actions
INSERT INTO aip_ontology_actions (id, name, object_type, description, parameters, requires_approval, risk_level)
VALUES
    ('act_change_stand', 'change_stand', 'Flight', '更改航班停机位', '[{"name": "stand_id", "type": "string", "required": true, "description": "目标停机位"}]', true, 'MEDIUM'),
    ('act_delay_flight', 'delay_flight', 'Flight', '延迟航班', '[{"name": "delay_minutes", "type": "integer", "required": true, "description": "延迟分钟数"}]', false, 'LOW'),
    ('act_assign_team', 'assign_team', 'Flight', '分配班组到航班', '[{"name": "team_id", "type": "string", "required": true, "description": "班组ID"}]', true, 'MEDIUM'),
    ('act_update_status', 'update_status', 'Flight', '更新航班状态', '[{"name": "status", "type": "string", "required": true, "description": "新状态"}]', true, 'HIGH'),
    ('act_occupy', 'occupy', 'Stand', '占用停机位', '[{"name": "flight_id", "type": "string", "required": true, "description": "航班ID"}]', false, 'LOW'),
    ('act_release', 'release', 'Stand', '释放停机位', '[]', false, 'LOW'),
    ('act_reserve', 'reserve', 'Stand', '预定停机位', '[{"name": "flight_id", "type": "string", "required": true}, {"name": "duration_minutes", "type": "integer"}]', true, 'MEDIUM'),
    ('act_close', 'close', 'Stand', '关闭停机位', '[{"name": "reason", "type": "string", "description": "关闭原因"}]', true, 'MEDIUM'),
    ('act_acknowledge', 'acknowledge', 'Anomaly', '确认异常', '[]', false, 'LOW'),
    ('act_resolve', 'resolve', 'Anomaly', '解决异常', '[{"name": "resolution", "type": "string", "required": true, "description": "解决方案"}]', true, 'MEDIUM'),
    ('act_escalate', 'escalate', 'Anomaly', '升级异常', '[{"name": "escalation_level", "type": "integer", "required": true}]', true, 'HIGH'),
    ('act_create_todo', 'create', 'Todo', '创建待办', '[{"name": "title", "type": "string", "required": true}, {"name": "priority", "type": "string", "enum_values": ["low", "medium", "high", "urgent"]}]', false, 'LOW'),
    ('act_complete_todo', 'complete', 'Todo', '完成待办', '[]', false, 'LOW')
ON CONFLICT (object_type, name) DO NOTHING;

-- Insert default tool mappings (22 legacy tools)
INSERT INTO aip_tool_mappings (id, tool_name, object_type, action_name, requires_approval, risk_level, migration_status)
VALUES
    ('map_change_flight_stand', 'change_flight_stand', 'Flight', 'change_stand', true, 'MEDIUM', 'in_progress'),
    ('map_delay_flight', 'delay_flight', 'Flight', 'delay_flight', false, 'LOW', 'in_progress'),
    ('map_assign_team_to_flight', 'assign_team_to_flight', 'Flight', 'assign_team', true, 'MEDIUM', 'in_progress'),
    ('map_update_flight_status', 'update_flight_status', 'Flight', 'update_status', true, 'HIGH', 'in_progress'),
    ('map_mark_flight_arrived', 'mark_flight_arrived', 'Flight', 'mark_arrived', false, 'LOW', 'in_progress'),
    ('map_mark_flight_departed', 'mark_flight_departed', 'Flight', 'mark_departed', false, 'LOW', 'in_progress'),
    ('map_occupy_stand', 'occupy_stand', 'Stand', 'occupy', false, 'LOW', 'in_progress'),
    ('map_release_stand', 'release_stand', 'Stand', 'release', false, 'LOW', 'in_progress'),
    ('map_reserve_stand', 'reserve_stand', 'Stand', 'reserve', true, 'MEDIUM', 'in_progress'),
    ('map_close_stand', 'close_stand', 'Stand', 'close', true, 'MEDIUM', 'in_progress'),
    ('map_update_stand_status', 'update_stand_status', 'Stand', 'update_status', false, 'LOW', 'in_progress'),
    ('map_assign_flight_to_team', 'assign_flight_to_team', 'Team', 'assign_flight', false, 'LOW', 'in_progress'),
    ('map_update_team_status', 'update_team_status', 'Team', 'update_status', false, 'LOW', 'in_progress'),
    ('map_change_team_location', 'change_team_location', 'Team', 'change_location', false, 'LOW', 'in_progress'),
    ('map_acknowledge_anomaly', 'acknowledge_anomaly', 'Anomaly', 'acknowledge', false, 'LOW', 'in_progress'),
    ('map_assign_team_to_anomaly', 'assign_team_to_anomaly', 'Anomaly', 'assign_team', false, 'LOW', 'in_progress'),
    ('map_resolve_anomaly', 'resolve_anomaly', 'Anomaly', 'resolve', true, 'MEDIUM', 'in_progress'),
    ('map_escalate_anomaly', 'escalate_anomaly', 'Anomaly', 'escalate', true, 'HIGH', 'in_progress'),
    ('map_create_todo', 'create_todo', 'Todo', 'create', false, 'LOW', 'in_progress'),
    ('map_complete_todo', 'complete_todo', 'Todo', 'complete', false, 'LOW', 'in_progress'),
    ('map_assign_todo', 'assign_todo', 'Todo', 'assign', false, 'LOW', 'in_progress')
ON CONFLICT (tool_name) DO NOTHING;

-- Insert default AIP functions
INSERT INTO aip_functions (id, name, category, object_type, action_name, description, parameters_schema, requires_approval, risk_level)
VALUES
    ('fn_change_stand', 'Flight.change_stand', 'object_action', 'Flight', 'change_stand', '更改航班停机位', '{"type": "object", "properties": {"stand_id": {"type": "string", "description": "目标停机位"}}, "required": ["stand_id"]}', true, 'MEDIUM'),
    ('fn_delay_flight', 'Flight.delay_flight', 'object_action', 'Flight', 'delay_flight', '延迟航班', '{"type": "object", "properties": {"delay_minutes": {"type": "integer", "description": "延迟分钟数"}}, "required": ["delay_minutes"]}', false, 'LOW'),
    ('fn_assign_team', 'Flight.assign_team', 'object_action', 'Flight', 'assign_team', '分配班组', '{"type": "object", "properties": {"team_id": {"type": "string", "description": "班组ID"}}, "required": ["team_id"]}', true, 'MEDIUM'),
    ('fn_occupy_stand', 'Stand.occupy', 'object_action', 'Stand', 'occupy', '占用停机位', '{"type": "object", "properties": {"flight_id": {"type": "string", "description": "航班ID"}}, "required": ["flight_id"]}', false, 'LOW'),
    ('fn_resolve_anomaly', 'Anomaly.resolve', 'object_action', 'Anomaly', 'resolve', '解决异常', '{"type": "object", "properties": {"resolution": {"type": "string", "description": "解决方案"}}, "required": ["resolution"]}', true, 'MEDIUM'),
    ('fn_create_todo', 'Todo.create', 'object_action', 'Todo', 'create', '创建待办', '{"type": "object", "properties": {"title": {"type": "string"}, "priority": {"type": "string", "enum": ["low", "medium", "high", "urgent"]}}, "required": ["title"]}', false, 'LOW')
ON CONFLICT (name) DO NOTHING;

-- Insert default constraints
INSERT INTO aip_constraints (id, name, object_type, action_name, constraint_type, expression, error_message, severity)
VALUES
    ('const_stand_capacity', 'stand_capacity_check', 'Stand', 'occupy', 'capacity', 'stand.capacity > 0', '停机位容量必须大于0', 'ERROR'),
    ('const_stand_available', 'stand_availability', 'Stand', 'occupy', 'availability', 'stand.status == "available"', '停机位不可用', 'ERROR'),
    ('const_team_available', 'team_availability', 'Team', 'assign_flight', 'availability', 'team.status == "available"', '班组不可用', 'ERROR'),
    ('const_flight_status', 'flight_mutable_status', 'Flight', 'change_stand', 'business_rule', 'flight.status in ["scheduled", "arrived"]', '航班状态不允许更改停机位', 'ERROR')
ON CONFLICT (object_type, action_name, name) DO NOTHING;

