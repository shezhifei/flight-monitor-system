-- 异常监控、自然语言查询日志与 KPI 物化视图


CREATE TABLE IF NOT EXISTS anomaly_rules (
    rule_id VARCHAR(64) PRIMARY KEY,
    rule_type VARCHAR(64) NOT NULL,
    name VARCHAR(128) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    severity VARCHAR(16) NOT NULL DEFAULT 'medium',
    auto_create_todo BOOLEAN NOT NULL DEFAULT TRUE,
    todo_priority VARCHAR(16) NOT NULL DEFAULT 'HIGH',
    escalation_intervals JSONB NOT NULL DEFAULT '[5, 15, 30]'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_anomaly_rules_severity CHECK (severity IN ('low', 'medium', 'high', 'critical'))
);

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
) VALUES
    (
        'service_node_timeout',
        'service_node_timeout',
        'Service node timeout',
        TRUE,
        '{"minutes_after_arrival": 20}'::jsonb,
        'medium',
        TRUE,
        'HIGH',
        '[5, 15, 30]'::jsonb
    ),
    (
        'gate_stand_conflict',
        'gate_stand_conflict',
        'Gate or stand conflict',
        TRUE,
        '{"conflict_window_minutes": 45}'::jsonb,
        'high',
        TRUE,
        'HIGH',
        '[5, 15, 30]'::jsonb
    )
ON CONFLICT (rule_id) DO NOTHING;

CREATE TABLE IF NOT EXISTS anomalies (
    anomaly_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    anomaly_type VARCHAR(64) NOT NULL,
    severity VARCHAR(16) NOT NULL DEFAULT 'medium',
    status VARCHAR(16) NOT NULL DEFAULT 'open',
    title VARCHAR(255) NOT NULL,
    description TEXT,
    detected_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TIMESTAMP WITH TIME ZONE,
    escalation_level INTEGER NOT NULL DEFAULT 0,
    last_escalated_at TIMESTAMP WITH TIME ZONE,
    linked_todo_id VARCHAR(26),
    rule_id VARCHAR(64),
    context_data JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_anomalies_flight_id FOREIGN KEY (flight_id) REFERENCES flights(flight_id) ON DELETE CASCADE,
    CONSTRAINT fk_anomalies_rule_id FOREIGN KEY (rule_id) REFERENCES anomaly_rules(rule_id) ON DELETE SET NULL,
    CONSTRAINT fk_anomalies_todo_id FOREIGN KEY (linked_todo_id) REFERENCES todos(todo_id) ON DELETE SET NULL,
    CONSTRAINT chk_anomalies_severity CHECK (severity IN ('low', 'medium', 'high', 'critical')),
    CONSTRAINT chk_anomalies_status CHECK (status IN ('open', 'acknowledged', 'resolved'))
);

CREATE INDEX IF NOT EXISTS idx_anomalies_flight_id ON anomalies(flight_id);
CREATE INDEX IF NOT EXISTS idx_anomalies_status_detected_at ON anomalies(status, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_anomalies_type_detected_at ON anomalies(anomaly_type, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_anomalies_rule_id ON anomalies(rule_id);
CREATE INDEX IF NOT EXISTS idx_anomalies_open_signature
    ON anomalies(flight_id, anomaly_type, rule_id)
    WHERE status <> 'resolved';

CREATE TABLE IF NOT EXISTS nl_query_log (
    log_id BIGSERIAL PRIMARY KEY,
    conversation_id VARCHAR(64),
    user_id VARCHAR(64),
    query_text TEXT NOT NULL,
    interpretation TEXT,
    summary TEXT,
    visualization_hint VARCHAR(32),
    duration_ms INTEGER,
    tool_calls JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_nl_query_log_user_created ON nl_query_log(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_nl_query_log_conversation ON nl_query_log(conversation_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_matviews
        WHERE schemaname = 'public' AND matviewname = 'mv_daily_flight_kpi'
    ) THEN
        EXECUTE '
            CREATE MATERIALIZED VIEW mv_daily_flight_kpi AS
            SELECT
                DATE(scheduled_departure AT TIME ZONE ''Asia/Shanghai'') AS flight_date,
                COUNT(*) AS total_flights,
                COUNT(*) FILTER (
                    WHERE actual_departure IS NOT NULL AND actual_arrival IS NOT NULL
                ) AS completed_flights,
                AVG(EXTRACT(EPOCH FROM (actual_departure - actual_arrival)) / 60)
                    FILTER (
                        WHERE actual_departure IS NOT NULL AND actual_arrival IS NOT NULL
                    ) AS avg_turnaround_minutes,
                PERCENTILE_CONT(0.9) WITHIN GROUP (
                    ORDER BY EXTRACT(EPOCH FROM (actual_departure - actual_arrival)) / 60
                ) FILTER (
                    WHERE actual_departure IS NOT NULL AND actual_arrival IS NOT NULL
                ) AS p90_turnaround_minutes,
                COUNT(*) FILTER (
                    WHERE actual_departure <= scheduled_departure + INTERVAL ''15 minutes''
                )::FLOAT
                    / NULLIF(COUNT(*) FILTER (WHERE actual_departure IS NOT NULL), 0)
                    AS on_time_departure_rate,
                COUNT(*) FILTER (
                    WHERE actual_arrival <= scheduled_arrival + INTERVAL ''15 minutes''
                )::FLOAT
                    / NULLIF(COUNT(*) FILTER (WHERE actual_arrival IS NOT NULL), 0)
                    AS on_time_arrival_rate,
                COUNT(*) FILTER (WHERE inbound_abnormal OR outbound_abnormal)::FLOAT
                    / NULLIF(COUNT(*), 0)
                    AS abnormal_ratio
            FROM flights
            WHERE scheduled_departure IS NOT NULL
            GROUP BY flight_date
        ';
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_daily_flight_kpi_date
    ON mv_daily_flight_kpi(flight_date);

