-- M4: anomalies can point at any ontology subject; flight_id remains a compatibility column.
ALTER TABLE anomalies ADD COLUMN IF NOT EXISTS subject_type VARCHAR(64);
ALTER TABLE anomalies ADD COLUMN IF NOT EXISTS subject_id VARCHAR(64);

UPDATE anomalies
SET subject_type = COALESCE(subject_type, 'Flight'),
    subject_id = COALESCE(subject_id, flight_id)
WHERE subject_type IS NULL OR subject_id IS NULL;

ALTER TABLE anomalies ALTER COLUMN subject_type SET NOT NULL;
ALTER TABLE anomalies ALTER COLUMN subject_id SET NOT NULL;
ALTER TABLE anomalies ALTER COLUMN flight_id DROP NOT NULL;

ALTER TABLE anomalies DROP CONSTRAINT IF EXISTS fk_anomalies_flight_id;
CREATE INDEX IF NOT EXISTS idx_anomalies_subject ON anomalies(subject_type, subject_id);
