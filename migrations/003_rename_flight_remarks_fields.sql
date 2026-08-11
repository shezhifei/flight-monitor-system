-- =====================================================
-- Migration: Rename Flight Remarks Fields
-- Version: 003
-- Description: Rename loading_remarks and maintenance_remarks to more accurate names
-- =====================================================

-- 检查并重命名字段
DO $$
BEGIN
    -- 重命名配载备注字段
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'loading_remarks') THEN
        IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_name = 'flights'
              AND column_name = 'load_planning_remarks'
        ) THEN
            ALTER TABLE flights DROP COLUMN loading_remarks;
        ELSE
            ALTER TABLE flights RENAME COLUMN loading_remarks TO load_planning_remarks;
        END IF;
        COMMENT ON COLUMN flights.load_planning_remarks IS '配载计划备注';
    END IF;

    -- 重命开机务备注字段
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'maintenance_remarks') THEN
        IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_name = 'flights'
              AND column_name = 'aircraft_maintenance_remarks'
        ) THEN
            ALTER TABLE flights DROP COLUMN maintenance_remarks;
        ELSE
            ALTER TABLE flights RENAME COLUMN maintenance_remarks TO aircraft_maintenance_remarks;
        END IF;
        COMMENT ON COLUMN flights.aircraft_maintenance_remarks IS '飞机机务备注';
    END IF;

    RAISE NOTICE '航班备注字段重命名完成';
END $$;

-- 标记迁移已完成

