-- no-transaction
-- Speed up load_equipment_by_order_ids: dispatch_order_equipment rows are
-- filtered by released_at IS NULL plus dispatch_order_id IN (...).
-- Note: released_at lives on dispatch_order_equipment, not dispatch_orders.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_dispatch_order_equipment_live_release
    ON dispatch_order_equipment (dispatch_order_id)
    WHERE released_at IS NULL;
