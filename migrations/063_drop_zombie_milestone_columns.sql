-- =====================================================
-- 063: 清除 flights 主表中的僵尸时间列
--
-- 这些列在 032 强解耦迁移后已全部由 dispatch_timeline_events 接管，
-- 代码中不再对其进行任何读写，属于历史遗留的僵尸列。
-- =====================================================

ALTER TABLE flights
    DROP COLUMN IF EXISTS wheel_chocks_time,
    DROP COLUMN IF EXISTS cabin_door_open_time,
    DROP COLUMN IF EXISTS deboarding_complete_time,
    DROP COLUMN IF EXISTS cleaning_start_time,
    DROP COLUMN IF EXISTS cleaning_end_time,
    DROP COLUMN IF EXISTS loading_complete_time,
    DROP COLUMN IF EXISTS cabin_door_close_time,
    DROP COLUMN IF EXISTS cargo_door_close_time;

-- 回滚
-- ALTER TABLE flights
--     ADD COLUMN IF NOT EXISTS wheel_chocks_time TIMESTAMPTZ,
--     ADD COLUMN IF NOT EXISTS cabin_door_open_time TIMESTAMPTZ,
--     ADD COLUMN IF NOT EXISTS deboarding_complete_time TIMESTAMPTZ,
--     ADD COLUMN IF NOT EXISTS cleaning_start_time TIMESTAMPTZ,
--     ADD COLUMN IF NOT EXISTS cleaning_end_time TIMESTAMPTZ,
--     ADD COLUMN IF NOT EXISTS loading_complete_time TIMESTAMPTZ,
--     ADD COLUMN IF NOT EXISTS cabin_door_close_time TIMESTAMPTZ,
--     ADD COLUMN IF NOT EXISTS cargo_door_close_time TIMESTAMPTZ;
