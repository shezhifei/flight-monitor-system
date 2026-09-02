-- M2: team-type schema overlays are persisted alongside the canonical object.
ALTER TABLE team_types
    ADD COLUMN IF NOT EXISTS attributes JSONB NOT NULL DEFAULT '{}'::jsonb;

COMMENT ON COLUMN team_types.attributes IS
    'Schema-driven ontology field overlay values for TeamType';
