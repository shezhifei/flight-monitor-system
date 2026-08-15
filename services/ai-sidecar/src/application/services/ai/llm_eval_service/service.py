"""
E3: LLM Agent Evaluation & Observability Service

提供生产级评估门户：
- EvalJob 生命周期管理 (pending → running → completed/failed)
- Span 落库与查询 (llm_call, tool_call, checkpoint)
- 门禁指标计算 (tool accuracy, hallucination rate, etc.)
- OpenTelemetry 集成
- Grafana 仪表盘集成

设计参考：
- DeepEval agent evaluation framework
- MorphLLM production eval best practices  
- Grafana Agent Observability OTLP export
"""

import json
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Optional
from uuid import UUID, uuid4
import re
import time

from asyncpg import Connection

try:
    from opentelemetry import trace, metrics
    OTEL_AVAILABLE = True
except ImportError:
    OTEL_AVAILABLE = False
    trace = None
    metrics = None

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


# ============================================================================
# Core Data Models
# ============================================================================

@dataclass
class EvalJob:
    """评估作业定义。"""
    
    job_id: UUID = field(default_factory=uuid4)
    name: str = ""
    dataset_path: str = ""  # Path to JSONL test dataset
    description: str = ""    # ADD THIS FIELD
    
    status: str = "pending"  # pending | running | completed | failed
    progress_percent: float = 0.0
    total_runs: int = 0
    completed_runs: int = 0
    
    metrics_config: dict[str, Any] = field(default_factory=dict)
    # {
    #   "tool_accuracy_min": 0.95,        # >=95%
    #   "hallucination_rate_max": 0.05,  # <=5%
    #   "zero_violations_required": True, # Must be 0
    #   "avg_rounds_target": 8,          # <=8
    #   "plan_board_compliance_min": 0.90 # >=90%
    # }
    
    created_at: datetime = field(default_factory=datetime.utcnow)
    started_at: datetime | None = None
    completed_at: datetime | None = None
    
    error_message: str | None = None  # ADD THIS FIELD
    
    def is_active(self) -> bool:
        """Check if job is still running."""
        return self.status in ("pending", "running")
    
    def is_passed(self) -> bool:
        """Check if all gates passed."""
        return self.status == "completed"


@dataclass
class EvalSpan:
    """Span 数据结构（追踪单次运行细节）。"""
    
    span_id: UUID = field(default_factory=uuid4)
    job_id: UUID | None = None  # ADD THIS FIELD
    run_id: str = ""           # Unique run identifier
    parent_span_id: UUID | None = None
    
    span_type: str = ""        # llm_call | tool_call | checkpoint | error
    start_time: float = 0.0
    end_time: float = 0.0
    
    context: dict[str, Any] = field(default_factory=dict)
    result: dict[str, Any] = field(default_factory=dict)
    metrics: dict[str, Any] = field(default_factory=dict)
    error_message: str | None = None
    
    # Common for llm_call spans
    model_name: str = ""
    input_tokens: int = 0
    output_tokens: int = 0
    total_cost_usd: float = 0.0


@dataclass
class GateMetricsSummary:
    """门禁指标汇总数据。"""
    
    job_id: UUID
    metric_name: str
    value: float
    threshold: float
    status: str  # pass | fail | warn
    
    snapshot_at: datetime = field(default_factory=datetime.utcnow)
    details: dict[str, Any] = field(default_factory=dict)


# ============================================================================
# Evaluation Service
# ============================================================================

class EvaluationService:
    """生产级评估服务，支持在线/离线两种模式。"""
    
    _instance: "EvaluationService | None" = None
    
    def __init__(self, db_pool: Connection):
        self._db_pool = db_pool
        self._traces = trace.get_tracer(__name__) if OTEL_AVAILABLE else None
        self._metrics = metrics.get_meter(__name__) if OTEL_AVAILABLE else None
        
        # Create performance counters (only if OTel available)
        if OTEL_AVAILABLE:
            self.tool_calls_total = self._metrics.create_counter(
                "eval_tool_calls_total",
                description="Total tool calls during eval",
            )
            self.token_usage_total = self._metrics.create_counter(
                "eval_token_usage_total",
                description="Token usage split by type",
            )
    
    @classmethod
    def get_instance(cls, db_pool: Connection) -> "EvaluationService":
        """Get singleton instance."""
        if cls._instance is None:
            cls._instance = EvaluationService(db_pool)
        return cls._instance
    
    async def create_job(
        self,
        name: str,
        dataset_path: str,
        metrics_config: dict[str, Any],
        description: str = "",
    ) -> EvalJob:
        """Create new evaluation job."""
        job = EvalJob(
            name=name,
            dataset_path=dataset_path,
            description=description,
            metrics_config=metrics_config,
            status="pending",
        )
        
        # Persist to database
        await self._save_eval_job(job)
        
        logger.info(f"[Eval Service] Created job id={job.job_id}, name='{job.name}'")
        return job
    
    async def run_job(self, job: EvalJob) -> EvalJob:
        """Execute evaluation job against test dataset."""
        job.status = "running"
        job.started_at = datetime.now(timezone.utc)
        job.progress_percent = 0.0
        
        await self._update_eval_job(job)
        
        try:
            # Load test dataset (JSONL format)
            tests = await self._load_test_dataset(job.dataset_path)
            job.total_runs = len(tests)
            
            # Execute each test case
            passed_gates = []
            failed_gates = []
            
            for idx, test_case in enumerate(tests):
                job.completed_runs = idx + 1
                job.progress_percent = (idx / len(tests)) * 100
                
                span = await self._execute_single_test(job, test_case)
                
                # Evaluate against gate metrics
                gate_results = await self._evaluate_gates(job, span)
                
                for gate in gate_results.passing:
                    passed_gates.append(gate.metric_name)
                for gate in gate_results.failing:
                    failed_gates.append(gate.metric_name)
            
            # Determine final status
            if len(failed_gates) > 0:
                job.status = "failed"
                logger.warning(f"[Eval Service] Job failed: {len(failed_gates)} gates failed")
            else:
                job.status = "completed"
                logger.info(f"[Eval Service] Job completed: all {len(passed_gates)} gates passed")
            
            job.completed_at = datetime.now(timezone.utc)
            await self._update_eval_job(job)
            
            return job
            
        except Exception as e:
            job.status = "failed"
            job.error_message = str(e)
            await self._update_eval_job(job)
            raise
    
    async def _execute_single_test(
        self,
        job: EvalJob,
        test_case: dict[str, Any],
    ) -> EvalSpan:
        """Execute single test case and record span."""
        user_query = test_case["user_query"]
        expected_action = test_case["expected_action"]
        
        with self._traces.start_as_current_span("eval.test_execution") as span:
            # Execute agent against query
            result = await self._run_agent_on_query(user_query, expected_action)
            
            # Record span to Postgres
            eval_span = EvalSpan(
                run_id=f"{job.job_id}_{test_case.get('test_id', '')}",
                span_type="llm_call",
                start_time=result.get("start_time", time.time()),
                end_time=result.get("end_time", time.time()),
                context={"query": user_query},
                result=result,
                metrics={
                    "tokens_used": result.get("tokens", {}),
                    "duration_ms": result.get("duration_ms", 0),
                    "success": result.get("success", False),
                },
            )
            
            await self._persist_span(eval_span)
            
            return eval_span
    
    async def _evaluate_gates(
        self,
        job: EvalJob,
        span: EvalSpan,
    ) -> tuple[list[GateMetricsSummary], list[GateMetricsSummary]]:
        """Evaluate against configured metrics gates."""
        passing = []
        failing = []
        
        config = job.metrics_config
        
        # Tool accuracy gate
        tool_correctness = await self._calculate_tool_correctness(span.result)
        expected_accuracy = config.get("tool_accuracy_min", 0.95)
        
        gate_result = self._check_gate(
            "tool_accuracy",
            tool_correctness,
            expected_accuracy,
            "pass",
            "fail",
            config.get("gate_warning_threshold", 0.90),
        )
        
        (passing if gate_result.status == "pass" else failing).append(gate_result)
        
        # Hallucination rate gate
        hallucination_rate = await self._calculate_hallucination_rate(span.result)
        max_hallucination = config.get("hallucination_rate_max", 0.05)
        
        gate_result = self._check_gate(
            "hallucination_rate",
            hallucination_rate,
            max_hallucination,
            "pass",
            "fail",
            max_hallucination * 1.5,
        )
        
        (passing if gate_result.status == "pass" else failing).append(gate_result)
        
        return passing, failing
    
    def _check_gate(
        self,
        metric_name: str,
        actual_value: float,
        threshold: float,
        pass_status: str,
        fail_status: str,
        warning_threshold: float | None = None,
    ) -> GateMetricsSummary:
        """Check a single metric gate."""
        
        # For boolean thresholds (like zero_violations_required)
        if isinstance(threshold, bool):
            is_pass = actual_value == threshold
        else:
            # Numeric thresholds: pass if within bounds
            is_pass = actual_value <= threshold if threshold > 0 else actual_value == threshold
        
        status = pass_status if is_pass else fail_status
        
        # Add warning if between threshold and warning_threshold
        if warning_threshold:
            if is_pass and actual_value > warning_threshold:
                status = "warn"
        
        return GateMetricsSummary(
            job_id=None,  # Will be set when persisted
            metric_name=metric_name,
            value=actual_value,
            threshold=threshold,
            status=status,
        )
    
    async def _calculate_tool_correctness(self, result: dict[str, Any]) -> float:
        """Calculate tool call correctness rate."""
        called_tools = result.get("called_tools", [])
        correct_tools = sum(1 for t in called_tools if t.get("correct", False))
        
        return correct_tools / len(called_tools) if called_tools else 1.0
    
    async def _calculate_hallucination_rate(self, result: dict[str, Any]) -> float:
        """Calculate hallucination detection rate."""
        response = result.get("agent_response", "")
        flight_numbers_mentioned = result.get("extracted_flight_numbers", [])
        
        invalid_count = 0
        for fn in flight_numbers_mentioned:
            # Simple validation: should match airline code pattern (XX####)
            if not self._validate_flight_number(fn):
                invalid_count += 1
        
        total = len(flight_numbers_mentioned)
        return invalid_count / total if total > 0 else 0.0
    
    def _validate_flight_number(self, fn: str) -> bool:
        """Validate flight number format (e.g., CA1234, MU5678)."""
        pattern = r"^[A-Z]{2}\d{3,4}$"
        return bool(re.match(pattern, fn.strip().upper()))
    
    async def _persist_span(self, span: EvalSpan):
        """Persist span to PostgreSQL eval_spans table."""
        async with self._db_pool.acquire() as conn:
            await conn.execute(
                """
                INSERT INTO ai_eval_spans (
                    span_id, job_id, run_id, parent_span_id,
                    span_type, start_time, end_time, context, result, metrics,
                    model_name, input_tokens, output_tokens, total_cost_usd,
                    error_message
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                """,
                span.span_id,
                span.job_id if hasattr(span, 'job_id') else None,
                span.run_id,
                span.parent_span_id,
                span.span_type,
                span.start_time,
                span.end_time,
                json.dumps(span.context),
                json.dumps(span.result),
                json.dumps(span.metrics),
                span.model_name,
                span.input_tokens,
                span.output_tokens,
                span.total_cost_usd,
                span.error_message or "",
            )
    
    async def _get_all_gates_for_job(self, job_id: UUID) -> tuple[list[GateMetricsSummary], list[GateMetricsSummary]]:
        """Retrieve all gate results for a job from PostgreSQL."""
        passing = []
        failing = []
        
        async with self._db_pool.acquire() as conn:
            rows = await conn.fetch(
                "SELECT * FROM ai_eval_metrics_summary WHERE job_id = $1",
                job_id,
            )
            
            for row in rows:
                gate = GateMetricsSummary(
                    job_id=row['job_id'],
                    metric_name=row['metric_name'],
                    value=float(row['value']),
                    threshold=float(row['threshold']),
                    status=row['status'],
                    details=row.get('details', {}),
                    snapshot_at=row['snapshot_at'],
                )
                
                if gate.status == 'pass':
                    passing.append(gate)
                elif gate.status in ('fail', 'error'):
                    failing.append(gate)
                else:
                    passing.append(gate)  # warn treated as pass for now
        
        return passing, failing
    
    # Helper methods (stubs for testing)
    async def _load_test_dataset(self, path: str) -> list[dict[str, Any]]:
        """Load JSONL test dataset."""
        return []
    
    async def _run_agent_on_query(
        self,
        user_query: str,
        expected_action: str,
    ) -> dict[str, Any]:
        """Run agent against user query."""
        return {"success": True}
    
    async def _save_eval_job(self, job: EvalJob):
        """Save eval job to PostgreSQL ai_eval_jobs table."""
        async with self._db_pool.acquire() as conn:
            await conn.execute(
                """
                INSERT INTO ai_eval_jobs (
                    job_id, name, description, dataset_path,
                    metrics_config, status, progress_percent, total_runs, completed_runs
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                """,
                job.job_id,
                job.name,
                job.description or "",
                job.dataset_path,
                json.dumps(job.metrics_config),
                job.status,
                job.progress_percent,
                job.total_runs,
                job.completed_runs,
            )
    
    async def _update_eval_job(self, job: EvalJob):
        """Update eval job in PostgreSQL ai_eval_jobs table."""
        async with self._db_pool.acquire() as conn:
            await conn.execute(
                """
                UPDATE ai_eval_jobs
                SET status = $1, progress_percent = $2, total_runs = $3, completed_runs = $4,
                    started_at = $5, completed_at = $6, error_message = $7
                WHERE job_id = $8
                """,
                job.status,
                job.progress_percent,
                job.total_runs,
                job.completed_runs,
                job.started_at,
                job.completed_at,
                job.error_message or "",
                job.job_id,
            )


def time():
    """Remove unused time wrapper function."""
    pass