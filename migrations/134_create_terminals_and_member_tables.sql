-- PR #本体两层改造 - Create terminal directory + member tables
-- As specified in docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md
--
-- Referential integrity is enforced at the application layer (per
-- docs/plans/2026-08-12-remove-foreign-keys-spec.md), NOT by database FKs.
-- This migration therefore uses plain ID columns with UNIQUE constraints,
-- no FOREIGN KEY / REFERENCES clauses (guard: tests/tools/test_no_new_foreign_keys.py).

SET TRANSACTION READ WRITE;

-- ============================================================================
-- Gate Directory (登机口目录)
-- ============================================================================
CREATE TABLE IF NOT EXISTS gates (
    gate_id VARCHAR(64) PRIMARY KEY,
    code VARCHAR(16) NOT NULL UNIQUE,  -- e.g. G-A01
    name VARCHAR(128),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_gates_code ON gates(code);
CREATE INDEX idx_gates_active ON gates(is_active);

COMMENT ON TABLE gates IS '登机口目录 - 构成事实是 terminal_gates 成员关系表';

-- ============================================================================
-- Baggage Carousel Directory (行李转盘目录)
-- ============================================================================
CREATE TABLE IF NOT EXISTS baggage_carousels (
    carousel_id VARCHAR(64) PRIMARY KEY,
    code VARCHAR(16) NOT NULL UNIQUE,  -- e.g. B1
    name VARCHAR(128),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_baggage_carousels_code ON baggage_carousels(code);
CREATE INDEX idx_baggage_carousels_active ON baggage_carousels(is_active);

COMMENT ON TABLE baggage_carousels IS '行李转盘目录 - 构成事实是 terminal_carousels 成员关系表';

-- ============================================================================
-- Terminal Directory (航站楼目录)
-- ============================================================================
CREATE TABLE IF NOT EXISTS terminals (
    terminal_id VARCHAR(64) PRIMARY KEY,
    code VARCHAR(16) NOT NULL UNIQUE,  -- T1/T2/T3
    name VARCHAR(128) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_terminals_code ON terminals(code);
CREATE INDEX idx_terminals_active ON terminals(is_active);

COMMENT ON TABLE terminals IS '航站楼目录 - 构成事实是成员表';
COMMENT ON COLUMN terminals.code IS '航站楼编码 (T1/T2/T3)';
COMMENT ON COLUMN terminals.is_active IS '是否启用';

-- ============================================================================
-- Terminal Member Tables (航站楼成员关系表)
-- 一口/一机位/一转盘同时只属于一座楼 - UNIQUE constraint ensures this
-- NOTE: no FK to stands/gates/baggage_carousels (application-layer integrity)
-- ============================================================================

-- Terminal-Stand relations (stand_id refers to stands.id at the application layer)
CREATE TABLE IF NOT EXISTS terminal_stands (
    terminal_id VARCHAR(64) NOT NULL,
    stand_id VARCHAR(64) NOT NULL,
    added_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (terminal_id, stand_id),
    UNIQUE(stand_id)  -- One stand can only belong to one terminal
);

CREATE INDEX idx_terminal_stands_stand ON terminal_stands(stand_id);
CREATE INDEX idx_terminal_stands_terminal ON terminal_stands(terminal_id);

COMMENT ON TABLE terminal_stands IS '航站楼 - 机位成员关系表';
COMMENT ON COLUMN terminal_stands.stand_id IS '机位 ID (stands.id) - UNIQUE 约束确保一对一归属';

-- Terminal-Gate relations
CREATE TABLE IF NOT EXISTS terminal_gates (
    terminal_id VARCHAR(64) NOT NULL,
    gate_id VARCHAR(64) NOT NULL,
    added_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (terminal_id, gate_id),
    UNIQUE(gate_id)  -- One gate can only belong to one terminal
);

CREATE INDEX idx_terminal_gates_gate ON terminal_gates(gate_id);
CREATE INDEX idx_terminal_gates_terminal ON terminal_gates(terminal_id);

COMMENT ON TABLE terminal_gates IS '航站楼 - 登机口成员关系表';
COMMENT ON COLUMN terminal_gates.gate_id IS '登机口 ID (gates.gate_id) - UNIQUE 约束确保一对一归属';

-- Terminal-Carousel relations
CREATE TABLE IF NOT EXISTS terminal_carousels (
    terminal_id VARCHAR(64) NOT NULL,
    carousel_id VARCHAR(64) NOT NULL,
    added_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (terminal_id, carousel_id),
    UNIQUE(carousel_id)  -- One carousel can only belong to one terminal
);

CREATE INDEX idx_terminal_carousels_carousel ON terminal_carousels(carousel_id);
CREATE INDEX idx_terminal_carousels_terminal ON terminal_carousels(terminal_id);

COMMENT ON TABLE terminal_carousels IS '航站楼 - 行李转盘成员关系表';
COMMENT ON COLUMN terminal_carousels.carousel_id IS '行李转盘 ID (baggage_carousels.carousel_id) - UNIQUE 约束确保一对一归属';

-- ============================================================================
-- Business rule comments
-- ============================================================================

-- Prevent orphaned references: enforced at the application layer (no FKs).
-- All new STAND/GATE/CAROUSEL creation must include terminal_id reference
-- and atomic INSERT into both parent table + terminal_*_members table.