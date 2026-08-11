-- =====================================================
-- 119: Ontology V1 core — 飞机中心本体
--
-- 对应 docs/architecture/ONTOLOGY_V1.md §4/§6/§10：
--  - aircraft: registration 原样唯一（§4.1, 不变量 1）
--  - stand_occupations: 机位占用，主体是飞机（§4.4, 不变量 3）
--  - gate_assignments: 登机口分配，首次分配即生效（§4.5）
--  - turnaround_links: 进-出任务衔接边（§4.8, 不变量 4）
--  - resource_adjustment_suggestions: 分权建议，接受即执行（§4.9）
--  - flights 新列: direction / flight_kind / is_draft / divert（§4.2）
--  - AOC / TOC / GROUND 岗位角色 + 细粒度权限（§3）
--
-- 冲突（机位重叠等）为告警不硬拦；本迁移只做结构约束。
-- 禁双岗（§3.2, 不变量 9）由应用层 enforce。
-- =====================================================

-- -----------------------------------------------------
-- 1. flights 本体列
-- -----------------------------------------------------
ALTER TABLE flights
    ADD COLUMN IF NOT EXISTS direction VARCHAR(16),
    ADD COLUMN IF NOT EXISTS flight_kind VARCHAR(32) NOT NULL DEFAULT 'passenger',
    ADD COLUMN IF NOT EXISTS is_draft BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS divert BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN flights.direction IS '航段方向: inbound | outbound | both(现表过站聚合行)';
COMMENT ON COLUMN flights.flight_kind IS '航班种类: passenger | ferry | …(受控码表, 默认 passenger)';
COMMENT ON COLUMN flights.is_draft IS 'draft 计划标记; 批确认前不可被正式 StandOccupation 引用(§3.3, 不变量 5)';
COMMENT ON COLUMN flights.divert IS '备降标记; 本场 ontology + divert 标记(§1.3)';

-- 工单双挂一致性（不变量 7）: DispatchOrder 同时挂航段与机号时须一致
ALTER TABLE dispatch_orders
    ADD COLUMN IF NOT EXISTS aircraft_registration VARCHAR(32);

-- -----------------------------------------------------
-- 2. aircraft — 飞机中心（registration 原样 + 唯一）
-- -----------------------------------------------------
CREATE TABLE IF NOT EXISTS aircraft (
    registration VARCHAR(32) PRIMARY KEY,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notes TEXT
);

COMMENT ON TABLE aircraft IS '飞机(本体中心); registration 原样存储并全局唯一(ONTOLOGY_V1 §4.1)';
COMMENT ON COLUMN aircraft.notes IS '自由备注; 机位/口不使用简单标量, 见 stand_occupations/gate_assignments';

-- -----------------------------------------------------
-- 3. stand_occupations — 机位占用（主体=飞机）
-- -----------------------------------------------------
CREATE TABLE IF NOT EXISTS stand_occupations (
    id VARCHAR(26) PRIMARY KEY,
    registration VARCHAR(32) NOT NULL REFERENCES aircraft(registration) ON DELETE RESTRICT,
    stand_code VARCHAR(32) NOT NULL,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    kind VARCHAR(16) NOT NULL DEFAULT 'normal',
    moving_to_stand VARCHAR(32),
    flight_id VARCHAR(26) REFERENCES flights(flight_id) ON DELETE SET NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    created_by VARCHAR(26),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_stand_occupation_time CHECK (ends_at > starts_at),
    CONSTRAINT chk_stand_occupation_kind CHECK (kind IN ('normal', 'moving')),
    CONSTRAINT chk_stand_occupation_status CHECK (status IN ('active', 'released', 'expired')),
    CONSTRAINT chk_stand_occupation_moving CHECK (kind <> 'moving' OR moving_to_stand IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_stand_occupations_reg_status
    ON stand_occupations (registration, status, starts_at DESC);
CREATE INDEX IF NOT EXISTS idx_stand_occupations_stand_time
    ON stand_occupations (stand_code, starts_at);
CREATE INDEX IF NOT EXISTS idx_stand_occupations_flight
    ON stand_occupations (flight_id);

COMMENT ON TABLE stand_occupations IS '机位占用: [start,end) + aircraft + stand; 冲突告警不硬拦(§4.4)';
COMMENT ON COLUMN stand_occupations.kind IS 'normal 常规占用; moving 拖曳过渡占用(from_stand→to_stand)';
COMMENT ON COLUMN stand_occupations.status IS 'active 生效; released 释放; expired 过期';
COMMENT ON COLUMN stand_occupations.flight_id IS '可选记录原因航段; 真相在飞机占用';

-- -----------------------------------------------------
-- 4. gate_assignments — 登机口分配（首次分配即生效）
-- -----------------------------------------------------
CREATE TABLE IF NOT EXISTS gate_assignments (
    id VARCHAR(26) PRIMARY KEY,
    registration VARCHAR(32) NOT NULL REFERENCES aircraft(registration) ON DELETE RESTRICT,
    gate_code VARCHAR(32) NOT NULL,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    flight_id VARCHAR(26) REFERENCES flights(flight_id) ON DELETE SET NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    created_by VARCHAR(26),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_gate_assignment_time CHECK (ends_at > starts_at),
    CONSTRAINT chk_gate_assignment_status CHECK (status IN ('active', 'released', 'expired'))
);

CREATE INDEX IF NOT EXISTS idx_gate_assignments_reg_status
    ON gate_assignments (registration, status, starts_at DESC);
CREATE INDEX IF NOT EXISTS idx_gate_assignments_gate_time
    ON gate_assignments (gate_code, starts_at);
CREATE INDEX IF NOT EXISTS idx_gate_assignments_flight
    ON gate_assignments (flight_id);

COMMENT ON TABLE gate_assignments IS '登机口分配: 飞机+时段+gate; 口-位弱校验, 不一致告警(§4.5)';
COMMENT ON COLUMN gate_assignments.status IS 'active 生效; released 释放; expired 过期';

-- -----------------------------------------------------
-- 5. turnaround_links — 进-出任务衔接边（不是机号边）
-- -----------------------------------------------------
CREATE TABLE IF NOT EXISTS turnaround_links (
    id VARCHAR(26) PRIMARY KEY,
    inbound_flight_id VARCHAR(26) NOT NULL REFERENCES flights(flight_id) ON DELETE CASCADE,
    outbound_flight_id VARCHAR(26) NOT NULL REFERENCES flights(flight_id) ON DELETE CASCADE,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    source VARCHAR(16) NOT NULL DEFAULT 'auto',
    broken_reason VARCHAR(255),
    created_by VARCHAR(26),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_turnaround_pair UNIQUE (inbound_flight_id, outbound_flight_id),
    CONSTRAINT chk_turnaround_pair CHECK (inbound_flight_id <> outbound_flight_id),
    CONSTRAINT chk_turnaround_status CHECK (status IN ('active', 'broken')),
    CONSTRAINT chk_turnaround_source CHECK (source IN ('auto', 'manual'))
);

CREATE INDEX IF NOT EXISTS idx_turnaround_links_inbound
    ON turnaround_links (inbound_flight_id, status);
CREATE INDEX IF NOT EXISTS idx_turnaround_links_outbound
    ON turnaround_links (outbound_flight_id, status);

COMMENT ON TABLE turnaround_links IS '周转链接: (inbound_flight_id, outbound_flight_id) 任务对唯一(§4.8, 不变量 4)';
COMMENT ON COLUMN turnaround_links.status IS 'active 健康衔接(同机); broken 已拆(换机后异机)';
COMMENT ON COLUMN turnaround_links.source IS 'auto 系统自动(同机+时间窗); manual 人工维护';

-- -----------------------------------------------------
-- 6. resource_adjustment_suggestions — 分权建议
-- -----------------------------------------------------
CREATE TABLE IF NOT EXISTS resource_adjustment_suggestions (
    id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL REFERENCES flights(flight_id) ON DELETE CASCADE,
    kind VARCHAR(16) NOT NULL,
    current_value VARCHAR(32),
    suggested_value VARCHAR(32) NOT NULL,
    status VARCHAR(24) NOT NULL DEFAULT 'pending',
    reason TEXT,
    payload JSONB NOT NULL DEFAULT '{}',
    created_by VARCHAR(26) NOT NULL,
    decided_by VARCHAR(26),
    decided_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_suggestion_kind CHECK (kind IN ('stand', 'gate')),
    CONSTRAINT chk_suggestion_status CHECK (status IN ('pending', 'accepted_executed', 'rejected', 'expired'))
);

CREATE INDEX IF NOT EXISTS idx_suggestions_flight_status
    ON resource_adjustment_suggestions (flight_id, status);
CREATE INDEX IF NOT EXISTS idx_suggestions_status_created
    ON resource_adjustment_suggestions (status, created_at DESC);

COMMENT ON TABLE resource_adjustment_suggestions IS '资源调整建议: pending→accepted_executed|rejected|expired; 接受=自动执行+回写 Flight 计划字段(§4.9)';
COMMENT ON COLUMN resource_adjustment_suggestions.kind IS 'stand 机位建议(仅 AOC 可接受); gate 登机口建议(仅 TOC 可接受)';
COMMENT ON COLUMN resource_adjustment_suggestions.payload IS '建议 Action 载荷: 内嵌 Allocate/Adjust 参数与触发上下文';

-- -----------------------------------------------------
-- 7. 权限码（细粒度 resource.action 风格）
-- -----------------------------------------------------
INSERT INTO permissions (id, name, description, is_active)
VALUES
    ('perm_onto_read_v2', 'ontology.read', '查看本体资源视图(机位占用/口分配/链接/建议)', TRUE),
    ('perm_onto_reassign_v2', 'ontology.aircraft.reassign', '变更飞机(ReassignAircraft, 仅 AOC)', TRUE),
    ('perm_onto_stand_manage_v2', 'ontology.stand.manage', '正式机位占用分配/调整/释放(仅 AOC)', TRUE),
    ('perm_onto_gate_manage_v2', 'ontology.gate.manage', '正式登机口分配/调整/释放(仅 TOC)', TRUE),
    ('perm_onto_sugg_stand_v2', 'ontology.suggestion.accept_stand', '接受机位建议(仅 AOC)', TRUE),
    ('perm_onto_sugg_gate_v2', 'ontology.suggestion.accept_gate', '接受登机口建议(仅 TOC)', TRUE),
    ('perm_onto_sugg_reject_v2', 'ontology.suggestion.reject', '驳回资源调整建议', TRUE),
    ('perm_onto_confirm_v2', 'ontology.plan.confirm', 'draft 计划整批确认(仅 AOC)', TRUE)
ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description,
    is_active = TRUE,
    updated_at = CURRENT_TIMESTAMP;

-- -----------------------------------------------------
-- 8. 岗位角色: AOC / TOC / GROUND(地服)
-- -----------------------------------------------------
INSERT INTO roles (id, name, description, is_system, is_active)
VALUES
    ('role_ontology_aoc', 'AOC', '运行控制中心: 机号权威、正式机位、draft 确认、机位建议', TRUE, TRUE),
    ('role_ontology_toc', 'TOC', '登机口控制: 正式登机口分配、口建议', TRUE, TRUE),
    ('role_ontology_ground', 'GROUND', '地服: 工单与保障状态; 黑名单=改机号/正式位/正式口', TRUE, TRUE)
ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description,
    is_system = TRUE,
    is_active = TRUE;

-- AOC: 全部本体权限（含建议驳回与只读）
INSERT INTO role_permissions (role_id, permission_id, created_at)
SELECT r.id, p.id, CURRENT_TIMESTAMP
FROM roles r, permissions p
WHERE r.name = 'AOC'
  AND p.name IN (
    'ontology.read',
    'ontology.aircraft.reassign',
    'ontology.stand.manage',
    'ontology.suggestion.accept_stand',
    'ontology.suggestion.reject',
    'ontology.plan.confirm'
  )
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- TOC: 登机口 + 口建议 + 只读 + 驳回
INSERT INTO role_permissions (role_id, permission_id, created_at)
SELECT r.id, p.id, CURRENT_TIMESTAMP
FROM roles r, permissions p
WHERE r.name = 'TOC'
  AND p.name IN (
    'ontology.read',
    'ontology.gate.manage',
    'ontology.suggestion.accept_gate',
    'ontology.suggestion.reject'
  )
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- GROUND(地服): 只读资源 + 驳回建议; 不授予改机号/正式位/正式口
INSERT INTO role_permissions (role_id, permission_id, created_at)
SELECT r.id, p.id, CURRENT_TIMESTAMP
FROM roles r, permissions p
WHERE r.name = 'GROUND'
  AND p.name IN (
    'ontology.read',
    'ontology.suggestion.reject'
  )
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- -----------------------------------------------------
-- 9. 权限模板（管理端可一键套用）
-- -----------------------------------------------------
INSERT INTO permission_templates
    (id, name, code, description, permissions, is_system, category, display_order)
VALUES
    ('tpl_ontology_aoc', 'AOC 运行控制', 'ontology.aoc',
     '本体 V1: AOC 岗位模板(机号权威/正式机位/draft 确认/机位建议)',
     ARRAY['ontology.read', 'ontology.aircraft.reassign', 'ontology.stand.manage',
           'ontology.suggestion.accept_stand', 'ontology.suggestion.reject', 'ontology.plan.confirm'],
     TRUE, 'ontology', 10),
    ('tpl_ontology_toc', 'TOC 登机口控制', 'ontology.toc',
     '本体 V1: TOC 岗位模板(正式登机口/口建议)',
     ARRAY['ontology.read', 'ontology.gate.manage',
           'ontology.suggestion.accept_gate', 'ontology.suggestion.reject'],
     TRUE, 'ontology', 20),
    ('tpl_ontology_ground', 'GROUND 地服', 'ontology.ground',
     '本体 V1: 地服岗位模板(只读资源; 禁改机号/正式位/正式口)',
     ARRAY['ontology.read', 'ontology.suggestion.reject'],
     TRUE, 'ontology', 30)
ON CONFLICT (code) DO UPDATE SET
    name = EXCLUDED.name,
    description = EXCLUDED.description,
    permissions = EXCLUDED.permissions,
    is_system = TRUE,
    is_active = TRUE;

-- 回滚：
-- DELETE FROM permission_templates WHERE code IN ('ontology.aoc', 'ontology.toc', 'ontology.ground');
-- DELETE FROM role_permissions WHERE role_id IN (SELECT id FROM roles WHERE name IN ('AOC','TOC','GROUND'));
-- DELETE FROM roles WHERE name IN ('AOC', 'TOC', 'GROUND');
-- DELETE FROM permissions WHERE name IN (
--     'ontology.read', 'ontology.aircraft.reassign', 'ontology.stand.manage',
--     'ontology.gate.manage', 'ontology.suggestion.accept_stand',
--     'ontology.suggestion.accept_gate', 'ontology.suggestion.reject', 'ontology.plan.confirm');
-- DROP TABLE IF EXISTS resource_adjustment_suggestions;
-- DROP TABLE IF EXISTS turnaround_links;
-- DROP TABLE IF EXISTS gate_assignments;
-- DROP TABLE IF EXISTS stand_occupations;
-- DROP TABLE IF EXISTS aircraft;
-- ALTER TABLE flights
--     DROP COLUMN IF EXISTS divert,
--     DROP COLUMN IF EXISTS is_draft,
--     DROP COLUMN IF EXISTS flight_kind,
--     DROP COLUMN IF EXISTS direction;
