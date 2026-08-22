-- Shadow Mode 验证框架 - 性能对比追踪表
-- Migration: 20260822_create_shadow_performance_metrics.sql
-- Created: August 22, 2026
-- Purpose: 用于记录新旧实现的性能差异，支持 A/B 测试决策

-- ============================================================
-- Shadow Mode Performance Metrics Table
-- ============================================================

CREATE TABLE IF NOT EXISTS shadow_mode_performance_metrics (
    -- Primary Key
    id BIGSERIAL PRIMARY KEY,
    
    -- Test Identification
    test_id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    test_run_id VARCHAR(100) NOT NULL,  -- 关联具体的测试运行批次
    
    -- Query/Operation Type Classification
    query_type VARCHAR(50) NOT NULL,  -- e.g., 'redis_get_batch', 'ai_copilot_response', 'dispatch_order_query'
    
    -- Implementation Versions
    old_implementation_name VARCHAR(50) DEFAULT 'baseline',
    new_implementation_name VARCHAR(50) DEFAULT 'optimized',
    
    -- Performance Measurements (milliseconds)
    old_latency_ms INTEGER NOT NULL,      -- 基线实现的延迟
    new_latency_ms INTEGER NOT NULL,      -- 优化实现的延迟
    latency_reduction_percent FLOAT,      -- 延迟改善百分比
    
    -- Accuracy & Completeness Scores (for functional validation)
    accuracy_score FLOAT,                 -- 结果准确性得分 (0-1)
    completeness_score FLOAT,             -- 数据完整性得分 (0-1)
    discrepancy_detected BOOLEAN DEFAULT false,
    
    -- Operational Metadata
    sample_size INTEGER DEFAULT 1,        -- 样本数量（如批量操作的条目数）
    memory_delta_kb INTEGER,              -- 内存变化量（新 - 旧）
    cpu_usage_delta_percent FLOAT,        -- CPU 使用率变化
    
    -- Human Feedback (可选：人工复核结果)
    operator_feedback TEXT,               -- 操作员备注
    validated_by VARCHAR(100),            -- 验证人 ID
    
    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    
    -- Constraint: Ensure latency is non-negative
    CONSTRAINT valid_latency CHECK (old_latency_ms >= 0 AND new_latency_ms >= 0),
    CONSTRAINT valid_scores CHECK (accuracy_score IS NULL OR (accuracy_score >= 0 AND accuracy_score <= 1)),
    CONSTRAINT valid_scores_check CHECK (completeness_score IS NULL OR (completeness_score >= 0 AND completeness_score <= 1))
);

-- ============================================================
-- Indexes for Performance Analysis Queries
-- ============================================================

-- 按查询类型聚合统计
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_spm_query_type 
    ON shadow_mode_performance_metrics(query_type);

-- 按测试运行 ID 分组
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_spm_test_run_id 
    ON shadow_mode_performance_metrics(test_run_id);

-- 按时间范围分析趋势
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_spm_created_at 
    ON shadow_mode_performance_metrics(created_at DESC);

-- 联合索引：快速筛选有效样本
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_spm_query_type_validated 
    ON shadow_mode_performance_metrics(query_type, operator_feedback IS NOT NULL);

-- ============================================================
-- Comments for Documentation
-- ============================================================

COMMENT ON TABLE shadow_mode_performance_metrics IS 'Shadow Mode 性能对比追踪表';
COMMENT ON COLUMN shadow_mode_performance_metrics.test_id IS '唯一测试标识符';
COMMENT ON COLUMN shadow_mode_performance_metrics.query_type IS '查询/操作类型分类';
COMMENT ON COLUMN shadow_mode_performance_metrics.old_latency_ms IS '基线实现延迟（毫秒）';
COMMENT ON COLUMN shadow_mode_performance_metrics.new_latency_ms IS '优化实现延迟（毫秒）';
COMMENT ON COLUMN shadow_mode_performance_metrics.latency_reduction_percent IS '延迟改善百分比 = (1 - new/old) * 100';
COMMENT ON COLUMN shadow_mode_performance_metrics.accuracy_score IS '功能准确性验证得分（0-1）';
COMMENT ON COLUMN shadow_mode_performance_metrics.completeness_score IS '数据完整性验证得分（0-1）';
COMMENT ON COLUMN shadow_mode_performance_metrics.discrepancy_detected IS '是否检测到结果不一致';

-- ============================================================
-- Helper Functions
-- ============================================================

-- 计算延迟改善百分比（自动触发）
CREATE OR REPLACE FUNCTION calculate_latency_improvement()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.old_latency_ms > 0 THEN
        NEW.latency_reduction_percent := (1.0 - (NEW.new_latency_ms::float / NEW.old_latency_ms::float)) * 100.0;
    ELSE
        NEW.latency_reduction_percent := NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 自动计算改善百分比的触发器
DROP TRIGGER IF EXISTS trg_calculate_latency_improvement ON shadow_mode_performance_metrics;
CREATE TRIGGER trg_calculate_latency_improvement
    BEFORE INSERT OR UPDATE ON shadow_mode_performance_metrics
    FOR EACH ROW EXECUTE FUNCTION calculate_latency_improvement();

-- 更新时间戳
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_update_timestamp ON shadow_mode_performance_metrics;
CREATE TRIGGER trg_update_timestamp
    BEFORE UPDATE ON shadow_mode_performance_metrics
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================
-- View: Summary Statistics by Query Type
-- ============================================================

CREATE OR REPLACE VIEW v_spm_summary_by_query_type AS
SELECT
    query_type,
    COUNT(*) as total_samples,
    ROUND(AVG(old_latency_ms)::numeric, 2) as avg_old_latency_ms,
    ROUND(AVG(new_latency_ms)::numeric, 2) as avg_new_latency_ms,
    ROUND(AVG(latency_reduction_percent)::numeric, 2) as avg_improvement_percent,
    ROUND(MIN(accuracy_score)::numeric, 3) as min_accuracy,
    ROUND(MAX(completeness_score)::numeric, 3) as max_completeness,
    SUM(CASE WHEN discrepancy_detected THEN 1 ELSE 0 END) as discrepancy_count,
    MAX(created_at) as last_recorded_at
FROM shadow_mode_performance_metrics
GROUP BY query_type
ORDER BY query_type;

-- ============================================================
-- View: Recent High-Impact Improvements (>10% improvement)
-- ============================================================

CREATE OR REPLACE VIEW v_spm_high_impact_improvements AS
SELECT
    test_id,
    query_type,
    old_latency_ms,
    new_latency_ms,
    ROUND(latency_reduction_percent::numeric, 2) as improvement_percent,
    accuracy_score,
    completed_at,
    validated_by
FROM shadow_mode_performance_metrics
WHERE latency_reduction_percent > 10
  AND operator_feedback IS NOT NULL
ORDER BY latency_reduction_percent DESC
LIMIT 100;

-- ============================================================
-- Sample Data Insertion (Example Usage)
-- ============================================================

-- 插入一条示例记录（实际使用时由应用程序自动生成）
INSERT INTO shadow_mode_performance_metrics (
    test_run_id,
    query_type,
    old_implementation_name,
    new_implementation_name,
    old_latency_ms,
    new_latency_ms,
    accuracy_score,
    completeness_score,
    sample_size,
    operator_feedback
) VALUES (
    'test-run-20260822-001',
    'redis_get_batch',
    'baseline_single_get',
    'optimized_pipeline_get',
    15,
    8,
    1.0,
    1.0,
    10,
    'Pipeline batch GET vs single GET: significant improvement observed'
);
