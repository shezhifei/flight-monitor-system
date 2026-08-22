-- Shadow Mode performance comparison store used by ENABLE_SHADOW_MODE.
-- Vector ANN index on ai_copilot_drafts.embedding is skipped: that column is
-- not present in root migrations (embeddings live elsewhere as JSONB).

BEGIN;

CREATE TABLE IF NOT EXISTS shadow_mode_performance_metrics (
    test_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    query_type VARCHAR(50) NOT NULL,
    old_latency_ms INTEGER NOT NULL,
    new_latency_ms INTEGER NOT NULL,
    accuracy_diff DOUBLE PRECISION,
    operator_feedback TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT shadow_mode_performance_metrics_latency_nonneg
        CHECK (old_latency_ms >= 0 AND new_latency_ms >= 0)
);

CREATE INDEX IF NOT EXISTS idx_spm_query_type
    ON shadow_mode_performance_metrics (query_type);

CREATE INDEX IF NOT EXISTS idx_spm_created_at
    ON shadow_mode_performance_metrics (created_at DESC);

CREATE OR REPLACE VIEW v_spm_summary_by_query_type AS
SELECT
    query_type,
    COUNT(*)::bigint AS total_samples,
    ROUND(AVG(old_latency_ms)::numeric, 2) AS avg_old_latency_ms,
    ROUND(AVG(new_latency_ms)::numeric, 2) AS avg_new_latency_ms,
    ROUND(
        CASE
            WHEN AVG(old_latency_ms) > 0
                THEN ((AVG(old_latency_ms) - AVG(new_latency_ms)) / AVG(old_latency_ms) * 100.0)
            ELSE NULL
        END::numeric,
        2
    ) AS avg_improvement_percent,
    ROUND(AVG(accuracy_diff)::numeric, 3) AS min_accuracy,
    ROUND(AVG(accuracy_diff)::numeric, 3) AS max_completeness
FROM shadow_mode_performance_metrics
GROUP BY query_type;

CREATE OR REPLACE VIEW v_spm_high_impact_improvements AS
SELECT
    test_id,
    query_type,
    old_latency_ms,
    new_latency_ms,
    ROUND(
        CASE
            WHEN old_latency_ms > 0
                THEN ((old_latency_ms - new_latency_ms)::numeric / old_latency_ms * 100.0)
            ELSE NULL
        END,
        2
    ) AS improvement_percent,
    accuracy_diff AS accuracy_score,
    'baseline'::text AS old_implementation_name,
    'optimized'::text AS new_implementation_name,
    operator_feedback,
    NULL::text AS validated_by,
    created_at
FROM shadow_mode_performance_metrics
WHERE old_latency_ms > 0
  AND ((old_latency_ms - new_latency_ms)::numeric / old_latency_ms * 100.0) > 10
ORDER BY improvement_percent DESC NULLS LAST
LIMIT 100;

COMMIT;
