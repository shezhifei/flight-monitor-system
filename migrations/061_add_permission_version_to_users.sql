-- 迁移：添加 permission_version 到 users 表
-- 日期：2026-04-06
-- 说明：用于 JWT 权限版本控制，当用户权限变更时使旧令牌失效

ALTER TABLE users ADD COLUMN IF NOT EXISTS permission_version INTEGER NOT NULL DEFAULT 1;

-- 为已有用户初始化 permission_version（如果 Redis 中已有记录，后续会通过同步机制更新）
-- 此处保持默认值 1 即可
