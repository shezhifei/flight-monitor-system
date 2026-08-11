-- Migration: 006_extend_user_permissions
-- Description: 扩展用户表和权限表，支持字段级编辑权限和组织架构
-- Created: 2026-01-29

-- =====================================================
-- 1. 新增扩展时间字段编辑权限
-- =====================================================
INSERT INTO permissions (id, name, description) VALUES
    ('01H00000000000000000000020', 'flight:edit:wheel_chocks', '编辑上轮挡时间'),
    ('01H00000000000000000000021', 'flight:edit:cabin_door_open', '编辑开舱门时间'),
    ('01H00000000000000000000022', 'flight:edit:deboarding', '编辑下客完成时间'),
    ('01H00000000000000000000023', 'flight:edit:cleaning_start', '编辑清洁开始时间'),
    ('01H00000000000000000000024', 'flight:edit:cleaning_end', '编辑清洁结束时间'),
    ('01H00000000000000000000025', 'flight:edit:cabin_door_close', '编辑关客舱门时间'),
    ('01H00000000000000000000026', 'flight:edit:cargo_door_close', '编辑关货舱门时间'),
    ('01H00000000000000000000027', 'flight:edit:loading_complete', '编辑装载完成时间'),
    ('01H00000000000000000000028', 'flight:edit:off_blocks', '编辑撤轮挡时间'),
    ('01H00000000000000000000029', 'flight:edit:passengers_ready', '编辑人齐时间'),
    ('01H00000000000000000000030', 'flight:edit:boarding_permission', '编辑允许登机时间')
ON CONFLICT (name) DO NOTHING;

-- =====================================================
-- 2. 扩展用户表 - 组织架构字段
-- =====================================================
ALTER TABLE users ADD COLUMN IF NOT EXISTS department VARCHAR(100);
ALTER TABLE users ADD COLUMN IF NOT EXISTS job_level SMALLINT DEFAULT 1;
ALTER TABLE users ADD COLUMN IF NOT EXISTS job_title VARCHAR(100);

COMMENT ON COLUMN users.department IS '所属科室/部门';
COMMENT ON COLUMN users.job_level IS '职级(1=一线员工, 2=班组长, 3=主管, 4=经理, 5=总监)';
COMMENT ON COLUMN users.job_title IS '职位名称';

-- 创建索引以支持按科室查询
CREATE INDEX IF NOT EXISTS idx_users_department ON users(department);
CREATE INDEX IF NOT EXISTS idx_users_job_level ON users(job_level);

-- =====================================================
-- 3. 为 admin 角色自动分配新权限
-- =====================================================
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p 
WHERE r.name = 'admin' AND p.name LIKE 'flight:edit:%'
ON CONFLICT DO NOTHING;

-- =====================================================
-- 4. 记录迁移版本
-- =====================================================

