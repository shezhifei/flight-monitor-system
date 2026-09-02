-- F3: apply the flight identity split after migration 153 preflight.
--
-- The migration is deliberately explicit and resumable:
--   * directional flights receive deterministic ids derived from the legacy id;
--   * the legacy aggregate row is retained as a soft-deleted audit row;
--   * the legacy id becomes the TurnaroundLink id and the monitor row_id is
--     never changed;
--   * all known flight_id references are remapped before the old row is hidden.
-- No physical DELETE is used.

ALTER TABLE flights
    ADD COLUMN IF NOT EXISTS direction VARCHAR(16),
    ADD COLUMN IF NOT EXISTS flight_type VARCHAR(16),
    ADD COLUMN IF NOT EXISTS mission SMALLINT,
    ADD COLUMN IF NOT EXISTS origin_stations JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS destination_stations JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS is_vip BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS stand_type VARCHAR(64),
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

ALTER TABLE flights
    DROP CONSTRAINT IF EXISTS chk_flights_direction_contract;
ALTER TABLE flights
    ADD CONSTRAINT chk_flights_direction_contract
    CHECK (
        deleted_at IS NOT NULL
        OR (direction IS NOT NULL AND direction IN ('inbound', 'outbound'))
    )
    NOT VALID;

CREATE TABLE IF NOT EXISTS f3_identity_map (
    old_flight_id VARCHAR(26) NOT NULL,
    leg_type VARCHAR(16) NOT NULL,
    leg_id VARCHAR(26) NOT NULL,
    new_flight_id VARCHAR(26) NOT NULL,
    mapping_version VARCHAR(32) NOT NULL DEFAULT '155',
    mapped_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (old_flight_id, leg_type),
    UNIQUE (new_flight_id)
);

ALTER TABLE f3_identity_map
    ADD COLUMN IF NOT EXISTS mapping_version VARCHAR(32) NOT NULL DEFAULT '155',
    ADD COLUMN IF NOT EXISTS mapped_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

COMMENT ON TABLE f3_identity_map IS
    'F3 航班身份拆分审计映射；old_flight_id 是已隐藏的旧聚合身份，new_flight_id 是方向航班身份。';
COMMENT ON COLUMN f3_identity_map.mapping_version IS
    '产生该映射的迁移版本；不要作为业务外键使用。';
COMMENT ON COLUMN f3_identity_map.mapped_at IS
    '映射首次生成时间；迁移重跑不会清除审计记录。';

-- 只清理由上一次中断执行留下、且仍对应当前活动候选的映射；已完成的
-- 映射记录保留为审计证据。整个迁移在事务中执行，正常失败不会留下半成品。
DELETE FROM f3_identity_map m
WHERE EXISTS (
    SELECT 1
    FROM flights f
    WHERE f.flight_id = m.old_flight_id
      AND f.deleted_at IS NULL
      AND (f.direction IS NULL OR f.direction = 'both')
);

INSERT INTO f3_identity_map (old_flight_id, leg_type, leg_id, new_flight_id)
SELECT f.flight_id,
       l.leg_type,
       l.leg_id,
       SUBSTRING(MD5(f.flight_id || ':' || l.leg_type || ':flight-v2'), 1, 26)
FROM flights f
JOIN flight_legs l ON l.flight_id = f.flight_id AND l.deleted_at IS NULL
WHERE f.deleted_at IS NULL
  AND (f.direction IS NULL OR f.direction = 'both')
  AND l.leg_type IN ('inbound', 'outbound');

DO $$
DECLARE bad_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO bad_count
    FROM flight_identity_split_report r
    JOIN flights f ON f.flight_id = r.old_flight_id
    WHERE r.action = 'fail'
      AND f.deleted_at IS NULL
      AND (f.direction IS NULL OR f.direction = 'both');
    IF bad_count > 0 THEN
        RAISE EXCEPTION 'F3 apply refused: preflight report contains % invalid rows', bad_count;
    END IF;

    IF EXISTS (
    SELECT 1 FROM f3_identity_map m
        GROUP BY m.old_flight_id
        HAVING COUNT(*) NOT BETWEEN 1 AND 2
           OR COUNT(*) FILTER (WHERE m.leg_type = 'inbound') > 1
           OR COUNT(*) FILTER (WHERE m.leg_type = 'outbound') > 1
    ) THEN
        RAISE EXCEPTION 'F3 apply refused: identity map contains invalid leg cardinality';
    END IF;
END $$;

-- A single-leg flight keeps its original identity. Two-leg flights use the
-- deterministic directional ids generated above.
UPDATE f3_identity_map m
SET new_flight_id = old_flight_id
WHERE m.old_flight_id IN (
    SELECT old_flight_id FROM f3_identity_map GROUP BY old_flight_id HAVING COUNT(*) = 1
);

-- One-leg rows retain their original id, but become directional and take the
-- canonical leg values into the merged flight columns.
UPDATE flights f
SET direction = m.leg_type,
    flight_number = l.flight_no,
    flight_type = l.flight_type,
    mission = l.mission,
    origin_stations = l.origin_stations,
    destination_stations = l.destination_stations,
    is_vip = l.is_vip,
    stand_type = l.stand_type,
    scheduled_departure = CASE WHEN m.leg_type = 'outbound' THEN l.scheduled_time ELSE NULL END,
    scheduled_arrival = CASE WHEN m.leg_type = 'inbound' THEN l.scheduled_time ELSE NULL END,
    estimated_departure = CASE WHEN m.leg_type = 'outbound' THEN f.estimated_departure ELSE NULL END,
    estimated_arrival = CASE WHEN m.leg_type = 'inbound' THEN f.estimated_arrival ELSE NULL END,
    actual_departure = CASE WHEN m.leg_type = 'outbound' THEN f.actual_departure ELSE NULL END,
    actual_arrival = CASE WHEN m.leg_type = 'inbound' THEN f.actual_arrival ELSE NULL END,
    is_quick_turnaround = FALSE,
    updated_at = NOW()
FROM f3_identity_map m
JOIN flight_legs l ON l.leg_id = m.leg_id
WHERE f.flight_id = m.old_flight_id
  AND m.old_flight_id IN (
      SELECT old_flight_id FROM f3_identity_map GROUP BY old_flight_id HAVING COUNT(*) = 1
  );

-- Re-parent the leg rows before hiding the aggregate row. This keeps the
-- existing write/read compatibility layer usable until F4 removes it.
UPDATE flight_legs l
SET flight_id = m.new_flight_id,
    updated_at = NOW()
FROM f3_identity_map m
WHERE l.leg_id = m.leg_id;

-- Two-leg rows are cloned into two directional flights. The dynamic column
-- list preserves fields added by earlier migrations without relying on a
-- fragile hand-maintained SELECT f.* list.
DO $$
DECLARE
    cols TEXT;
    expr TEXT;
    item RECORD;
BEGIN
    SELECT string_agg(format('%I', c.column_name), ', ' ORDER BY c.ordinal_position)
      INTO cols
    FROM information_schema.columns c
    WHERE c.table_schema = 'public'
      AND c.table_name = 'flights'
      AND c.column_name NOT IN ('flight_id', 'deleted_at');

    FOR item IN
        SELECT m.old_flight_id, m.leg_type, m.leg_id, m.new_flight_id
        FROM f3_identity_map m
        WHERE m.old_flight_id IN (
            SELECT old_flight_id FROM f3_identity_map GROUP BY old_flight_id HAVING COUNT(*) = 2
        )
        ORDER BY m.old_flight_id, m.leg_type
    LOOP
        SELECT string_agg(
            CASE c.column_name
                WHEN 'direction' THEN format('%L', item.leg_type)
                WHEN 'flight_number' THEN 'l.flight_no'
                WHEN 'flight_type' THEN 'l.flight_type'
                WHEN 'mission' THEN 'l.mission'
                WHEN 'origin_stations' THEN 'l.origin_stations'
                WHEN 'destination_stations' THEN 'l.destination_stations'
                WHEN 'is_vip' THEN 'l.is_vip'
                WHEN 'stand_type' THEN 'l.stand_type'
                WHEN 'scheduled_departure' THEN format('CASE WHEN %L = ''outbound'' THEN l.scheduled_time ELSE NULL END', item.leg_type)
                WHEN 'scheduled_arrival' THEN format('CASE WHEN %L = ''inbound'' THEN l.scheduled_time ELSE NULL END', item.leg_type)
                WHEN 'estimated_departure' THEN format('CASE WHEN %L = ''outbound'' THEN f.estimated_departure ELSE NULL END', item.leg_type)
                WHEN 'estimated_arrival' THEN format('CASE WHEN %L = ''inbound'' THEN f.estimated_arrival ELSE NULL END', item.leg_type)
                WHEN 'actual_departure' THEN format('CASE WHEN %L = ''outbound'' THEN f.actual_departure ELSE NULL END', item.leg_type)
                WHEN 'actual_arrival' THEN format('CASE WHEN %L = ''inbound'' THEN f.actual_arrival ELSE NULL END', item.leg_type)
                WHEN 'is_quick_turnaround' THEN 'FALSE'
                ELSE format('f.%I', c.column_name)
            END,
            ', ' ORDER BY c.ordinal_position
        ) INTO expr
        FROM information_schema.columns c
        WHERE c.table_schema = 'public'
          AND c.table_name = 'flights'
          AND c.column_name NOT IN ('flight_id', 'deleted_at');

        EXECUTE format(
            'INSERT INTO flights (flight_id, %s) SELECT %L, %s FROM flights f JOIN flight_legs l ON l.leg_id = %L WHERE f.flight_id = %L ON CONFLICT (flight_id) DO NOTHING',
            cols, item.new_flight_id, expr, item.leg_id, item.old_flight_id
        );
    END LOOP;
END $$;

-- Ensure one-to-one links use the legacy aggregate id. A pre-existing link
-- with the same id must already point to the same pair.
UPDATE turnaround_links tl
SET inbound_flight_id = m.new_flight_id
FROM f3_identity_map m
WHERE tl.inbound_flight_id = m.old_flight_id AND m.leg_type = 'inbound';
UPDATE turnaround_links tl
SET outbound_flight_id = m.new_flight_id
FROM f3_identity_map m
WHERE tl.outbound_flight_id = m.old_flight_id AND m.leg_type = 'outbound';

INSERT INTO turnaround_links (
    id, inbound_flight_id, outbound_flight_id, status, source,
    broken_reason, created_by, created_at, updated_at
)
SELECT old_flight_id,
       MAX(new_flight_id) FILTER (WHERE leg_type = 'inbound'),
       MAX(new_flight_id) FILTER (WHERE leg_type = 'outbound'),
       'active', 'auto', NULL, 'migration:155', NOW(), NOW()
FROM f3_identity_map
GROUP BY old_flight_id
HAVING COUNT(*) = 2
ON CONFLICT (id) DO UPDATE
SET inbound_flight_id = EXCLUDED.inbound_flight_id,
    outbound_flight_id = EXCLUDED.outbound_flight_id,
    status = 'active', updated_at = NOW();

-- Remap order subjects according to the frozen TaskType.anchor contract.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM dispatch_orders d
        JOIN f3_identity_map m ON m.old_flight_id = d.flight_id
        LEFT JOIN task_types t ON t.code = d.task_type
        WHERE m.old_flight_id IN (SELECT old_flight_id FROM f3_identity_map GROUP BY old_flight_id HAVING COUNT(*) = 2)
          AND (t.anchor IS NULL OR t.anchor NOT IN ('inbound', 'outbound', 'link'))
    ) THEN
        RAISE EXCEPTION 'F3 apply refused: dispatch order has missing/invalid TaskType.anchor';
    END IF;
END $$;

UPDATE dispatch_orders d
SET flight_id = CASE
    WHEN t.anchor = 'outbound' THEN m_out.new_flight_id
    WHEN t.anchor = 'link' THEN m.old_flight_id
    ELSE m_in.new_flight_id
END
FROM f3_identity_map m
LEFT JOIN f3_identity_map m_in ON m_in.old_flight_id = m.old_flight_id AND m_in.leg_type = 'inbound'
LEFT JOIN f3_identity_map m_out ON m_out.old_flight_id = m.old_flight_id AND m_out.leg_type = 'outbound'
, task_types t
WHERE d.flight_id = m.old_flight_id
  AND m.leg_type = 'inbound'
  AND t.code = d.task_type
  AND t.anchor IN ('inbound', 'outbound', 'link')
  AND (CASE
      WHEN t.anchor = 'outbound' THEN m_out.new_flight_id
      WHEN t.anchor = 'link' THEN m.old_flight_id
      ELSE m_in.new_flight_id
  END) IS NOT NULL;

-- Direction-specific ontology records.
UPDATE gate_assignments g
SET flight_id = m.new_flight_id
FROM f3_identity_map m
WHERE g.flight_id = m.old_flight_id AND m.leg_type = 'outbound';

UPDATE carousel_assignments c
SET flight_id = m.new_flight_id
FROM f3_identity_map m
WHERE c.flight_id = m.old_flight_id AND m.leg_type = 'inbound';

UPDATE stand_occupations s
SET flight_id = m.new_flight_id
FROM f3_identity_map m
WHERE s.flight_id = m.old_flight_id AND m.leg_type = 'inbound';

-- Subject-first anomaly contract: keep the inbound id for legacy consumers,
-- but make the turnaround link the canonical subject.
UPDATE anomalies a
SET subject_type = 'TurnaroundLink',
    subject_id = m.old_flight_id,
    flight_id = m_in.new_flight_id
FROM f3_identity_map m
JOIN f3_identity_map m_in ON m_in.old_flight_id = m.old_flight_id AND m_in.leg_type = 'inbound'
WHERE a.flight_id = m.old_flight_id AND m.leg_type = 'inbound'
  AND m.old_flight_id IN (SELECT old_flight_id FROM f3_identity_map GROUP BY old_flight_id HAVING COUNT(*) = 2);

-- Timeline events carry an optional leg_type, so preserve that direction.
UPDATE flight_dispatch_timeline_events e
SET flight_id = COALESCE(
    CASE WHEN e.leg_type = 'outbound' THEN m_out.new_flight_id ELSE m_in.new_flight_id END,
    m.old_flight_id
)
FROM f3_identity_map m
LEFT JOIN f3_identity_map m_in ON m_in.old_flight_id = m.old_flight_id AND m_in.leg_type = 'inbound'
LEFT JOIN f3_identity_map m_out ON m_out.old_flight_id = m.old_flight_id AND m_out.leg_type = 'outbound'
WHERE e.flight_id = m.old_flight_id AND m.leg_type = 'inbound';

-- All remaining scalar flight_id references are historical/runtime projections;
-- point them at the inbound directional flight. This loop is intentionally
-- explicit about excluded tables whose semantics were handled above.
DO $$
DECLARE r RECORD;
BEGIN
    -- 这里必须是显式白名单。新增包含 flight_id 的表时，迁移应当先失败，
    -- 而不是静默地把它当成 inbound 的历史投影。
    FOR r IN
        SELECT table_name
        FROM (VALUES
            ('flight_sync_bindings'),
            ('flight_identity_bindings'),
            ('flight_aircraft_sequences'),
            ('flight_state_changes'),
            ('flight_business_cases'),
            ('business_case_workflow_runs'),
            ('snapshots'),
            ('event_stream_versions'),
            ('workflow_dispatch_mappings'),
            ('dispatch_alerts'),
            ('dispatch_collaboration_events'),
            ('dispatch_chat_groups'),
            ('flight_custom_field_archive'),
            ('flight_runtime_list_projection'),
            ('resource_adjustment_suggestions'),
            ('notifications')
        ) AS allowed(table_name)
    LOOP
        IF to_regclass(format('public.%I', r.table_name)) IS NULL THEN
            CONTINUE;
        END IF;
        IF NOT EXISTS (
            SELECT 1
            FROM information_schema.columns c
            WHERE c.table_schema = 'public'
              AND c.table_name = r.table_name
              AND c.column_name = 'flight_id'
        ) THEN
            CONTINUE;
        END IF;
        EXECUTE format(
            'UPDATE public.%I t SET flight_id = m.new_flight_id FROM f3_identity_map m WHERE t.flight_id = m.old_flight_id AND m.leg_type = ''inbound''',
            r.table_name
        );
    END LOOP;

    FOR r IN
        SELECT DISTINCT c.table_name
        FROM information_schema.columns c
        WHERE c.table_schema = 'public'
          AND c.column_name = 'flight_id'
          AND c.table_name NOT IN (
              'flights', 'flight_legs', 'flight_monitor_rows', 'flight_identity_split_report',
              'dispatch_orders', 'gate_assignments', 'carousel_assignments',
              'stand_occupations', 'anomalies', 'flight_dispatch_timeline_events',
              'turnaround_links', 'f3_identity_map', 'archived_flights',
              'archived_flight_state_changes', 'archived_flight_business_cases',
              'archived_snapshots', 'archived_event_stream_versions',
              'flight_sync_bindings', 'flight_identity_bindings', 'flight_aircraft_sequences',
              'flight_state_changes', 'flight_business_cases', 'business_case_workflow_runs',
              'snapshots', 'event_stream_versions', 'workflow_dispatch_mappings',
              'dispatch_alerts', 'dispatch_collaboration_events', 'dispatch_chat_groups',
              'flight_custom_field_archive', 'flight_runtime_list_projection',
              'resource_adjustment_suggestions', 'notifications'
          )
    LOOP
        RAISE EXCEPTION 'F3 apply refused: table %.% with flight_id is not in remap whitelist',
            'public', r.table_name;
    END LOOP;
END $$;

-- Keep the monitor row_id stable. For split rows, populate the two new
-- directional ids and link id while retaining the old aggregate row key.
UPDATE flight_monitor_rows r
SET link_id = m.old_flight_id,
    kind = 'turnaround',
    inbound_flight_id = m_in.new_flight_id,
    outbound_flight_id = m_out.new_flight_id,
    is_active = TRUE,
    updated_at = NOW()
FROM f3_identity_map m
JOIN f3_identity_map m_in ON m_in.old_flight_id = m.old_flight_id AND m_in.leg_type = 'inbound'
JOIN f3_identity_map m_out ON m_out.old_flight_id = m.old_flight_id AND m_out.leg_type = 'outbound'
WHERE r.row_id = m.old_flight_id AND m.leg_type = 'inbound';

-- Retain the legacy aggregate as a hidden audit row, never as an active
-- directional Flight and never as a query-time concatenation source.
UPDATE flights f
SET direction = NULL, deleted_at = COALESCE(deleted_at, NOW()), updated_at = NOW()
WHERE f.flight_id IN (
    SELECT old_flight_id FROM f3_identity_map GROUP BY old_flight_id HAVING COUNT(*) = 2
);

-- Active flights must obey the two-value direction contract after cutover.
DO $$
DECLARE
    r RECORD;
    remaining BIGINT;
BEGIN
    -- All explicitly remapped scalar tables must be free of split aggregate ids.
    -- Directional rows may legitimately retain the old id only in the audit/link
    -- tables handled above. Missing tables are tolerated for staged deployments.
    FOR r IN
        SELECT table_name
        FROM (VALUES
            ('flight_sync_bindings'), ('flight_identity_bindings'),
            ('flight_aircraft_sequences'), ('flight_state_changes'),
            ('flight_business_cases'), ('business_case_workflow_runs'),
            ('snapshots'), ('event_stream_versions'), ('workflow_dispatch_mappings'),
            ('dispatch_alerts'), ('dispatch_collaboration_events'),
            ('dispatch_chat_groups'), ('flight_custom_field_archive'),
            ('flight_runtime_list_projection'), ('resource_adjustment_suggestions'),
            ('notifications')
        ) AS allowed(table_name)
    LOOP
        IF to_regclass(format('public.%I', r.table_name)) IS NULL THEN
            CONTINUE;
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = r.table_name AND column_name = 'flight_id'
        ) THEN
            CONTINUE;
        END IF;
        EXECUTE format(
            'SELECT COUNT(*) FROM public.%I t JOIN f3_identity_map m ON m.old_flight_id = t.flight_id AND m.leg_type = ''inbound''',
            r.table_name
        ) INTO remaining;
        IF remaining > 0 THEN
            RAISE EXCEPTION 'F3 apply failed: % rows still point to split aggregate ids in %', remaining, r.table_name;
        END IF;
    END LOOP;

    IF EXISTS (
        SELECT 1
        FROM flights
        WHERE deleted_at IS NULL
          AND (direction IS NULL OR direction NOT IN ('inbound', 'outbound'))
    ) THEN
        RAISE EXCEPTION 'F3 apply failed: active flight with invalid direction remains';
    END IF;
END $$;

-- Validate only after every legacy aggregate has been backfilled or hidden.
-- `NOT VALID` above keeps the migration applicable to an existing database;
-- this final validation makes the invariant fully trusted for the planner and
-- for future writes.
ALTER TABLE flights VALIDATE CONSTRAINT chk_flights_direction_contract;

COMMENT ON COLUMN flights.direction IS '航班方向：inbound 或 outbound；旧聚合行仅保留为 deleted audit row';
