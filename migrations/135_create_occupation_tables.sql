-- PR #本体两层改造 - Create carousel_assignments (行李转盘分配)
-- As specified in docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md
--
-- NOTE: stand_occupations / gate_assignments are already created by migration 119
-- (ontology_v1_core.sql) with PK `id` and subject `registration`/`flight_id`.
-- This migration only adds the table 119 does NOT define: carousel_assignments.
-- Referential integrity is enforced at the application layer (no FK clauses;
-- guard: tests/tools/test_no_new_foreign_keys.py).

SET TRANSACTION READ WRITE;

-- ============================================================================
-- Carousel Assignments (行李转盘分配)
-- NO business constraints - unlimited concurrent assignments allowed
-- Same carousel can serve multiple flights simultaneously
-- No unique constraint, no overlap check, no direction check
-- 转盘占用与周转链、航班条数互斥无关（plan §占用三对象）
-- ============================================================================
CREATE TABLE IF NOT EXISTS carousel_assignments (
    id VARCHAR(26) PRIMARY KEY,
    carousel_code VARCHAR(64) NOT NULL,
    registration VARCHAR(32),
    flight_id VARCHAR(64),

    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,

    status VARCHAR(16) NOT NULL DEFAULT 'active'
        CHECK (status IN ('draft', 'active', 'released', 'expired')),

    client_action_id VARCHAR(128),  -- 幂等 token（重复 allocate 撞键）

    created_by VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT chk_carousel_assign_time CHECK (ends_at > starts_at)
);

CREATE INDEX idx_carousel_assign_carousels ON carousel_assignments(carousel_code);
CREATE INDEX idx_carousel_assign_flights ON carousel_assignments(flight_id);
CREATE INDEX idx_carousel_assign_reg ON carousel_assignments(registration);
CREATE INDEX idx_carousel_assign_time ON carousel_assignments(starts_at, ends_at);

-- CRITICAL: UNIQUE constraint on client_action_id
-- Since we allow unlimited allocations, the token is the ONLY uniqueness guarantee
CREATE UNIQUE INDEX uq_carousel_assign_client_action ON carousel_assignments(client_action_id)
    WHERE client_action_id IS NOT NULL;

COMMENT ON TABLE carousel_assignments IS '行李转盘分配表 - 无业务约束，允许无限并发';
COMMENT ON COLUMN carousel_assignments.carousel_code IS '转盘编码 - 可被多个航班同时使用';
COMMENT ON COLUMN carousel_assignments.client_action_id IS '幂等 token - 因无自然键，必须由客户端提供唯一 token';