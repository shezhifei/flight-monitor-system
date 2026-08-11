-- 派工系统基础表迁移
-- Migration: 007_dispatch_system_base.sql
-- Description: 创建派工系统所需的基础数据表


-- ============================================
-- 1. 科室表（与现有 users.department 集成）
-- ============================================
CREATE TABLE IF NOT EXISTS departments (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    code VARCHAR(20) UNIQUE,
    description TEXT,
    manager_id VARCHAR(26) REFERENCES users(id),
    terminal VARCHAR(20),  -- 预留：所属航站楼
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_departments_name ON departments(name);
CREATE INDEX IF NOT EXISTS idx_departments_terminal ON departments(terminal);

COMMENT ON TABLE departments IS '科室/部门表';
COMMENT ON COLUMN departments.terminal IS '预留字段：所属航站楼';

-- ============================================
-- 2. 班组类型表
-- ============================================
CREATE TABLE IF NOT EXISTS team_types (
    id VARCHAR(26) PRIMARY KEY,
    department_id VARCHAR(26) REFERENCES departments(id),
    name VARCHAR(100) NOT NULL,
    code VARCHAR(20) UNIQUE,
    description TEXT,
    color VARCHAR(7),  -- UI显示颜色，如 #FF6B6B
    is_driver_type BOOLEAN DEFAULT FALSE,  -- 是否是司机类型班组
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE,
    UNIQUE(department_id, name)
);

CREATE INDEX IF NOT EXISTS idx_team_types_department ON team_types(department_id);

COMMENT ON TABLE team_types IS '班组类型表';
COMMENT ON COLUMN team_types.is_driver_type IS '是否为司机类型班组';

-- ============================================
-- 3. 机位表
-- ============================================
CREATE TABLE IF NOT EXISTS stands (
    id VARCHAR(26) PRIMARY KEY,
    code VARCHAR(20) NOT NULL UNIQUE,  -- 如 T1-A01
    name VARCHAR(100),
    terminal VARCHAR(20),  -- T1, T2
    area VARCHAR(20),  -- A, B, C
    position_lat DECIMAL(10, 7) NOT NULL,
    position_lng DECIMAL(10, 7) NOT NULL,
    stand_type VARCHAR(20),  -- contact, remote
    size_category VARCHAR(10),  -- A, B, C, D, E, F (ICAO)
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_stands_code ON stands(code);
CREATE INDEX IF NOT EXISTS idx_stands_terminal ON stands(terminal);

COMMENT ON TABLE stands IS '机位/停机位表';

-- ============================================
-- 4. 班组表
-- ============================================
CREATE TABLE IF NOT EXISTS teams (
    id VARCHAR(26) PRIMARY KEY,
    team_type_id VARCHAR(26) REFERENCES team_types(id),
    name VARCHAR(100) NOT NULL,
    code VARCHAR(20) UNIQUE,
    leader_id VARCHAR(26) REFERENCES users(id),
    terminal VARCHAR(20),  -- 预留：所属航站楼
    current_status VARCHAR(20) DEFAULT 'off_duty',  -- on_duty, off_duty, break
    current_position_lat DECIMAL(10, 7),
    current_position_lng DECIMAL(10, 7),
    current_stand_id VARCHAR(26) REFERENCES stands(id),
    last_position_update TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_teams_type ON teams(team_type_id);
CREATE INDEX IF NOT EXISTS idx_teams_status ON teams(current_status);
CREATE INDEX IF NOT EXISTS idx_teams_terminal ON teams(terminal);

COMMENT ON TABLE teams IS '班组表';
COMMENT ON COLUMN teams.current_status IS '当前状态：on_duty在岗, off_duty离岗, break休息';

-- ============================================
-- 5. 班组成员表
-- ============================================
CREATE TABLE IF NOT EXISTS team_members (
    id VARCHAR(26) PRIMARY KEY,
    team_id VARCHAR(26) REFERENCES teams(id) ON DELETE CASCADE,
    user_id VARCHAR(26) REFERENCES users(id),
    role VARCHAR(20) DEFAULT 'member',  -- leader, member
    can_drive BOOLEAN DEFAULT FALSE,  -- 是否有驾驶资格
    joined_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    left_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT TRUE,
    UNIQUE(team_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_team_members_team ON team_members(team_id);
CREATE INDEX IF NOT EXISTS idx_team_members_user ON team_members(user_id);

COMMENT ON TABLE team_members IS '班组成员表';

-- ============================================
-- 6. 班组类型-作业类型能力表
-- ============================================
CREATE TABLE IF NOT EXISTS team_type_steps (
    team_type_id VARCHAR(26) REFERENCES team_types(id) ON DELETE CASCADE,
    task_type VARCHAR(50) NOT NULL,
    priority INT DEFAULT 0,  -- 优先级，用于多类型可保障时的选择
    PRIMARY KEY (team_type_id, task_type)
);

COMMENT ON TABLE team_type_steps IS '班组类型可保障的作业类型';

-- ============================================
-- 7. 设备类型表
-- ============================================
CREATE TABLE IF NOT EXISTS equipment_types (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    code VARCHAR(20) UNIQUE,
    category VARCHAR(50),  -- vehicle, loader, support
    requires_driver BOOLEAN DEFAULT FALSE,
    driver_team_type_id VARCHAR(26) REFERENCES team_types(id),  -- 需要的司机班组类型
    icon VARCHAR(100),
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_equipment_types_category ON equipment_types(category);

COMMENT ON TABLE equipment_types IS '设备类型表';

-- ============================================
-- 8. 设备类型-作业类型需求表
-- ============================================
CREATE TABLE IF NOT EXISTS equipment_type_steps (
    equipment_type_id VARCHAR(26) REFERENCES equipment_types(id) ON DELETE CASCADE,
    task_type VARCHAR(50) NOT NULL,
    min_count INT DEFAULT 1,
    max_count INT DEFAULT 1,
    is_required BOOLEAN DEFAULT TRUE,  -- 必须还是可选
    PRIMARY KEY (equipment_type_id, task_type)
);

COMMENT ON TABLE equipment_type_steps IS '设备类型与作业类型的关联';

-- ============================================
-- 9. 设备表
-- ============================================
CREATE TABLE IF NOT EXISTS equipment (
    id VARCHAR(26) PRIMARY KEY,
    equipment_type_id VARCHAR(26) REFERENCES equipment_types(id),
    code VARCHAR(50) NOT NULL UNIQUE,  -- 设备编号
    name VARCHAR(100),
    license_plate VARCHAR(20),  -- 车牌号
    terminal VARCHAR(20),  -- 预留：所属航站楼
    status VARCHAR(20) DEFAULT 'available',  -- available, in_use, maintenance, retired
    current_position_lat DECIMAL(10, 7),
    current_position_lng DECIMAL(10, 7),
    current_stand_id VARCHAR(26) REFERENCES stands(id),
    last_position_update TIMESTAMP WITH TIME ZONE,
    current_dispatch_id VARCHAR(26),  -- 当前派工单ID
    last_maintenance_date DATE,
    next_maintenance_date DATE,
    metadata JSONB,  -- 扩展属性
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_equipment_type ON equipment(equipment_type_id);
CREATE INDEX IF NOT EXISTS idx_equipment_status ON equipment(status);
CREATE INDEX IF NOT EXISTS idx_equipment_terminal ON equipment(terminal);

COMMENT ON TABLE equipment IS '设备表';

-- ============================================
-- 10. 作业类型定义表
-- ============================================
CREATE TABLE IF NOT EXISTS task_types (
    id VARCHAR(26) PRIMARY KEY,
    code VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    category VARCHAR(50),  -- arrival, departure, turnaround
    sequence_order INT,  -- 作业类型顺序
    default_duration_minutes INT,  -- 默认持续时间
    trigger_offset_minutes INT DEFAULT 30,  -- 派单提前量
    trigger_type VARCHAR(20) DEFAULT 'before_eta',  -- before_eta, after_arrival, before_etd
    description TEXT,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE task_types IS '作业类型定义表';

-- 预置作业类型
INSERT INTO task_types (id, code, name, category, sequence_order, trigger_type, trigger_offset_minutes, default_duration_minutes) VALUES
    ('step_001', 'wheel_chocks_on', '上轮挡', 'arrival', 1, 'after_arrival', 0, 2),
    ('step_002', 'cabin_door_open', '开客舱门', 'arrival', 2, 'after_arrival', 2, 3),
    ('step_003', 'deboarding', '旅客下机', 'arrival', 3, 'after_arrival', 5, 15),
    ('step_004', 'cleaning', '客舱清洁', 'turnaround', 4, 'after_arrival', 20, 25),
    ('step_005', 'catering', '配餐', 'turnaround', 5, 'before_etd', 60, 20),
    ('step_006', 'boarding', '旅客登机', 'departure', 6, 'before_etd', 40, 25),
    ('step_007', 'cargo_loading', '行李装载', 'departure', 7, 'before_etd', 30, 20),
    ('step_008', 'cabin_door_close', '关客舱门', 'departure', 8, 'before_etd', 10, 3),
    ('step_009', 'cargo_door_close', '关货舱门', 'departure', 9, 'before_etd', 8, 2),
    ('step_010', 'pushback', '推出/牵引', 'departure', 10, 'before_etd', 5, 5),
    ('step_011', 'wheel_chocks_off', '撤轮挡', 'departure', 11, 'before_etd', 3, 2)
ON CONFLICT (id) DO NOTHING;

-- ============================================
-- 11. 派工单表
-- ============================================
CREATE TABLE IF NOT EXISTS dispatch_orders (
    id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,  -- 关联航班
    task_type VARCHAR(50) NOT NULL,
    stand_id VARCHAR(26) REFERENCES stands(id),
    
    -- 分配单位（二选一）
    assignee_type VARCHAR(20) NOT NULL,  -- 'team' 或 'individual'
    team_id VARCHAR(26) REFERENCES teams(id),
    individual_user_id VARCHAR(26) REFERENCES users(id),
    
    -- 司机资源
    driver_type VARCHAR(20),  -- 'team', 'individual', 或 NULL
    driver_team_id VARCHAR(26) REFERENCES teams(id),
    driver_user_id VARCHAR(26) REFERENCES users(id),
    
    -- 时间节点
    planned_start_time TIMESTAMP WITH TIME ZONE,
    planned_end_time TIMESTAMP WITH TIME ZONE,
    actual_start_time TIMESTAMP WITH TIME ZONE,
    actual_end_time TIMESTAMP WITH TIME ZONE,
    
    -- 状态
    status VARCHAR(20) DEFAULT 'pending',  -- pending, assigned, in_progress, completed, cancelled
    dispatch_type VARCHAR(20) DEFAULT 'auto',  -- auto, manual
    dispatched_at TIMESTAMP WITH TIME ZONE,
    dispatched_by VARCHAR(26) REFERENCES users(id),
    
    -- 快照
    snapshot_assignee_position JSONB,
    snapshot_equipment_positions JSONB,
    estimated_arrival_minutes INT,
    
    -- 完成信息
    completed_by VARCHAR(26) REFERENCES users(id),
    completion_notes TEXT,
    
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(flight_id, task_type),
    CHECK (
        ((assignee_type = 'team' AND team_id IS NOT NULL AND individual_user_id IS NULL) OR
         (assignee_type = 'individual' AND individual_user_id IS NOT NULL AND team_id IS NULL))
        AND
        ((driver_type IS NULL AND driver_team_id IS NULL AND driver_user_id IS NULL) OR
         (driver_type = 'team' AND driver_team_id IS NOT NULL AND driver_user_id IS NULL) OR
         (driver_type = 'individual' AND driver_user_id IS NOT NULL AND driver_team_id IS NULL))
    )
);

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_flight ON dispatch_orders(flight_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_status ON dispatch_orders(status);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_team ON dispatch_orders(team_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_planned_time ON dispatch_orders(planned_start_time);

COMMENT ON TABLE dispatch_orders IS '派工单表';

-- ============================================
-- 12. 派工单人员明细表
-- ============================================
CREATE TABLE IF NOT EXISTS dispatch_order_members (
    id VARCHAR(26) PRIMARY KEY,
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    user_id VARCHAR(26) REFERENCES users(id),
    role VARCHAR(20) DEFAULT 'member',  -- leader, member, driver
    source_type VARCHAR(20) NOT NULL,  -- 'team' 或 'individual'
    source_team_id VARCHAR(26) REFERENCES teams(id),
    assigned_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    check_in_time TIMESTAMP WITH TIME ZONE,
    check_out_time TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT TRUE,
    UNIQUE(dispatch_order_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_order_members_order ON dispatch_order_members(dispatch_order_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_order_members_user ON dispatch_order_members(user_id);

COMMENT ON TABLE dispatch_order_members IS '派工单人员明细（具体参与人员）';

-- ============================================
-- 13. 派工单设备关联表
-- ============================================
CREATE TABLE IF NOT EXISTS dispatch_order_equipment (
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    equipment_id VARCHAR(26) REFERENCES equipment(id),
    assigned_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    released_at TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY (dispatch_order_id, equipment_id)
);

COMMENT ON TABLE dispatch_order_equipment IS '派工单与设备的关联';

-- ============================================
-- 14. 派工单操作日志
-- ============================================
CREATE TABLE IF NOT EXISTS dispatch_order_logs (
    id VARCHAR(26) PRIMARY KEY,
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    action VARCHAR(50) NOT NULL,
    actor_id VARCHAR(26) REFERENCES users(id),
    details JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_dispatch_order_logs_order ON dispatch_order_logs(dispatch_order_id);

COMMENT ON TABLE dispatch_order_logs IS '派工单操作日志';

-- ============================================
-- 15. 派工告警表
-- ============================================
CREATE TABLE IF NOT EXISTS dispatch_alerts (
    id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26),
    task_type VARCHAR(50),
    alert_type VARCHAR(50) NOT NULL,
    severity VARCHAR(20) DEFAULT 'warning',
    message TEXT NOT NULL,
    is_resolved BOOLEAN DEFAULT FALSE,
    resolved_at TIMESTAMP WITH TIME ZONE,
    resolved_by VARCHAR(26) REFERENCES users(id),
    resolution_notes TEXT,
    notify_users VARCHAR(26)[],
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_dispatch_alerts_resolved ON dispatch_alerts(is_resolved);
CREATE INDEX IF NOT EXISTS idx_dispatch_alerts_flight ON dispatch_alerts(flight_id);

COMMENT ON TABLE dispatch_alerts IS '派工告警表';

-- ============================================
-- 16. 新增派工相关权限
-- ============================================
INSERT INTO permissions (id, name, description, created_at) VALUES
    ('perm_dispatch_view', 'dispatch:view', '查看派工单', CURRENT_TIMESTAMP),
    ('perm_dispatch_manage', 'dispatch:manage', '管理派工', CURRENT_TIMESTAMP),
    ('perm_team_view', 'team:view', '查看班组', CURRENT_TIMESTAMP),
    ('perm_team_manage', 'team:manage', '管理班组', CURRENT_TIMESTAMP),
    ('perm_equipment_view', 'equipment:view', '查看设备', CURRENT_TIMESTAMP),
    ('perm_equipment_manage', 'equipment:manage', '管理设备', CURRENT_TIMESTAMP),
    ('perm_schedule_view', 'schedule:view', '查看排班', CURRENT_TIMESTAMP),
    ('perm_schedule_manage', 'schedule:manage', '管理排班', CURRENT_TIMESTAMP)
ON CONFLICT (name) DO NOTHING;

-- 为 admin 角色添加新权限
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'admin' 
AND p.name IN ('dispatch:view', 'dispatch:manage', 'team:view', 'team:manage', 
               'equipment:view', 'equipment:manage', 'schedule:view', 'schedule:manage')
ON CONFLICT DO NOTHING;

-- ============================================
-- 17. users 表增加 department_id 外键
-- ============================================
ALTER TABLE users ADD COLUMN IF NOT EXISTS department_id VARCHAR(26) REFERENCES departments(id);
CREATE INDEX IF NOT EXISTS idx_users_department_id ON users(department_id);

-- ============================================
-- 18. 为 equipment 表的 current_dispatch_id 增加外键约束
-- ============================================
ALTER TABLE equipment DROP CONSTRAINT IF EXISTS fk_equipment_current_dispatch;
ALTER TABLE equipment ADD CONSTRAINT fk_equipment_current_dispatch 
    FOREIGN KEY (current_dispatch_id) REFERENCES dispatch_orders(id) ON DELETE SET NULL;

