-- =====================================================
-- 117: 预排冲突预警提前量配置
--
-- completion_warning_lead_minutes:
--   当前工单仍执行时,提前多久对下一单的共享人员冲突预警。
--   有效范围 0..60 分钟;0 表示下一单到达计划开始时间才触发。
--
-- 优先级:单次工单覆盖值 > 生成规则快照值 > 部门默认值(当前规则) > 系统默认 5。
-- 已发布工单保留生成时的规则快照;规则后续修改不得静默改写已发布订单。
-- 历史订单和手工订单该列为 NULL,回退到部门/系统默认。
-- =====================================================

ALTER TABLE department_flight_generation_rules
    ADD COLUMN IF NOT EXISTS completion_warning_lead_minutes INTEGER;

ALTER TABLE dispatch_orders
    ADD COLUMN IF NOT EXISTS completion_warning_lead_minutes INTEGER;

DO $$
BEGIN
    ALTER TABLE department_flight_generation_rules
        ADD CONSTRAINT chk_generation_rule_completion_warning_lead
        CHECK (completion_warning_lead_minutes IS NULL OR completion_warning_lead_minutes BETWEEN 0 AND 60);
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE dispatch_orders
        ADD CONSTRAINT chk_dispatch_orders_completion_warning_lead
        CHECK (completion_warning_lead_minutes IS NULL OR completion_warning_lead_minutes BETWEEN 0 AND 60);
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

COMMENT ON COLUMN department_flight_generation_rules.completion_warning_lead_minutes IS
    '预排冲突预警提前量(分钟),0..60;NULL 表示回退系统默认 5 分钟';
COMMENT ON COLUMN dispatch_orders.completion_warning_lead_minutes IS
    '生成订单时的预警提前量快照或调度员覆盖值(分钟),0..60;NULL 表示回退部门/系统默认';

-- 回滚：
-- ALTER TABLE dispatch_orders
--     DROP CONSTRAINT IF EXISTS chk_dispatch_orders_completion_warning_lead,
--     DROP COLUMN IF EXISTS completion_warning_lead_minutes;
-- ALTER TABLE department_flight_generation_rules
--     DROP CONSTRAINT IF EXISTS chk_generation_rule_completion_warning_lead,
--     DROP COLUMN IF EXISTS completion_warning_lead_minutes;
