-- PR #本体两层改造 - Create personnel_runtime (人员在岗运行时)
-- As specified in docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md
--
-- "personnel_runtime = 谁能被派工"（谁在岗）。无行视为 off_duty。
-- 与 terminal 目录无关：无 FK（referential integrity at application layer，
-- guard: tests/tools/test_no_new_foreign_keys.py）。

SET TRANSACTION READ WRITE;

CREATE TABLE IF NOT EXISTS personnel_runtime (
    user_id VARCHAR(26) PRIMARY KEY,
    current_status VARCHAR(20) NOT NULL DEFAULT 'off_duty'
        CHECK (current_status IN ('on_duty', 'off_duty', 'break', 'on_leave')),
    current_stand_id VARCHAR(64),
    current_position_lat DECIMAL(10, 7),
    current_position_lng DECIMAL(10, 7),
    last_position_update TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_by VARCHAR(26)
);

CREATE INDEX idx_personnel_runtime_status ON personnel_runtime(current_status);

COMMENT ON TABLE personnel_runtime IS '人员在岗运行时表 - 谁能被派工；无行视为 off_duty';
COMMENT ON COLUMN personnel_runtime.user_id IS '个人账号 user_id (users.id)';
COMMENT ON COLUMN personnel_runtime.current_status IS '在岗状态：on_duty / off_duty / break / on_leave';