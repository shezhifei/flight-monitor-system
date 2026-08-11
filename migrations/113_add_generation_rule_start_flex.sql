-- =====================================================
-- 113: 部门作业生成规则新增"开始时间窗弹性"
--
-- 重排(replan)求解时,每张派工单的开始时间允许在 [earliest, latest] 内滑动。
-- 此前 latest 由代码内的全局常量 REPLAN_START_FLEX_MINUTES 统一决定,
-- 导致顶着起飞时刻的作业(如推出)与有过站余量的作业(如客舱清洁)
-- 拿到相同的松弛量,开始时间窗互相重叠,作业开始顺序因此可能颠倒。
--
-- 弹性改由各部门按作业类型自行配置。本表已是"部门 x 作业类型 x 航段"的
-- 版本化配置表(generation_anchor_type / start_offset_minutes /
-- duration_minutes 均在此),弹性与它们同属一类参数。
--
-- 允许为空:NULL 表示该部门尚未配置,读取端回退到系统默认值。
-- 不设 DEFAULT,避免给既有规则行静默赋予一个业务上无意义的数字。
-- =====================================================

ALTER TABLE department_flight_generation_rules
    ADD COLUMN IF NOT EXISTS start_flex_minutes INT;

-- 负弹性无意义(会把 latest 推到 earliest 之前),在库层挡住而不只靠服务层过滤。
DO $$
BEGIN
    ALTER TABLE department_flight_generation_rules
        ADD CONSTRAINT chk_generation_rule_start_flex_nonnegative
        CHECK (start_flex_minutes IS NULL OR start_flex_minutes >= 0);
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

COMMENT ON COLUMN department_flight_generation_rules.start_flex_minutes IS
    '重排时该作业开始时间允许后滑的分钟数;NULL 表示未配置,回退系统默认值';

-- 回滚
-- ALTER TABLE department_flight_generation_rules
--     DROP CONSTRAINT IF EXISTS chk_generation_rule_start_flex_nonnegative;
-- ALTER TABLE department_flight_generation_rules
--     DROP COLUMN IF EXISTS start_flex_minutes;
