-- M2: metadata-driven field definitions. No physical FKs after migration 120.
CREATE TABLE IF NOT EXISTS ontology_field_overlays (
    object_name VARCHAR(64) NOT NULL,
    field_name VARCHAR(128) NOT NULL,
    field_type VARCHAR(32) NOT NULL,
    catalog_code VARCHAR(64),
    object_name_target VARCHAR(64),
    required BOOLEAN NOT NULL DEFAULT FALSE,
    list_visible BOOLEAN NOT NULL DEFAULT FALSE,
    filterable BOOLEAN NOT NULL DEFAULT FALSE,
    widget VARCHAR(32),
    description TEXT,
    visible_when JSONB,
    max_length INTEGER,
    min_value DOUBLE PRECISION,
    max_value DOUBLE PRECISION,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (object_name, field_name),
    CONSTRAINT chk_ontology_field_overlay_type CHECK (
        field_type IN ('string','number','boolean','datetime','catalog_ref','catalog_ref[]','object_ref','object_ref[]')
    )
);

CREATE INDEX IF NOT EXISTS idx_ontology_field_overlays_active
    ON ontology_field_overlays (object_name, is_active);
