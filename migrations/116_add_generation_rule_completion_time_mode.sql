-- =====================================================
-- 116: 预计完成时间双模式与生成订单计算快照
--
-- start_plus_duration:
--   planned_end_time = planned_start_time + duration
-- completion_anchor_offset:
--   planned_end_time = completion_anchor_time + completion_offset_minutes
--
-- 旧规则保持原行为；历史和手工订单不伪造计算依据。
-- =====================================================

ALTER TABLE department_flight_generation_rules
    ADD COLUMN IF NOT EXISTS completion_time_mode VARCHAR(32)
        NOT NULL DEFAULT 'start_plus_duration',
    ADD COLUMN IF NOT EXISTS completion_anchor_type VARCHAR(64),
    ADD COLUMN IF NOT EXISTS completion_offset_minutes INTEGER;

DO $$
BEGIN
    ALTER TABLE department_flight_generation_rules
        ADD CONSTRAINT chk_generation_rule_completion_time_mode
        CHECK (completion_time_mode IN ('start_plus_duration', 'completion_anchor_offset'));
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE department_flight_generation_rules
        ADD CONSTRAINT chk_generation_rule_completion_anchor_type
        CHECK (
            completion_anchor_type IS NULL OR completion_anchor_type IN (
                'scheduled_time',
                'actual_arrival',
                'estimated_arrival',
                'scheduled_arrival',
                'actual_departure',
                'estimated_departure',
                'scheduled_departure'
            )
        );
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE department_flight_generation_rules
        ADD CONSTRAINT chk_generation_rule_completion_configuration
        CHECK (
            (
                completion_time_mode = 'start_plus_duration'
                AND completion_anchor_type IS NULL
                AND completion_offset_minutes IS NULL
            ) OR (
                completion_time_mode = 'completion_anchor_offset'
                AND completion_anchor_type IS NOT NULL
                AND completion_offset_minutes IS NOT NULL
                AND duration_minutes IS NULL
                AND duration_by_crew_size IS NULL
            )
        );
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

COMMENT ON COLUMN department_flight_generation_rules.completion_time_mode IS
    '预计完成时间模式：start_plus_duration 或 completion_anchor_offset';
COMMENT ON COLUMN department_flight_generation_rules.completion_anchor_type IS
    '完成锚点模式使用的时间锚点；开始加时长模式必须为空';
COMMENT ON COLUMN department_flight_generation_rules.completion_offset_minutes IS
    '完成锚点偏移分钟数，可为负数；开始加时长模式必须为空';

ALTER TABLE dispatch_orders
    ADD COLUMN IF NOT EXISTS completion_time_mode VARCHAR(32),
    ADD COLUMN IF NOT EXISTS completion_anchor_type VARCHAR(64),
    ADD COLUMN IF NOT EXISTS completion_anchor_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS completion_offset_minutes INTEGER;

COMMENT ON COLUMN dispatch_orders.completion_time_mode IS '生成订单时的预计完成时间模式快照';
COMMENT ON COLUMN dispatch_orders.completion_anchor_type IS '生成订单时的完成锚点类型快照';
COMMENT ON COLUMN dispatch_orders.completion_anchor_time IS '生成订单时解析得到的完成锚点时间快照';
COMMENT ON COLUMN dispatch_orders.completion_offset_minutes IS '生成订单时的完成锚点偏移分钟数快照';

-- 回滚：
-- ALTER TABLE dispatch_orders DROP COLUMN IF EXISTS completion_offset_minutes,
--     DROP COLUMN IF EXISTS completion_anchor_time,
--     DROP COLUMN IF EXISTS completion_anchor_type,
--     DROP COLUMN IF EXISTS completion_time_mode;
-- ALTER TABLE department_flight_generation_rules
--     DROP CONSTRAINT IF EXISTS chk_generation_rule_completion_configuration,
--     DROP CONSTRAINT IF EXISTS chk_generation_rule_completion_anchor_type,
--     DROP CONSTRAINT IF EXISTS chk_generation_rule_completion_time_mode,
--     DROP COLUMN IF EXISTS completion_offset_minutes,
--     DROP COLUMN IF EXISTS completion_anchor_type,
--     DROP COLUMN IF EXISTS completion_time_mode;
