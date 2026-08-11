-- =====================================================
-- 118: 派工告警幂等化与预排冲突预警字段
--
-- dedupe_key: `dispatch_schedule_overrun:{current_order_id}:{next_order_id}`
-- 唯一索引保证一次持续冲突只保留一条可更新告警;
-- 冲突关闭后再次出现时复用键但递增 occurrence_count,并清空确认状态。
-- acknowledge(已确认)与 resolve(已关闭)分离。
-- =====================================================

ALTER TABLE dispatch_alerts
    ADD COLUMN IF NOT EXISTS dedupe_key VARCHAR(128),
    ADD COLUMN IF NOT EXISTS current_order_id VARCHAR(26),
    ADD COLUMN IF NOT EXISTS next_order_id VARCHAR(26),
    ADD COLUMN IF NOT EXISTS last_detected_at TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS occurrence_count INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS acknowledged_at TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS acknowledged_by VARCHAR(26) REFERENCES users(id),
    ADD COLUMN IF NOT EXISTS details JSONB;

DO $$
BEGIN
    ALTER TABLE dispatch_alerts
        ADD CONSTRAINT chk_dispatch_alerts_occurrence_count
        CHECK (occurrence_count >= 1);
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_dispatch_alerts_dedupe_key
    ON dispatch_alerts (dedupe_key) WHERE dedupe_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_dispatch_alerts_current_next_order
    ON dispatch_alerts (current_order_id, next_order_id);

CREATE INDEX IF NOT EXISTS idx_dispatch_alerts_unresolved_created
    ON dispatch_alerts (is_resolved, created_at DESC);

COMMENT ON COLUMN dispatch_alerts.dedupe_key IS
    '幂等键,如 dispatch_schedule_overrun:{current_order_id}:{next_order_id};NULL 表示非预排冲突类告警';
COMMENT ON COLUMN dispatch_alerts.current_order_id IS '预排冲突中仍未完成的当前工单';
COMMENT ON COLUMN dispatch_alerts.next_order_id IS '即将开始的下一工单';
COMMENT ON COLUMN dispatch_alerts.last_detected_at IS '最近一次检测到冲突的时间';
COMMENT ON COLUMN dispatch_alerts.occurrence_count IS '同一冲突关闭后再次出现的次数';
COMMENT ON COLUMN dispatch_alerts.acknowledged_at IS '调度员确认时间;确认不等于关闭';
COMMENT ON COLUMN dispatch_alerts.acknowledged_by IS '确认人';
COMMENT ON COLUMN dispatch_alerts.details IS '告警结构化详情(共享人员/倒计时/预计冲突分钟/ETA 状态等)';

-- 回滚：
-- DROP INDEX IF EXISTS idx_dispatch_alerts_unresolved_created;
-- DROP INDEX IF EXISTS idx_dispatch_alerts_current_next_order;
-- DROP INDEX IF EXISTS idx_dispatch_alerts_dedupe_key;
-- ALTER TABLE dispatch_alerts
--     DROP CONSTRAINT IF EXISTS chk_dispatch_alerts_occurrence_count,
--     DROP COLUMN IF EXISTS details,
--     DROP COLUMN IF EXISTS acknowledged_by,
--     DROP COLUMN IF EXISTS acknowledged_at,
--     DROP COLUMN IF EXISTS occurrence_count,
--     DROP COLUMN IF EXISTS last_detected_at,
--     DROP COLUMN IF EXISTS next_order_id,
--     DROP COLUMN IF EXISTS current_order_id,
--     DROP COLUMN IF EXISTS dedupe_key;
