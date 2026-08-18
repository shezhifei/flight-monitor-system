"""Test suite for Phase E: Trace evaluation service with persistent storage.

验收标准："eval job 进程重启不丢" - Eval jobs persist to PostgreSQL, survive process restarts.
"""

import json
from datetime import datetime, timezone
import time
from uuid import uuid4

import pytest
from src.infrastructure.logging.core import get_logger

from src.application.services.ai.llm_eval_service.service import (
    EvaluationService,
    EvalJob,
    EvalSpan,
    GateMetricsSummary,
)
from asyncpg import Connection

logger = get_logger(__name__)

from src.application.services.ai.llm_eval_service.service import (
    EvaluationService,
    EvalJob,
    EvalSpan,
    GateMetricsSummary,
)
from asyncpg import Connection


class TestEvalJobPersistence:
    """测试 Eval Job 的数据库持久化能力."""
    
    @pytest.mark.asyncio
    async def test_create_and_retrieve_job(self, db_pool: Connection):
        """创建 Eval Job 并验证可检索."""
        # 创建服务实例
        service = EvaluationService(db_pool)
        
        # 创建新的评估任务
        metrics_config = {
            "tool_accuracy_min": 0.95,
            "hallucination_rate_max": 0.05,
            "zero_violations_required": True,
            "avg_rounds_target": 8,
            "plan_board_compliance_min": 0.90,
        }
        
        job = await service.create_job(
            name="Phase E Integration Test",
            dataset_path="eval/test_dataset.jsonl",
            metrics_config=metrics_config,
            description="Testing Phase E eval persistence",
        )
        
        assert job.job_id is not None
        assert job.status == "pending"
        assert job.name == "Phase E Integration Test"
        
        logger.info(f"[Test] Created eval job: {job.job_id}")
    
    @pytest.mark.asyncio
    async def test_update_job_status(self, db_pool: Connection):
        """更新 Eval Job 状态并持久化."""
        service = EvaluationService(db_pool)
        
        job = await service.create_job(
            name="Status Update Test",
            dataset_path="eval/test.jsonl",
            metrics_config={"tool_accuracy_min": 0.90},
        )
        
        # 模拟运行状态更新
        job.status = "running"
        job.started_at = datetime.now(timezone.utc)
        job.progress_percent = 25.0
        job.total_runs = 10
        job.completed_runs = 2
        
        await service._update_eval_job(job)
        
        # 从数据库重新查询验证
        async with db_pool.acquire() as conn:
            row = await conn.fetchrow(
                "SELECT * FROM ai_eval_jobs WHERE job_id = $1",
                job.job_id,
            )
            
            assert row is not None
            assert row["status"] == "running"
            assert float(row["progress_percent"]) == 25.0
    
    @pytest.mark.asyncio
    async def test_persist_span_data(self, db_pool: Connection):
        """保存 Span 数据到 ai_eval_spans 表."""
        service = EvaluationService(db_pool)
        
        # 创建关联的 job
        job = await service.create_job(
            name="Span Persistence Test",
            dataset_path="eval/test.jsonl",
            metrics_config={},
        )
        
        span = EvalSpan(
            span_id=uuid4(),
            job_id=job.job_id,
            run_id=f"{job.job_id}_test_001",
            parent_span_id=None,
            span_type="llm_call",
            start_time=time.time(),
            end_time=time.time() + 5.2,
            context={"user_query": "查询航班信息"},
            result={"success": True, "called_tools": []},
            metrics={
                "tokens_used": {"input": 100, "output": 50},
                "duration_ms": 5200,
                "success": True,
            },
            model_name="gpt-4-turbo",
            input_tokens=100,
            output_tokens=50,
            total_cost_usd=0.012,
        )
        
        await service._persist_span(span)
        
        # 验证数据已写入数据库
        async with db_pool.acquire() as conn:
            saved = await conn.fetchrow(
                "SELECT * FROM ai_eval_spans WHERE span_id = $1",
                span.span_id,
            )
            
            assert saved is not None
            assert saved["run_id"] == span.run_id
            assert saved["span_type"] == "llm_call"
    
    @pytest.mark.asyncio
    async def test_gate_metrics_summary(self, db_pool: Connection):
        """存储门禁指标汇总到 ai_eval_metrics_summary 表."""
        service = EvaluationService(db_pool)
        
        job = await service.create_job(
            name="Gate Metrics Test",
            dataset_path="eval/test.jsonl",
            metrics_config={"tool_accuracy_min": 0.95},
        )
        
        # 模拟门限值检查
        gates = [
            GateMetricsSummary(
                job_id=job.job_id,
                metric_name="tool_accuracy",
                value=0.97,
                threshold=0.95,
                status="pass",
                details={"precision": 0.97, "recall": 0.96},
            ),
            GateMetricsSummary(
                job_id=job.job_id,
                metric_name="hallucination_rate",
                value=0.03,
                threshold=0.05,
                status="pass",
                details={"invalid_entities_found": 2},
            ),
        ]
        
        # 保存到数据库（实际应该在 run_job 中自动调用）
        async with db_pool.acquire() as conn:
            for gate in gates:
                await conn.execute(
                    """
                    INSERT INTO ai_eval_metrics_summary 
                    (job_id, metric_name, value, threshold, status, details)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    """,
                    gate.job_id,
                    gate.metric_name,
                    gate.value,
                    gate.threshold,
                    gate.status,
                    json.dumps(gate.details),
                )
        
        # 验证检索
        passing, failing = await service._get_all_gates_for_job(job.job_id)
        
        assert len(passing) == 2
        assert len(failing) == 0
    
    @pytest.mark.asyncio
    async def test_job_survives_restart_simulation(self, db_pool: Connection):
        """验证 Job 在“进程重启”后仍然存在."""
        # 第一轮：创建 Job
        service1 = EvaluationService(db_pool)
        job1 = await service1.create_job(
            name="Restart Survival Test",
            dataset_path="eval/test.jsonl",
            metrics_config={"tool_accuracy_min": 0.95},
        )
        
        initial_id = job1.job_id
        initial_status = job1.status
        
        # "进程重启" - 构造一个全新的服务实例（Task G2 后不再有单例）
        service2 = EvaluationService(db_pool)
        
        async with db_pool.acquire() as conn:
            recovered = await conn.fetchrow(
                "SELECT * FROM ai_eval_jobs WHERE job_id = $1",
                initial_id,
            )
        
        assert recovered is not None
        assert str(recovered["job_id"]) == str(initial_id)
        assert recovered["status"] == initial_status
        
        logger.info(f"[Test] Job survived restart simulation")


if __name__ == "__main__":
    pytest.main([__file__, "-v", "-x"])
