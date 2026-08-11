-- 派工位置间移动时间统计表
-- Migration: 047_add_dispatch_travel_stats.sql
-- Description: 记录机位对之间的历史移动时间统计，供前端 WASM OR-Tools replan 使用


CREATE TABLE IF NOT EXISTS dispatch_stand_travel_stats (
    from_stand_id VARCHAR(26) NOT NULL REFERENCES stands(id),
    to_stand_id   VARCHAR(26) NOT NULL REFERENCES stands(id),
    sample_count  INT NOT NULL DEFAULT 0,
    total_minutes DOUBLE PRECISION NOT NULL DEFAULT 0,
    avg_minutes   DOUBLE PRECISION NOT NULL DEFAULT 0,
    min_minutes   DOUBLE PRECISION,
    max_minutes   DOUBLE PRECISION,
    last_updated  TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (from_stand_id, to_stand_id)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_stand_travel_stats_from ON dispatch_stand_travel_stats(from_stand_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_stand_travel_stats_to ON dispatch_stand_travel_stats(to_stand_id);

COMMENT ON TABLE dispatch_stand_travel_stats IS '机位对间移动时间统计（基于签退→签到时间差积累）';
COMMENT ON COLUMN dispatch_stand_travel_stats.sample_count IS '采样次数';
COMMENT ON COLUMN dispatch_stand_travel_stats.avg_minutes IS '平均移动时间（分钟）';

