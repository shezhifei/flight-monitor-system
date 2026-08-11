ALTER TABLE business_case_types
ADD COLUMN IF NOT EXISTS case_properties JSONB NOT NULL DEFAULT '{}'::jsonb;

UPDATE business_case_types
SET case_properties = jsonb_build_object(
    'binding_policy', jsonb_build_object(
        'flight_required', true,
        'allowed_leg_types', jsonb_build_array('outbound'),
        'default_leg_type', 'outbound',
        'leg_type_required', true,
        'flight_match_policy', jsonb_build_object(
            'allow_numeric_suffix', true,
            'exclude_cancelled', true,
            'exclude_departed', true,
            'exclude_actual_departure', true,
            'time_window_hours_before', 3,
            'time_window_hours_after', 8,
            'min_auto_match_score', 0.85
        )
    ),
    'extra_info_schema', jsonb_build_object(
        'fields', jsonb_build_object(
            'seat_no', jsonb_build_object(
                'type', 'string',
                'label', '座位号',
                'required', true,
                'display_in_notification', true
            )
        ),
        'summary_template', '座位号 {{seat_no}}'
    ),
    'workflow_policy', jsonb_build_object(
        'batch_notification_enabled', true,
        'batch_receipt_mode', 'shared_group'
    ),
    'duplicate_policy', jsonb_build_object(
        'enabled', true,
        'fields', jsonb_build_array('seat_no'),
        'include_extra_info', false,
        'include_bound_leg', true,
        'active_statuses', jsonb_build_array('INITIAL', 'PENDING', 'notification_sent', 'waiting_receipts')
    )
)
WHERE code = 'gate_baggage_check'
  AND (case_properties = '{}'::jsonb OR case_properties IS NULL);
