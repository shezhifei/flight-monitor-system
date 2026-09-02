-- M2 application-managed reference index for object_ref/object_ref[] fields.
-- Deliberately contains no physical foreign keys: ontology objects are
-- polymorphic and lifecycle policy is enforced by the application layer.
CREATE TABLE IF NOT EXISTS ontology_attribute_references (
    id BIGSERIAL PRIMARY KEY,
    owner_object_name VARCHAR(128) NOT NULL,
    owner_object_id VARCHAR(128) NOT NULL,
    field_name VARCHAR(128) NOT NULL,
    target_object_name VARCHAR(128) NOT NULL,
    target_key VARCHAR(128) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_ontology_attribute_references_active UNIQUE (owner_object_name, owner_object_id, field_name, target_object_name, target_key, is_active)
);

CREATE INDEX IF NOT EXISTS idx_ontology_attribute_references_target
    ON ontology_attribute_references (target_object_name, target_key);

CREATE INDEX IF NOT EXISTS idx_ontology_attribute_references_owner
    ON ontology_attribute_references (owner_object_name, owner_object_id, field_name);

ALTER TABLE ontology_attribute_references
    ADD COLUMN IF NOT EXISTS is_active BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE ontology_attribute_references
    DROP CONSTRAINT IF EXISTS ontology_attribute_references_owner_object_name_owner_object_id_field_name_target_o_key;
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'ontology_attribute_references'::regclass
          AND conname = 'uq_ontology_attribute_references_active'
    ) THEN
        ALTER TABLE ontology_attribute_references
            ADD CONSTRAINT uq_ontology_attribute_references_active
            UNIQUE (owner_object_name, owner_object_id, field_name, target_object_name, target_key, is_active);
    END IF;
END $$;

COMMENT ON TABLE ontology_attribute_references IS
    '应用层维护的 ontology object_ref 引用索引；停用、改 code、删除前用于冲突清单';
