-- =====================================================
-- Migration: Add Extended Fields to Flights Table
-- Version: 002
-- Description: Add 8 time fields and 3 remark fields to flights table
-- =====================================================

-- 检查并添加时间字段
DO $$
BEGIN
    -- 上轮挡时间
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'wheel_chocks_time') THEN
        ALTER TABLE flights ADD COLUMN wheel_chocks_time TIMESTAMP WITH TIME ZONE;
        COMMENT ON COLUMN flights.wheel_chocks_time IS '上轮挡时间';
    END IF;

    -- 开舱门时间
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'cabin_door_open_time') THEN
        ALTER TABLE flights ADD COLUMN cabin_door_open_time TIMESTAMP WITH TIME ZONE;
        COMMENT ON COLUMN flights.cabin_door_open_time IS '开舱门时间';
    END IF;

    -- 下客完成时间
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'deboarding_complete_time') THEN
        ALTER TABLE flights ADD COLUMN deboarding_complete_time TIMESTAMP WITH TIME ZONE;
        COMMENT ON COLUMN flights.deboarding_complete_time IS '下客完成时间';
    END IF;

    -- 清洁开始时间
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'cleaning_start_time') THEN
        ALTER TABLE flights ADD COLUMN cleaning_start_time TIMESTAMP WITH TIME ZONE;
        COMMENT ON COLUMN flights.cleaning_start_time IS '清洁开始时间';
    END IF;

    -- 清洁结束时间
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'cleaning_end_time') THEN
        ALTER TABLE flights ADD COLUMN cleaning_end_time TIMESTAMP WITH TIME ZONE;
        COMMENT ON COLUMN flights.cleaning_end_time IS '清洁结束时间';
    END IF;

    -- 关客舱门时间
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'cabin_door_close_time') THEN
        ALTER TABLE flights ADD COLUMN cabin_door_close_time TIMESTAMP WITH TIME ZONE;
        COMMENT ON COLUMN flights.cabin_door_close_time IS '关客舱门时间';
    END IF;

    -- 关货舱门时间
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'cargo_door_close_time') THEN
        ALTER TABLE flights ADD COLUMN cargo_door_close_time TIMESTAMP WITH TIME ZONE;
        COMMENT ON COLUMN flights.cargo_door_close_time IS '关货舱门时间';
    END IF;

    -- 装载完成时间
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'loading_complete_time') THEN
        ALTER TABLE flights ADD COLUMN loading_complete_time TIMESTAMP WITH TIME ZONE;
        COMMENT ON COLUMN flights.loading_complete_time IS '装载完成时间';
    END IF;

    -- 航班备注
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'flight_remarks') THEN
        ALTER TABLE flights ADD COLUMN flight_remarks TEXT;
        COMMENT ON COLUMN flights.flight_remarks IS '航班备注';
    END IF;

    -- 配载备注
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'loading_remarks') THEN
        ALTER TABLE flights ADD COLUMN loading_remarks TEXT;
        COMMENT ON COLUMN flights.loading_remarks IS '配载备注';
    END IF;

    -- 机务备注
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'maintenance_remarks') THEN
        ALTER TABLE flights ADD COLUMN maintenance_remarks TEXT;
        COMMENT ON COLUMN flights.maintenance_remarks IS '机务备注';
    END IF;

    -- 进出港异常标记（被 011 物化视图引用，必须在早期存在）
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'inbound_abnormal') THEN
        ALTER TABLE flights ADD COLUMN inbound_abnormal BOOLEAN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'outbound_abnormal') THEN
        ALTER TABLE flights ADD COLUMN outbound_abnormal BOOLEAN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'inbound_abnormal_reason') THEN
        ALTER TABLE flights ADD COLUMN inbound_abnormal_reason TEXT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'flights' AND column_name = 'outbound_abnormal_reason') THEN
        ALTER TABLE flights ADD COLUMN outbound_abnormal_reason TEXT;
    END IF;

    RAISE NOTICE '航班表扩展字段添加完成';
END $$;

-- 标记迁移已完成

