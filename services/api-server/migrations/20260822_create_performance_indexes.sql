-- 性能优化 - PostgreSQL 索引增强
-- Migration: 20260822_create_performance_indexes.sql
-- Created: August 22, 2026
-- Purpose: 为 Top 高频查询表创建优化索引

-- 启用 pg_stat_statements 扩展（如果未启用）
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- ============================================================================
-- ai_runs 表优化
-- ============================================================================

-- idx_ai_runs_job_id_created_at: 加速按 job_id 分组和按时间倒序查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_runs_job_id_created_at 
    ON ai_runs(job_id, created_at DESC);

-- idx_ai_runs_status_created: 加速按状态过滤的查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_runs_status_created 
    ON ai_runs(status, created_at) WHERE status IN ('pending', 'running', 'completed');

-- ============================================================================
-- dispatch_orders 表优化
-- ============================================================================

-- idx_dispatch_orders_released_status_deleted: 加速调度订单状态查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_dispatch_orders_released_status_deleted 
    ON dispatch_orders(released_at, status) WHERE deleted_at IS NULL;

-- idx_dispatch_orders_flight_id_status: 按航班号筛选活跃订单
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_dispatch_orders_flight_id_status 
    ON dispatch_orders(flight_id, status) WHERE deleted_at IS NULL;

-- ============================================================================
-- business_cases 表优化
-- ============================================================================

-- idx_business_cases_status_created: 加速业务案例状态筛选
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_business_cases_status_created 
    ON business_cases(status, created_at);

-- idx_business_cases_creator_created: 按创建者统计案例数
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_business_cases_creator_created 
    ON business_cases(creator_id, created_at DESC);

-- ============================================================================
-- ai_copilot_drafts 向量索引 (如果使用 pgvector)
-- ============================================================================

-- 如果是高维向量搜索场景，创建 HNSW 索引替代 IVFFlat
-- 需要先确保已安装 pgvector 扩展
CREATE EXTENSION IF NOT EXISTS vector;

-- idx_ai_copilot_embedding_vector_hnsw: HNSW ANN 索引用于相似度搜索
-- m=16 表示每个节点最多 16 个邻居，ef_construction=200 建索引时的参数
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_copilot_embedding_vector_hnsw 
    ON ai_copilot_drafts USING hnsw(embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 200);

-- ============================================================================
-- dispatch_order_adjustments 表优化
-- ============================================================================

-- idx_order_adjustments_order_updated: 按订单 ID 查找变更历史
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_order_adjustments_order_updated 
    ON dispatch_order_adjustments(dispatch_order_id, updated_at DESC);

-- ============================================================================
-- 统计信息更新
-- ============================================================================

-- 对所有主要表的统计信息进行更新，帮助查询规划器选择最优执行计划
ANALYZE ai_runs;
ANALYZE dispatch_orders;
ANALYZE business_cases;
ANALYZE ai_copilot_drafts;
ANALYZE dispatch_order_adjustments;

-- ============================================================================
-- 验证索引创建成功
-- ============================================================================

-- 检查索引是否存在
SELECT indexname, tablename 
FROM pg_indexes 
WHERE schemaname = 'public' 
  AND indexname LIKE 'idx_%performance%'
ORDER BY tablename, indexname;
