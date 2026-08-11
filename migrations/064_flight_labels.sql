-- =====================================================
-- 064: 航班标签系统
--
-- 1. 创建 label_definitions 标签字典表
-- 2. flights / flight_legs 各加 labels JSONB 列 + GIN 索引
-- 3. 预置系统标签
-- 4. 将现有布尔数据同步到 JSONB
-- =====================================================

-- 标签字典表
CREATE TABLE IF NOT EXISTS label_definitions (
    label_id    VARCHAR(26) PRIMARY KEY,
    code        VARCHAR(64) NOT NULL UNIQUE,
    name        VARCHAR(100) NOT NULL,
    color       VARCHAR(7) NOT NULL DEFAULT '#6B7280',
    icon        VARCHAR(32),
    scope       VARCHAR(8) NOT NULL DEFAULT 'flight',
    category    VARCHAR(16) NOT NULL DEFAULT 'custom',
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_by  VARCHAR(100),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_label_scope CHECK (scope IN ('flight', 'leg', 'both')),
    CONSTRAINT chk_label_category CHECK (category IN ('system', 'custom'))
);

CREATE INDEX IF NOT EXISTS idx_label_definitions_code ON label_definitions(code);
CREATE INDEX IF NOT EXISTS idx_label_definitions_scope ON label_definitions(scope);

-- 预置系统标签
INSERT INTO label_definitions (label_id, code, name, color, scope, category, sort_order) VALUES
    ('01JRXLBL0001QUICKTURNRND', 'quick_turnaround', '快速过站', '#FF9500', 'flight', 'system', 1),
    ('01JRXLBL0002BOARDRESTRCT', 'boarding_restriction', '登机限制', '#FF3B30', 'flight', 'system', 2),
    ('01JRXLBL0003VIPFLAGLABEL', 'vip', 'VIP', '#AF52DE', 'leg', 'system', 3)
ON CONFLICT (code) DO NOTHING;

-- 航班主表加 labels JSONB 列
ALTER TABLE flights
    ADD COLUMN IF NOT EXISTS labels JSONB NOT NULL DEFAULT '[]'::jsonb;

CREATE INDEX IF NOT EXISTS idx_flights_labels ON flights USING GIN (labels);

-- 航段表加 labels JSONB 列
ALTER TABLE flight_legs
    ADD COLUMN IF NOT EXISTS labels JSONB NOT NULL DEFAULT '[]'::jsonb;

CREATE INDEX IF NOT EXISTS idx_flight_legs_labels ON flight_legs USING GIN (labels);

-- 将现有布尔数据同步到 JSONB 标签
-- 快速过站
UPDATE flights SET labels = labels || '["quick_turnaround"]'::jsonb
WHERE is_quick_turnaround = TRUE
  AND NOT labels @> '["quick_turnaround"]'::jsonb;

-- 登机限制
UPDATE flights SET labels = labels || '["boarding_restriction"]'::jsonb
WHERE has_boarding_restriction = TRUE
  AND NOT labels @> '["boarding_restriction"]'::jsonb;

-- VIP (leg 级)
UPDATE flight_legs SET labels = labels || '["vip"]'::jsonb
WHERE is_vip = TRUE
  AND NOT labels @> '["vip"]'::jsonb;

-- 回滚
-- DROP INDEX IF EXISTS idx_flights_labels;
-- DROP INDEX IF EXISTS idx_flight_legs_labels;
-- ALTER TABLE flights DROP COLUMN IF EXISTS labels;
-- ALTER TABLE flight_legs DROP COLUMN IF EXISTS labels;
-- DROP TABLE IF EXISTS label_definitions;
