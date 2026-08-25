-- PR #本体两层改造 - Create terminals and member tables
-- This migration creates the terminal directory and member relationship tables
-- as specified in docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md

SET TRANSACTION READ WRITE;

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
-- ============================================================================

-- Terminal-Stand relations
CREATE TABLE IF NOT EXISTS terminal_stands (
    terminal_id VARCHAR(64) NOT NULL REFERENCES terminals(terminal_id) ON DELETE CASCADE,
    stand_id VARCHAR(64) NOT NULL REFERENCES stands(stand_id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (terminal_id, stand_id),
    UNIQUE(stand_id)  -- One stand can only belong to one terminal
);

CREATE INDEX idx_terminal_stands_stand ON terminal_stands(stand_id);
CREATE INDEX idx_terminal_stands_terminal ON terminal_stands(terminal_id);

COMMENT ON TABLE terminal_stands IS '航站楼 - 机位成员关系表';
COMMENT ON COLUMN terminal_stands.stand_id IS '机位 ID - UNIQUE 约束确保一对一归属';

-- Terminal-Gate relations
CREATE TABLE IF NOT EXISTS terminal_gates (
    terminal_id VARCHAR(64) NOT NULL REFERENCES terminals(terminal_id) ON DELETE CASCADE,
    gate_id VARCHAR(64) NOT NULL REFERENCES gates(gate_id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (terminal_id, gate_id),
    UNIQUE(gate_id)  -- One gate can only belong to one terminal
);

CREATE INDEX idx_terminal_gates_gate ON terminal_gates(gate_id);
CREATE INDEX idx_terminal_gates_terminal ON terminal_gates(terminal_id);

COMMENT ON TABLE terminal_gates IS '航站楼 - 登机口成员关系表';
COMMENT ON COLUMN terminal_gates.gate_id IS '登机口 ID - UNIQUE 约束确保一对一归属';

-- Terminal-Carousel relations
CREATE TABLE IF NOT EXISTS terminal_carousels (
    terminal_id VARCHAR(64) NOT NULL REFERENCES terminals(terminal_id) ON DELETE CASCADE,
    carousel_id VARCHAR(64) NOT NULL REFERENCES baggage_carousels(carousel_id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (terminal_id, carousel_id),
    UNIQUE(carousel_id)  -- One carousel can only belong to one terminal
);

CREATE INDEX idx_terminal_carousels_carousel ON terminal_carousels(carousel_id);
CREATE INDEX idx_terminal_carousels_terminal ON terminal_carousels(terminal_id);

COMMENT ON TABLE terminal_carousels IS '航站楼 - 行李转盘成员关系表';
COMMENT ON COLUMN terminal_carousels.carousel_id IS '行李转盘 ID - UNIQUE 约束确保一对一归属';

-- ============================================================================
-- Backfill existing stands with terminal information
-- If there are any legacy relationships, try to match them by stand.code or comments
-- ============================================================================

-- Note: Stand.terminal column will be deprecated - all terminal relationships
-- must go through terminal_stands table after this migration

-- ============================================================================
-- Data validation constraints
-- ============================================================================

-- Prevent orphaned references (will be enforced by FKs above)
-- All new STAND/GATE/CAROUSEL creation MUST include terminal_id reference
-- and atomic INSERT into both parent table + terminal_*_members table

COMMENT ON CONSTRAINT uq_terminal_stands_stand ON terminal_stands IS 
    'Business rule: ONE STAND = ONE TERMINAL. Ensured by UNIQUE(stand_id).';

COMMENT ON CONSTRAINT uq_terminal_gates_gate ON terminal_gates IS 
    'Business rule: ONE GATE = ONE TERMINAL. Ensured by UNIQUE(gate_id).';

COMMENT ON CONSTRAINT uq_terminal_carousels_carousel ON terminal_carousels IS 
    'Business rule: ONE CAROUSEL = ONE TERMINAL. Ensured by UNIQUE(carousel_id).';
