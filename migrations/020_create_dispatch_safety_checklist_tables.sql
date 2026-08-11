-- Create dispatch safety checklist template and record tables


CREATE TABLE IF NOT EXISTS dispatch_safety_checklist_templates (
    template_id VARCHAR(26) PRIMARY KEY,
    task_type VARCHAR(50) NOT NULL REFERENCES task_types(code) ON DELETE CASCADE,
    checklist_version VARCHAR(32) NOT NULL,
    checklist_items JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_by VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    updated_by VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_dispatch_safety_template_step_version UNIQUE (task_type, checklist_version),
    CONSTRAINT chk_dispatch_safety_template_items_array CHECK (jsonb_typeof(checklist_items) = 'array')
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_dispatch_safety_template_active_step
    ON dispatch_safety_checklist_templates(task_type)
    WHERE is_active = TRUE;

CREATE INDEX IF NOT EXISTS idx_dispatch_safety_template_step_updated
    ON dispatch_safety_checklist_templates(task_type, updated_at DESC);

CREATE TABLE IF NOT EXISTS dispatch_safety_checklist_records (
    record_id VARCHAR(26) PRIMARY KEY,
    dispatch_order_id VARCHAR(26) NOT NULL REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    item_code VARCHAR(64) NOT NULL,
    result VARCHAR(16) NOT NULL,
    checked_by VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    note TEXT,
    template_version VARCHAR(32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_dispatch_safety_record_order_item UNIQUE (dispatch_order_id, item_code),
    CONSTRAINT chk_dispatch_safety_record_result CHECK (result IN ('pass', 'fail', 'na'))
);

CREATE INDEX IF NOT EXISTS idx_dispatch_safety_record_order_checked
    ON dispatch_safety_checklist_records(dispatch_order_id, checked_at DESC);

UPDATE dispatch_safety_checklist_templates
SET is_active = FALSE,
    updated_at = CURRENT_TIMESTAMP
WHERE task_type IN ('cleaning', 'boarding', 'pushback')
  AND is_active = TRUE;

INSERT INTO dispatch_safety_checklist_templates (
    template_id,
    task_type,
    checklist_version,
    checklist_items,
    is_active,
    created_by,
    updated_by,
    created_at,
    updated_at
) VALUES
(
    'dsl_tpl_cleaning_v1',
    'cleaning',
    'v1',
    '[
        {"item_code":"ppe","title":"PPE check","required":true,"allow_na":false,"order":1},
        {"item_code":"cabin_clear","title":"Cabin clear of tools","required":true,"allow_na":false,"order":2},
        {"item_code":"waste_sealed","title":"Waste sealed and tagged","required":true,"allow_na":false,"order":3}
    ]'::jsonb,
    TRUE,
    NULL,
    NULL,
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
),
(
    'dsl_tpl_boarding_v1',
    'boarding',
    'v1',
    '[
        {"item_code":"door_zone_clear","title":"Door area clear","required":true,"allow_na":false,"order":1},
        {"item_code":"boarding_bridge_lock","title":"Bridge or stair lock check","required":true,"allow_na":false,"order":2},
        {"item_code":"final_manifest_sync","title":"Final manifest synced","required":true,"allow_na":true,"order":3}
    ]'::jsonb,
    TRUE,
    NULL,
    NULL,
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
),
(
    'dsl_tpl_pushback_v1',
    'pushback',
    'v1',
    '[
        {"item_code":"towbar_lock","title":"Towbar lock check","required":true,"allow_na":false,"order":1},
        {"item_code":"chocks_removed","title":"Wheel chocks removed","required":true,"allow_na":false,"order":2},
        {"item_code":"ground_clearance","title":"Ground clearance confirmed","required":true,"allow_na":false,"order":3}
    ]'::jsonb,
    TRUE,
    NULL,
    NULL,
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
)
ON CONFLICT (task_type, checklist_version) DO UPDATE SET
    checklist_items = EXCLUDED.checklist_items,
    is_active = EXCLUDED.is_active,
    updated_at = CURRENT_TIMESTAMP;

