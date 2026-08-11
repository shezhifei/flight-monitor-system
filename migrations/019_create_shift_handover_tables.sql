-- Create shift handover tables


CREATE TABLE IF NOT EXISTS shift_handovers (
    handover_id VARCHAR(26) PRIMARY KEY,
    shift_date DATE NOT NULL,
    shift_code VARCHAR(32) NOT NULL,
    from_user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    to_user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    status VARCHAR(16) NOT NULL DEFAULT 'draft',
    summary TEXT,
    risk_level VARCHAR(16) NOT NULL DEFAULT 'medium',
    signed_at TIMESTAMPTZ,
    submitted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_shift_handover_status
        CHECK (status IN ('draft', 'pending', 'sign_off', 'completed')),
    CONSTRAINT chk_shift_handover_risk_level
        CHECK (risk_level IN ('low', 'medium', 'high', 'critical'))
);

CREATE INDEX IF NOT EXISTS idx_shift_handovers_shift_date_status
    ON shift_handovers(shift_date, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_shift_handovers_to_user_status
    ON shift_handovers(to_user_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS shift_handover_items (
    item_id VARCHAR(26) PRIMARY KEY,
    handover_id VARCHAR(26) NOT NULL REFERENCES shift_handovers(handover_id) ON DELETE CASCADE,
    item_type VARCHAR(32) NOT NULL DEFAULT 'other',
    title VARCHAR(255) NOT NULL,
    detail TEXT,
    owner_user_id VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    due_at TIMESTAMPTZ,
    is_mandatory BOOLEAN NOT NULL DEFAULT TRUE,
    acknowledged BOOLEAN NOT NULL DEFAULT FALSE,
    acknowledged_at TIMESTAMPTZ,
    acknowledged_by VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_shift_handover_item_type
        CHECK (item_type IN ('pending_task', 'open_anomaly', 'risk_note', 'other'))
);

CREATE INDEX IF NOT EXISTS idx_shift_handover_items_handover
    ON shift_handover_items(handover_id, created_at ASC);

CREATE INDEX IF NOT EXISTS idx_shift_handover_items_pending
    ON shift_handover_items(handover_id, is_mandatory, acknowledged);

