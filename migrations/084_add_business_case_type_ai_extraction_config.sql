ALTER TABLE business_case_types
ADD COLUMN IF NOT EXISTS ai_extraction_config JSONB NOT NULL DEFAULT '{}'::jsonb;

UPDATE business_case_types
SET ai_extraction_config = jsonb_build_object(
    'enabled', true,
    'aliases', jsonb_build_array('登机口开包', '开包', '开包检查'),
    'trigger_phrases', jsonb_build_array('有登机口开包', '开包收到了'),
    'leg_binding', jsonb_build_object(
        'allowed', jsonb_build_array('outbound'),
        'default', 'outbound',
        'required', true
    ),
    'flight_matching', jsonb_build_object(
        'prefer_leg', 'outbound',
        'exclude_cancelled', true,
        'exclude_departed', true,
        'exclude_actual_departure', true,
        'window_hours_before', 3,
        'window_hours_after', 8,
        'min_auto_match_score', 0.86
    ),
    'fields', jsonb_build_object(
        'seat_no', jsonb_build_object(
            'type', 'string',
            'label', '座位号',
            'required', true,
            'aliases', jsonb_build_array('座位号', '座位', '坐位'),
            'examples', jsonb_build_array('23A', '32F', '1A')
        )
    ),
    'description_template', '登机口开包，座位号 {{seat_no}}',
    'remarks_template', '座位号 {{seat_no}}',
    'forbidden_fields', jsonb_build_array('gate'),
    'examples', jsonb_build_array(
        jsonb_build_object(
            'transcript', '7714座位号23A',
            'action', jsonb_build_object(
                'case_type', 'gate_baggage_check',
                'flight_number_raw', '7714',
                'leg_type_hint', 'outbound',
                'fields', jsonb_build_object('seat_no', '23A'),
                'remarks', '座位号 23A'
            )
        )
    )
)
WHERE code = 'gate_baggage_check'
  AND (ai_extraction_config = '{}'::jsonb OR ai_extraction_config IS NULL);

CREATE INDEX IF NOT EXISTS idx_business_case_types_ai_enabled
ON business_case_types (code)
WHERE is_active = TRUE
  AND ai_extraction_config @> '{"enabled": true}'::jsonb;

