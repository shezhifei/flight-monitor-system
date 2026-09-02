-- F2: explicit task binding anchor. This is distinct from generation timing anchors.
ALTER TABLE task_types
    ADD COLUMN IF NOT EXISTS anchor VARCHAR(16) NOT NULL DEFAULT 'link';

UPDATE task_types
SET anchor = CASE lower(category)
    WHEN 'arrival' THEN 'inbound'
    WHEN 'departure' THEN 'outbound'
    ELSE 'link'
END
WHERE anchor IS NULL OR anchor = 'link';

ALTER TABLE task_types
    DROP CONSTRAINT IF EXISTS chk_task_types_anchor;
ALTER TABLE task_types
    ADD CONSTRAINT chk_task_types_anchor CHECK (anchor IN ('inbound', 'outbound', 'link'));

CREATE INDEX IF NOT EXISTS idx_task_types_anchor_active ON task_types (anchor, is_active);
