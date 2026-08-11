ALTER TABLE dispatch_orders
    ADD COLUMN IF NOT EXISTS estimated_completion_time TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS estimated_completion_reported_by VARCHAR(26),
    ADD COLUMN IF NOT EXISTS estimated_completion_reported_at TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS estimated_completion_note TEXT;

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_estimated_completion_time
ON dispatch_orders(estimated_completion_time);

COMMENT ON COLUMN dispatch_orders.estimated_completion_time IS '一线回报的预计完成时间';
COMMENT ON COLUMN dispatch_orders.estimated_completion_reported_by IS '预计完成时间回报人';
COMMENT ON COLUMN dispatch_orders.estimated_completion_reported_at IS '预计完成时间回报时间';
COMMENT ON COLUMN dispatch_orders.estimated_completion_note IS '预计完成时间回报备注';
