-- Metadata catalogs (open/closed code tables). No physical FKs after 120;
-- referential integrity is application-layer (tests/tools/test_no_new_foreign_keys.py).

SET TRANSACTION READ WRITE;

CREATE TABLE IF NOT EXISTS metadata_catalogs (
    code VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    is_open BOOLEAN NOT NULL DEFAULT FALSE,
    is_ordered BOOLEAN NOT NULL DEFAULT FALSE,
    system_owned BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS metadata_catalog_entries (
    catalog_code VARCHAR(64) NOT NULL,
    code VARCHAR(64) NOT NULL,
    name VARCHAR(128) NOT NULL,
    rank INTEGER,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    source VARCHAR(16) NOT NULL DEFAULT 'manual',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (catalog_code, code),
    CONSTRAINT chk_metadata_catalog_entries_source CHECK (source IN ('manual', 'ingest'))
);

CREATE INDEX IF NOT EXISTS idx_metadata_catalog_entries_catalog_active
    ON metadata_catalog_entries (catalog_code, is_active);

INSERT INTO metadata_catalogs (code, name, description, is_open, is_ordered, system_owned)
VALUES
    (
        'icao_size',
        'ICAO 机位等级',
        '封闭有序码表 A–F，用于机位最大可停比较。',
        FALSE,
        TRUE,
        TRUE
    ),
    (
        'aircraft_type',
        '机型',
        '开放码表。电报/导入未见过的机型字符串 upsert 一行，rank 可空。',
        TRUE,
        FALSE,
        TRUE
    )
ON CONFLICT (code) DO NOTHING;

INSERT INTO metadata_catalog_entries (catalog_code, code, name, rank, source)
VALUES
    ('icao_size', 'A', 'A', 1, 'manual'),
    ('icao_size', 'B', 'B', 2, 'manual'),
    ('icao_size', 'C', 'C', 3, 'manual'),
    ('icao_size', 'D', 'D', 4, 'manual'),
    ('icao_size', 'E', 'E', 5, 'manual'),
    ('icao_size', 'F', 'F', 6, 'manual')
ON CONFLICT (catalog_code, code) DO NOTHING;

ALTER TABLE flights DROP COLUMN IF EXISTS aircraft_type_binary;

COMMENT ON TABLE metadata_catalogs IS '元数据码表目录；is_open=true 允许 ingest upsert 新 code';
COMMENT ON TABLE metadata_catalog_entries IS '码表行；引用完整性由应用层 enforce';
COMMENT ON COLUMN metadata_catalog_entries.rank IS '有序比较用，可空（未分级机型）';
