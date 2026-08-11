-- =====================================================
-- 062: 将 COBT/CODT 回归 flights 主表
--
-- COBT (Calculated Off-Block Time) 和 CODT (Coordinated Departure Time)
-- 属于外部系统同步的参考基准时间，与 ETD/ETA 同级，不属于地服操作打卡节点。
-- 此迁移将它们从 dispatch_timeline_events 的隐式 milestone 模式
-- 正式提升为 flights 表的物理列。
-- =====================================================

ALTER TABLE flights
    ADD COLUMN IF NOT EXISTS cobt_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS codt TIMESTAMPTZ;

COMMENT ON COLUMN flights.cobt_time IS '计算撤轮挡时间 (COBT)，由外部 CDM/A-CDM 系统同步，禁止人工修改';
COMMENT ON COLUMN flights.codt IS '协同起飞时间 (CODT)，由外部协同放行系统同步，禁止人工修改';

-- 回滚
-- ALTER TABLE flights
--     DROP COLUMN IF EXISTS cobt_time,
--     DROP COLUMN IF EXISTS codt;
