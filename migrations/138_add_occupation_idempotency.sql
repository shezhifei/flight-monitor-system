-- PR #本体两层改造 - 机位/口占用 also accept client idempotency token
-- As specified in docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md
-- Open Questions §2: 三种占用的 allocate 都接客户端幂等 token，落库 + 唯一索引，
-- 重复 token 返回既有行而非新建。
--
-- stand_occupations / gate_assignments 由迁移 119 建表（120 已删 FK）。
-- 这里只补 `client_action_id` 列 + 部分唯一索引（非空才唯一），与 carousel_assignments
-- (135) 的幂等做法一致。
--
-- Referential integrity is enforced at the application layer (no new FK clauses;
-- guard: tests/tools/test_no_new_foreign_keys.py).

SET TRANSACTION READ WRITE;

ALTER TABLE stand_occupations
    ADD COLUMN IF NOT EXISTS client_action_id VARCHAR(128);

CREATE UNIQUE INDEX IF NOT EXISTS uq_stand_occupations_client_action
    ON stand_occupations(client_action_id)
    WHERE client_action_id IS NOT NULL;

COMMENT ON COLUMN stand_occupations.client_action_id IS '客户端幂等 token；重复 allocate 撞键返回既有行';

ALTER TABLE gate_assignments
    ADD COLUMN IF NOT EXISTS client_action_id VARCHAR(128);

CREATE UNIQUE INDEX IF NOT EXISTS uq_gate_assignments_client_action
    ON gate_assignments(client_action_id)
    WHERE client_action_id IS NOT NULL;

COMMENT ON COLUMN gate_assignments.client_action_id IS '客户端幂等 token；重复 allocate 撞键返回既有行';