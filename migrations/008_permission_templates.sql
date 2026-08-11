-- 权限模板表迁移
-- Migration: 008_permission_templates.sql
-- Description: 创建权限模板表，支持预设权限组合快速应用到角色


-- ============================================
-- 1. 权限模板表
-- ============================================
CREATE TABLE IF NOT EXISTS permission_templates (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    code VARCHAR(50) UNIQUE,
    description TEXT,
    permissions TEXT[] NOT NULL DEFAULT '{}',
    is_system BOOLEAN DEFAULT FALSE,
    category VARCHAR(50),
    display_order INT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_permission_templates_category ON permission_templates(category);
CREATE INDEX IF NOT EXISTS idx_permission_templates_is_system ON permission_templates(is_system);
CREATE INDEX IF NOT EXISTS idx_permission_templates_display_order ON permission_templates(display_order);

COMMENT ON TABLE permission_templates IS '权限模板表';
COMMENT ON COLUMN permission_templates.code IS '模板代码，用于编程访问';
COMMENT ON COLUMN permission_templates.permissions IS '权限名称数组';
COMMENT ON COLUMN permission_templates.is_system IS '系统预设模板不可删除';
COMMENT ON COLUMN permission_templates.category IS '模板分类：dispatch, flight, user, system';

-- ============================================
-- 2. 预设模板数据
-- ============================================
INSERT INTO permission_templates (id, name, code, description, permissions, is_system, category, display_order) VALUES
-- 派工相关
('tpl_dispatch_viewer', '派工查看员', 'dispatch_viewer', 
 '只能查看派工单、班组和设备信息',
 ARRAY['dispatch:view', 'team:view', 'equipment:view'], TRUE, 'dispatch', 1),

('tpl_dispatch_operator', '派工操作员', 'dispatch_operator',
 '可查看和执行派工操作，但不能管理班组和设备',
 ARRAY['dispatch:view', 'dispatch:manage', 'team:view', 'equipment:view'], TRUE, 'dispatch', 2),

('tpl_dispatch_admin', '派工管理员', 'dispatch_admin',
 '完全的派工系统管理权限',
 ARRAY['dispatch:view', 'dispatch:manage', 'team:view', 'team:manage', 
       'equipment:view', 'equipment:manage', 'schedule:view', 'schedule:manage'], TRUE, 'dispatch', 3),

-- 航班相关
('tpl_flight_viewer', '航班查看员', 'flight_viewer',
 '只能查看航班信息',
 ARRAY['flight:read'], TRUE, 'flight', 1),

('tpl_flight_operator', '航班操作员', 'flight_operator',
 '可查看和编辑航班信息',
 ARRAY['flight:read', 'flight:write'], TRUE, 'flight', 2),

-- 用户管理
('tpl_user_viewer', '用户查看员', 'user_viewer',
 '只能查看用户信息',
 ARRAY['user:read'], TRUE, 'user', 1),

('tpl_user_admin', '用户管理员', 'user_admin',
 '完全的用户管理权限',
 ARRAY['user:read', 'user:write', 'user:manage'], TRUE, 'user', 2)

ON CONFLICT (id) DO NOTHING;

