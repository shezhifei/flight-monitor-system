-- Add KPI degradation anomaly rule


INSERT INTO anomaly_rules (
    rule_id,
    rule_type,
    name,
    enabled,
    config,
    severity,
    auto_create_todo,
    todo_priority,
    escalation_intervals
) VALUES (
    'kpi_degradation_otp',
    'kpi_degradation',
    'OTP degradation alert',
    TRUE,
    '{"metric":"on_time_departure_rate","threshold":0.7,"window_hours":4}'::jsonb,
    'high',
    TRUE,
    'HIGH',
    '[10,30,60]'::jsonb
)
ON CONFLICT (rule_id) DO NOTHING;

