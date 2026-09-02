-- M2: every resource/detail object keeps local extension values in attributes.
-- Existing readers can roll out incrementally because all columns are defaulted.
ALTER TABLE terminals ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE gates ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE baggage_carousels ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE equipment ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE equipment_types ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE teams ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE personnel_runtime ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE task_types ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE departments ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE dispatch_orders ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE department_qualification_catalog ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;
