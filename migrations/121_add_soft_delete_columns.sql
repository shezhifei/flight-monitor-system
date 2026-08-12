-- 121: Add soft-delete marker columns for audited tables.
--
-- Audit requirements forbid physical deletion of business data. All product
-- "delete" operations now set deleted_at instead of removing rows; read
-- paths filter deleted_at IS NULL. See
-- docs/plans/2026-08-12-remove-foreign-keys-spec.md (§3.2.1).
--
-- users is intentionally NOT included here: it reuses the existing
-- is_active flag as its soft-delete marker.
--
-- Idempotent: safe to re-run.

-- Flight domain (A/C class)
ALTER TABLE flights                        ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE flight_legs                    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE flight_dispatch_timeline_events ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE flight_business_cases          ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

-- Todos
ALTER TABLE todos                          ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE todo_state_changes             ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

-- RBAC / organization config (B/C class)
ALTER TABLE label_definitions              ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE permission_templates           ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE roles                          ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE role_permissions               ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE user_roles                     ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE permissions                    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE team_types                     ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE team_type_steps                ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE departments                    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

-- AI configuration / ontology customization (B class)
ALTER TABLE ai_entities                    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE ai_mcp_servers                 ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE ai_agent_skill_registry        ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE ai_entity_mcp_bindings         ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE ai_entity_skill_bindings       ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE aip_constraints                ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE aip_functions                  ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE aip_ontology_actions           ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE aip_ontology_objects           ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE aip_object_policies            ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE aip_tool_mappings              ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE ai_action_proposals            ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

-- Dispatch rule configuration (B class)
ALTER TABLE dispatch_order_adjustment_rules         ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE event_driven_dispatch_generation_rules  ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

-- Filter support indexes (partial: only live rows are filtered on)
CREATE INDEX IF NOT EXISTS idx_flights_deleted_at          ON flights (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_todos_deleted_at            ON todos (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_roles_deleted_at            ON roles (deleted_at) WHERE deleted_at IS NOT NULL;
