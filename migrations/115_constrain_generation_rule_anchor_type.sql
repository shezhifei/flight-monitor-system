-- =====================================================
-- 115: 限制部门作业生成规则的时间锚点类型
--
-- 历史前端曾提交 estimated_time / event 等运行时并不识别的值，旧生成器
-- 会把所有未知值静默回退到 scheduled_time。应用层现已改为明确拒绝；
-- 本约束补齐直接写库的防线。
--
-- 使用 NOT VALID，避免历史脏行阻断部署。PostgreSQL 仍会校验所有新增或
-- 更新的行；历史非法规则由运行时拒绝，清理后再执行 VALIDATE CONSTRAINT。
-- =====================================================

DO $$
BEGIN
    ALTER TABLE department_flight_generation_rules
        ADD CONSTRAINT chk_generation_rule_anchor_type
        CHECK (generation_anchor_type IN (
            'scheduled_time',
            'actual_arrival',
            'estimated_arrival',
            'scheduled_arrival',
            'actual_departure',
            'estimated_departure',
            'scheduled_departure'
        )) NOT VALID;
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

COMMENT ON COLUMN department_flight_generation_rules.generation_anchor_type IS
    '生成时间锚点；仅允许 scheduled_time 或 actual/estimated/scheduled arrival/departure';

-- 历史数据清理后执行：
-- ALTER TABLE department_flight_generation_rules
--     VALIDATE CONSTRAINT chk_generation_rule_anchor_type;

-- 回滚：
-- ALTER TABLE department_flight_generation_rules
--     DROP CONSTRAINT IF EXISTS chk_generation_rule_anchor_type;
