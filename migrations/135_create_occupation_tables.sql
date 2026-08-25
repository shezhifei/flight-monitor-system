-- PR #本体两层改造 - Create occupation tables (StandOccupation, GateAssignment, CarouselAssignment)
-- As specified in docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md

SET TRANSACTION READ WRITE;

-- ============================================================================
-- Stand Occupations (机位占用)
-- 主体是飞机 (registration); flight_id 可选用于关联航班
-- 重叠只告警不硬拦
-- ============================================================================
CREATE TABLE IF NOT EXISTS stand_occupations (
    occupation_id VARCHAR(64) PRIMARY KEY,
    stand_code VARCHAR(64) NOT NULL,
    registration VARCHAR(16) NOT NULL,  -- Aircraft registration (唯一标识)
    flight_id VARCHAR(64),  -- Optional: related flight for display purposes
    
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    
    status VARCHAR(20) NOT NULL DEFAULT 'draft' 
        CHECK (status IN ('draft', 'active', 'released')),
    
    client_action_id VARCHAR(128),  -- Idempotency token from client (UNIQUE!)
    
    created_by VARCHAR(64),  -- User ID
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_stand_occ_stands ON stand_occupations(stand_code);
CREATE INDEX idx_stand_occ_reg ON stand_occupations(registration);
CREATE INDEX idx_stand_occ_flight ON stand_occupations(flight_id);
CREATE INDEX idx_stand_occ_time ON stand_occupations(starts_at, ends_at);
CREATE INDEX idx_stand_occ_status ON stand_occupations(status);

-- CRITICAL: UNIQUE constraint on client_action_id for idempotent allocate
CREATE UNIQUE INDEX uq_stand_occ_client_action ON stand_occupations(client_action_id) 
    WHERE client_action_id IS NOT NULL;

COMMENT ON TABLE stand_occupations IS '机位占用表 - 主体是飞机注册码';
COMMENT ON COLUMN stand_occupations.registration IS '飞机注册号 - StandOccupation 的主体';
COMMENT ON COLUMN stand_occupations.flight_id IS '关联航班 - 仅用于展示回写，不是约束条件';
COMMENT ON COLUMN stand_occupations.client_action_id IS '幂等 token - 防止重复 allocate，由客户端提供';
COMMENT ON CONSTRAINT chk_stand_occ_time ON stand_occupations IS 
    'starts_at < ends_at must be enforced by application logic';

-- ============================================================================
-- Gate Assignments (登机口分配)
-- 主体是 flight_id (REQUIRED!) - NOT aircraft
-- 同航班一条限制
-- Draft 航班不可占用
-- ============================================================================
CREATE TABLE IF NOT EXISTS gate_assignments (
    assignment_id VARCHAR(64) PRIMARY KEY,
    gate_code VARCHAR(64) NOT NULL,
    flight_id VARCHAR(64) NOT NULL,  -- REQUIRED! Not aircraft
    
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    
    status VARCHAR(20) NOT NULL DEFAULT 'draft' 
        CHECK (status IN ('draft', 'active', 'released')),
    
    client_action_id VARCHAR(128),  -- Idempotency token
    
    created_by VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    CONSTRAINT fk_gate_assignment_flight FOREIGN KEY (flight_id) REFERENCES flights(flight_id)
);

CREATE INDEX idx_gate_assign_gates ON gate_assignments(gate_code);
CREATE INDEX idx_gate_assign_flights ON gate_assignments(flight_id);
CREATE INDEX idx_gate_assign_time ON gate_assignments(starts_at, ends_at);
CREATE INDEX idx_gate_assign_status ON gate_assignments(status);

-- UNIQUE constraint on client_action_id
CREATE UNIQUE INDEX uq_gate_assign_client_action ON gate_assignments(client_action_id) 
    WHERE client_action_id IS NOT NULL;

-- Business rule: One flight can only have ONE active gate assignment at a time
CREATE UNIQUE INDEX uq_gate_assign_flight_active 
    ON gate_assignments(flight_id) 
    WHERE status = 'active';

COMMENT ON TABLE gate_assignments IS '登机口分配表 - 主体是航班 flight_id';
COMMENT ON COLUMN gate_assignments.flight_id IS '航班 ID - GATE ASSIGNMENT 的主体，必填';
COMMENT ON COLUMN gate_assignments.client_action_id IS '幂等 token - 必须与 stand_occupations 采用同一模式';

-- ============================================================================
-- Carousel Assignments (行李转盘分配)
-- NO business constraints - unlimited concurrent assignments allowed
-- Same carousel can serve multiple flights simultaneously
-- No unique constraint, no overlap check, no direction check
-- Cancled outbound flights can also allocate (just usage pattern)
-- ============================================================================
CREATE TABLE IF NOT EXISTS carousel_assignments (
    assignment_id VARCHAR(64) PRIMARY KEY,
    carousel_code VARCHAR(64) NOT NULL,
    flight_id VARCHAR(64) NOT NULL,
    
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    
    status VARCHAR(20) NOT NULL DEFAULT 'draft' 
        CHECK (status IN ('draft', 'active', 'released')),
    
    client_action_id VARCHAR(128),  -- MUST use explicit token
    
    created_by VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    CONSTRAINT fk_carousel_assign_flight FOREIGN KEY (flight_id) REFERENCES flights(flight_id)
);

CREATE INDEX idx_carousel_assign_carousels ON carousel_assignments(carousel_code);
CREATE INDEX idx_carousel_assign_flights ON carousel_assignments(flight_id);
CREATE INDEX idx_carousel_assign_time ON carousel_assignments(starts_at, ends_at);
CREATE INDEX idx_carousel_assign_status ON carousel_assignments(status);

-- CRITICAL: UNIQUE constraint on client_action_id
-- Since we allow unlimited allocations, the token is the ONLY uniqueness guarantee
CREATE UNIQUE INDEX uq_carousel_assign_client_action ON carousel_assignments(client_action_id) 
    WHERE client_action_id IS NOT NULL;

COMMENT ON TABLE carousel_assignments IS '行李转盘分配表 - 无业务约束，允许无限并发';
COMMENT ON COLUMN carousel_assignments.carousel_code IS '转盘编码 - 可被多个航班同时使用';
COMMENT ON COLUMN carousel_assignments.client_action_id IS '幂等 token - 因无自然键，必须由客户端提供唯一 token';
COMMENT ON CONSTRAINT chk_carousel_no_overlap ON carousel_assignments IS 
    'NO constraint: same carousel + flight + window can have MULTIPLE records';

-- ============================================================================
-- Sync with Flight table display columns
-- After migration:
-- - flights.stand -> Display column for active stand_occupation via registration projection
-- - flights.gate -> Display column for active gate_assignment via flight_id
-- - flights.baggage_carousel -> Aggregate all carousel_codes from carousel_assignments for flight
-- - flights.terminal -> Derived from terminal_stands via stand_occupation's stand_code
-- These are SHOW-ONLY columns, NOT write targets
-- ============================================================================

-- ============================================================================
-- Historical data migration strategy
-- ============================================================================

-- Note: Existing gate_assignments that don't have flight_id will need:
-- 1. Backfill from flight_leg timeline if possible
-- 2. Mark as released/nullify if cannot determine flight_id
-- 3. Separate migration script to handle edge cases
