-- Soft-delete marker required by idx_dispatch_orders_released_status
-- (partial index WHERE deleted_at IS NULL). dispatch_orders was not included
-- in migration 121.

ALTER TABLE dispatch_orders ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
