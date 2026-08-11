-- =====================================================
-- 114: 部门作业生成规则新增"人数 -> 作业时长"映射
--
-- 重排(replan)求解时,每张派工单的作业时长此前是常量:
-- 取 planned_end_time - planned_start_time,3 个人干和 1 个人干占用同样时长。
-- 后果是模型没有任何理由多派人 ——"人数不足则工期拉长"这条最基本的业务事实
-- 不在模型里,槽位填不满时下游占用与冲突也就无从体现。
--
-- 时长改由各部门按作业类型自行配置。本表已是"部门 x 作业类型 x 航段"的
-- 版本化配置表(generation_anchor_type / start_offset_minutes /
-- duration_minutes / start_flex_minutes 均在此),该映射与它们同属一类参数,
-- 随规则版本一起版本化。
--
-- 用 JSONB 而不是新开子表:这是一个小的、整体读写的映射,开子表会引出
-- 独立的版本与生命周期,代价不划算。
--
-- 形如 {"1":45,"2":30,"3":25} —— 键为人数(正整数),值为分钟数(正整数)。
-- 允许为空:NULL 表示该部门尚未配置,读取端回退到 duration_minutes 常量
-- (与今天行为完全一致)。不设 DEFAULT,避免给既有规则行静默赋予一个
-- 业务上无意义的表。
-- =====================================================

ALTER TABLE department_flight_generation_rules
    ADD COLUMN IF NOT EXISTS duration_by_crew_size JSONB;

-- 只挡住类型层面的错误(必须是 JSON 对象)。键为正整数、值为正整数这类
-- 逐条校验留给读取端:非法条目 warn! 后忽略该键,而不是让整张规则存不进去。
DO $$
BEGIN
    ALTER TABLE department_flight_generation_rules
        ADD CONSTRAINT chk_generation_rule_duration_by_crew_size_object
        CHECK (duration_by_crew_size IS NULL OR jsonb_typeof(duration_by_crew_size) = 'object');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

COMMENT ON COLUMN department_flight_generation_rules.duration_by_crew_size IS
    '人数->作业时长(分钟)映射,如 {"1":45,"2":30,"3":25};NULL 表示未配置,回退 duration_minutes 常量';

-- 回滚
-- ALTER TABLE department_flight_generation_rules
--     DROP CONSTRAINT IF EXISTS chk_generation_rule_duration_by_crew_size_object;
-- ALTER TABLE department_flight_generation_rules
--     DROP COLUMN IF EXISTS duration_by_crew_size;
