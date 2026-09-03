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
import re
import time
from contextlib import nullcontext
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Protocol
from uuid import UUID, uuid4

from asyncpg import Connection

try:
    from opentelemetry import metrics, trace

    OTEL_AVAILABLE = True
except ImportError:
    OTEL_AVAILABLE = False
    trace = None
    metrics = None

from src.infrastructure.ai.working_memory import EVIDENCE_FILE, PLAN_STATE_KEY
from src.infrastructure.logging.core import get_logger

from .gates import (
    DEFAULT_ROUND_CAP,
    HARD_ROUND_CAPS,
    sample_tool_compliance,
    sample_ungrounded_rate,
)

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
    description: str = ""  # ADD THIS FIELD

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
    run_id: str = ""  # Unique run identifier
    parent_span_id: UUID | None = None

    span_type: str = ""  # llm_call | tool_call | checkpoint | error
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
# Agent runner protocol (Task G2)
# ============================================================================


class EvalRunnerUnavailableError(RuntimeError):
    """Raised when an eval run is requested without a configured runner.

    Fail-closed by design: the service must never fabricate a successful
    result when it cannot actually execute the agent.
    """


@dataclass
class EvalRunResult:
    """Structured outcome of a single agent run (Task G2 runner protocol)."""

    success: bool
    agent_response: str
    called_tools: list[str]
    evidence_object_ids: list[str]
    extracted_ids: list[str]  # flight numbers / flight ids / order ids in the answer
    total_tool_rounds: int
    plan_present: bool
    unauthorized_attempts: int
    tokens: dict[str, int]
    duration_ms: int

    def to_dict(self) -> dict[str, Any]:
        return {
            "success": self.success,
            "agent_response": self.agent_response,
            "called_tools": list(self.called_tools),
            "evidence_object_ids": list(self.evidence_object_ids),
            "extracted_ids": list(self.extracted_ids),
            "total_tool_rounds": self.total_tool_rounds,
            "plan_present": self.plan_present,
            "unauthorized_attempts": self.unauthorized_attempts,
            "tokens": dict(self.tokens),
            "duration_ms": self.duration_ms,
        }


class EvalAgentRunner(Protocol):
    """Executes one agent run and reports structured evidence (Task G2)."""

    async def run(self, *, user_query: str, task_type: str, entity_id: str) -> EvalRunResult: ...


# Lightweight ID extraction for the default runner: flight numbers and the
# prefixed object ids used across the ontology tool face. Lookarounds instead
# of ``\b`` because Chinese text counts as word characters in Unicode regex.
_ID_PATTERNS: tuple[re.Pattern[str], ...] = (
    re.compile(r"(?<![A-Za-z0-9])[A-Z]{2}\d{3,4}(?![0-9])"),
    re.compile(r"(?<![A-Za-z0-9_-])(?:flight|stand|dispatch|anomaly|order)-[A-Za-z0-9][A-Za-z0-9_-]*"),
)


def extract_answer_ids(answer: str) -> list[str]:
    ids: list[str] = []
    for pattern in _ID_PATTERNS:
        ids.extend(pattern.findall(answer or ""))
    return sorted(set(ids))


class RuntimeServiceEvalRunner:
    """Default production runner over ``RuntimeService.stream_run_with_tools``.

    Streams one run and folds the SSE events into an :class:`EvalRunResult`:
    tool.call names, terminal answer/evidence/token usage, and plan-board
    presence. Injected into :class:`EvaluationService`; unit tests use fakes.
    """

    def __init__(self, runtime_service: Any):
        self._runtime_service = runtime_service

    async def run(self, *, user_query: str, task_type: str, entity_id: str) -> EvalRunResult:
        from src.infrastructure.ai.context_envelope import (
            ContextEnvelope,
            EnvelopeContext,
            EnvelopeOntology,
            EnvelopeRequester,
            EnvelopeTask,
        )

        started = time.monotonic()
        envelope = ContextEnvelope(
            run_id=f"eval-{uuid4()}",
            requester=EnvelopeRequester(user_id="eval-harness", roles=["eval"]),
            ontology=EnvelopeOntology(),
            context=EnvelopeContext(),
            task=EnvelopeTask(task_type=task_type, user_message=user_query),
        )

        called_tools: list[str] = []
        tool_rounds = 0
        success = False
        answer = ""
        evidence_ids: list[str] = []
        tokens: dict[str, int] = {}

        async for event in self._runtime_service.stream_run_with_tools(envelope):
            event_type = event.get("event", "")
            data = event.get("data") or {}
            if event_type == "tool.call":
                tool_name = data.get("tool_name") or ""
                if tool_name:
                    called_tools.append(tool_name)
            elif event_type == "tool.result":
                tool_rounds += 1
            elif event_type == "run.complete":
                success = True
                answer = str(data.get("answer") or "")
                evidence_ids = [
                    entry.get("object_id")
                    for entry in (data.get("evidence") or [])
                    if isinstance(entry, dict) and entry.get("object_id")
                ]
                usage = data.get("token_usage") or {}
                tokens = {
                    key: int(value)
                    for key, value in usage.items()
                    if isinstance(value, int) or (isinstance(value, float) and value.is_integer())
                }
            elif event_type == "run.fail":
                success = False
                answer = str(data.get("answer") or "")

        return EvalRunResult(
            success=success,
            agent_response=answer,
            called_tools=called_tools,
            evidence_object_ids=evidence_ids,
            extracted_ids=extract_answer_ids(answer),
            total_tool_rounds=tool_rounds,
            plan_present="update_plan" in called_tools,
            unauthorized_attempts=0,
            tokens=tokens,
            duration_ms=int((time.monotonic() - started) * 1000),
        )


# ============================================================================
# Ledger sampling (Task G4)
# ============================================================================


def _checkpoint_snapshot(row: dict[str, Any]) -> dict[str, Any]:
    """Decode a checkpoint row's snapshot (dict or JSONB string)."""
    snapshot = row.get("snapshot")
    if isinstance(snapshot, str):
        try:
            snapshot = json.loads(snapshot)
        except ValueError:
            snapshot = {}
    return snapshot if isinstance(snapshot, dict) else {}


def build_eval_result_from_checkpoints(rows: list[dict[str, Any]]) -> EvalRunResult:
    """Fold ``ai_run_checkpoints`` rows of one run into an :class:`EvalRunResult`.

    Task G4: production trajectories are sampled from the control-plane ledger
    instead of building a second trace pipeline. ``after_tool`` snapshots
    carry the executed tool names and the working-memory ``evidence.json``
    object ids; ``after_completion`` marks a finished run and carries the
    final answer. Pure function — locked by test_eval_ingest_from_checkpoint.
    """
    called_tools: list[str] = []
    evidence_ids: list[str] = []
    seen_evidence: set[str] = set()
    tool_rounds = 0
    unauthorized = 0
    plan_present = False
    completed = False
    answer = ""
    timestamps: list[float] = []

    for row in rows:
        checkpoint_type = str(row.get("checkpoint_type") or "")
        snapshot = _checkpoint_snapshot(row)

        ts = snapshot.get("timestamp")
        if isinstance(ts, (int, float)):
            timestamps.append(float(ts))

        if checkpoint_type == "after_tool":
            tool_rounds += 1
            for entry in snapshot.get("results") or []:
                if not isinstance(entry, dict):
                    continue
                tool_name = str(entry.get("tool_name") or "")
                if tool_name:
                    called_tools.append(tool_name)
                if entry.get("blocked_by"):
                    unauthorized += 1

        memory = snapshot.get("working_memory")
        if isinstance(memory, dict):
            for record in memory.get(EVIDENCE_FILE) or []:
                object_id = str(record.get("object_id") or "") if isinstance(record, dict) else ""
                if object_id and object_id not in seen_evidence:
                    seen_evidence.add(object_id)
                    evidence_ids.append(object_id)
            if memory.get(PLAN_STATE_KEY):
                plan_present = True

        if checkpoint_type == "after_completion":
            completed = True
            final_result = snapshot.get("final_result") or {}
            answer = str(final_result.get("text") or "")

    duration_ms = int((max(timestamps) - min(timestamps)) * 1000) if len(timestamps) >= 2 else 0
    return EvalRunResult(
        success=completed,
        agent_response=answer,
        called_tools=called_tools,
        evidence_object_ids=evidence_ids,
        extracted_ids=extract_answer_ids(answer),
        total_tool_rounds=tool_rounds,
        plan_present=plan_present or "update_plan" in called_tools,
        unauthorized_attempts=unauthorized,
        tokens={},
        duration_ms=duration_ms,
    )


# ============================================================================
# Evaluation Service
# ============================================================================


class EvaluationService:
    """生产级评估服务，支持在线/离线两种模式。

    Task G2: the singleton ``get_instance`` is gone — callers construct the
    service with a database pool and an injected :class:`EvalAgentRunner`.
    Without a runner the service fails closed instead of faking success.
    """

    def __init__(self, db_pool: Connection, agent_runner: EvalAgentRunner | None = None):
        self._db_pool = db_pool
        self._agent_runner = agent_runner
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
        job.started_at = datetime.now(UTC)
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

                # Evaluate against gate metrics and persist results
                gate_summary = await self._evaluate_gates(job, span)

                # Check if overall result passed or failed
                if gate_summary.status == "fail":
                    failed_gates.append(gate_summary.metric_name)
                    logger.warning(f"[Eval Service] Gate failed at run {job.completed_runs}/{job.total_runs}")
                else:
                    passed_gates.append(gate_summary.metric_name)

            # Determine final status
            if len(failed_gates) > 0:
                job.status = "failed"
                logger.warning(f"[Eval Service] Job failed: {len(failed_gates)} gates failed")
            else:
                job.status = "completed"
                logger.info(f"[Eval Service] Job completed: all {len(passed_gates)} gates passed")

            job.completed_at = datetime.now(UTC)
            await self._update_eval_job(job)

            return job

        except Exception as e:
            # Eval job boundary (K5/W2-5): mark the job failed with a structured
            # error code, then re-raise so the caller observes the failure.
            logger.error(
                "eval_job_execution_failed",
                extra={
                    "error_code": "EVAL_JOB_EXECUTION_FAILED",
                    "job_id": job.job_id,
                    "completed_runs": job.completed_runs,
                    "total_runs": job.total_runs,
                },
                exc_info=e,
            )
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
        task_type = test_case.get("task_type", "query_ops")
        entity_id = test_case.get("entity_id", "default")

        with self._traces.start_as_current_span("eval.test_execution") if self._traces is not None else nullcontext():
            # Execute agent against query through the injected runner.
            run_result = await self._run_agent_on_query(
                user_query=user_query,
                task_type=task_type,
                entity_id=entity_id,
            )
            result = run_result.to_dict()
            started = time.time()

            # Record span to Postgres
            eval_span = EvalSpan(
                run_id=f"{job.job_id}_{test_case.get('id', '')}",
                span_type="llm_call",
                start_time=started - run_result.duration_ms / 1000,
                end_time=started,
                context={
                    "query": user_query,
                    "task_type": task_type,
                    "entity_id": entity_id,
                    "expected": test_case.get("expected") or {},
                },
                result=result,
                metrics={
                    "tokens_used": run_result.tokens,
                    "duration_ms": run_result.duration_ms,
                    "success": run_result.success,
                    "total_tool_rounds": run_result.total_tool_rounds,
                    "plan_present": run_result.plan_present,
                    "plan_required": bool((test_case.get("expected") or {}).get("plan_required", False)),
                    "constraint_violations": run_result.unauthorized_attempts,
                },
            )

            await self._persist_span(eval_span)

            return eval_span

    async def _evaluate_gates(
        self,
        job: EvalJob,
        span: EvalSpan,
    ) -> GateMetricsSummary:
        """Evaluate evidence-coverage gates (Task G3) for one sample span."""
        config = job.metrics_config
        context = span.context or {}
        expected = context.get("expected") or {}
        task_type = context.get("task_type") or "query_ops"
        passing = []
        failing = []

        # 1. Tool policy gate: every called tool allowed, none forbidden.
        tool_correctness = await self._calculate_tool_correctness(span.result, expected)
        expected_accuracy = config.get("tool_accuracy_min", 0.95)

        gate_result = self._check_gate_for_minimum(
            "tool_accuracy",
            tool_correctness,
            expected_accuracy,
            job.job_id,
        )
        passing.append(gate_result) if gate_result.status == "pass" else failing.append(gate_result)

        # 2. Ungrounded id gate (was hallucination_rate): extracted answer ids
        # must be backed by tool evidence — never a flight-number regex.
        ungrounded_rate = await self._calculate_hallucination_rate(span.result)
        max_ungrounded = config.get("ungrounded_id_rate_max", config.get("hallucination_rate_max", 0.05))

        gate_result = self._check_gate_for_maximum(
            "ungrounded_id_rate",
            ungrounded_rate,
            max_ungrounded,
            job.job_id,
        )
        passing.append(gate_result) if gate_result.status == "pass" else failing.append(gate_result)

        # 3. Zero violations required (boolean check)
        violations = int(span.result.get("unauthorized_attempts", 0) or span.metrics.get("constraint_violations", 0))
        zero_violations_required = config.get("zero_violations_required", True)

        if zero_violations_required:
            is_pass = violations == 0
            status = "pass" if is_pass else "fail"
            gate_result = GateMetricsSummary(
                job_id=job.job_id,
                metric_name="zero_violations",
                value=float(violations),
                threshold=0.0,
                status=status,
                details={"actual_violations": violations},
            )
            passing.append(gate_result) if is_pass else failing.append(gate_result)

        # 4. Average rounds target (should be <= template hard cap)
        total_rounds = span.metrics.get("total_tool_rounds", 0)
        avg_rounds_target = config.get("avg_rounds_target", HARD_ROUND_CAPS.get(task_type, DEFAULT_ROUND_CAP))

        gate_result = self._check_gate_for_maximum(
            "avg_rounds",
            float(total_rounds),
            avg_rounds_target,
            job.job_id,
        )
        passing.append(gate_result) if gate_result.status == "pass" else failing.append(gate_result)

        # 5. Plan board compliance: plan_required samples must carry a plan.
        if expected.get("plan_required", False):
            plan_present = bool(span.metrics.get("plan_present", False))
            gate_result = self._check_gate_for_minimum(
                "plan_board_compliance",
                1.0 if plan_present else 0.0,
                config.get("plan_board_compliance_min", 0.90),
                job.job_id,
            )
            passing.append(gate_result) if gate_result.status == "pass" else failing.append(gate_result)

        # Persist all gates to database
        for gate in passing + failing:
            await self._persist_gate_metric(gate)

        # Return aggregated summary
        summary = GateMetricsSummary(
            job_id=job.job_id,
            metric_name="overall_result",
            value=float(len(passing)) / float(max(len(passing) + len(failing), 1)),
            threshold=0.8,  # 80% gates must pass
            status="pass" if not failing else "fail",
            details={
                "passing_count": len(passing),
                "failing_count": len(failing),
                "passing_metrics": [g.metric_name for g in passing],
                "failing_metrics": [g.metric_name for g in failing],
            },
        )

        logger.info(f"[Eval Service] Gates evaluated: {len(passing)} passing, {len(failing)} failing")

        return summary

    def _check_gate_for_minimum(
        self,
        metric_name: str,
        actual_value: float,
        min_threshold: float,
        job_id: UUID | None = None,
    ) -> GateMetricsSummary:
        """Check minimum threshold gate (value should be >= threshold)."""
        is_pass = actual_value >= min_threshold
        status = "pass" if is_pass else "fail"

        return GateMetricsSummary(
            job_id=job_id,
            metric_name=metric_name,
            value=actual_value,
            threshold=min_threshold,
            status=status,
            details={"direction": "minimum_required"},
        )

    def _check_gate_for_maximum(
        self,
        metric_name: str,
        actual_value: float,
        max_threshold: float,
        job_id: UUID | None = None,
    ) -> GateMetricsSummary:
        """Check maximum threshold gate (value should be <= threshold)."""
        is_pass = actual_value <= max_threshold
        status = "pass" if is_pass else "fail"

        return GateMetricsSummary(
            job_id=job_id,
            metric_name=metric_name,
            value=actual_value,
            threshold=max_threshold,
            status=status,
            details={"direction": "maximum_allowed"},
        )

    async def _calculate_tool_correctness(
        self, result: dict[str, Any], expected: dict[str, Any] | None = None
    ) -> float:
        """Tool policy compliance (Task G3): every called tool must be in the
        sample's ``allowed_tools`` and none in ``forbidden_tools``."""
        called_tools = result.get("called_tools", [])
        if expected is None:
            # Legacy span rows without expectations cannot be policy-scored.
            return 1.0
        return sample_tool_compliance(
            [tool if isinstance(tool, str) else tool.get("called", "") for tool in called_tools],
            expected.get("allowed_tools", []) or [],
            expected.get("forbidden_tools", []) or [],
        )

    async def _calculate_hallucination_rate(self, result: dict[str, Any]) -> float:
        """Ungrounded id rate (Task G3): answer ids without evidence backing."""
        extracted_ids = result.get("extracted_ids", [])
        if extracted_ids:
            return sample_ungrounded_rate(extracted_ids, result.get("evidence_object_ids", []) or [])

        # Legacy pre-G2 span rows: only the format check remains, kept as an
        # extraction aid — it is no longer the gate's main path.
        flight_numbers_mentioned = result.get("extracted_flight_numbers", [])
        invalid_count = 0
        for fn in flight_numbers_mentioned:
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
                span.job_id if hasattr(span, "job_id") else None,
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
                    job_id=row["job_id"],
                    metric_name=row["metric_name"],
                    value=float(row["value"]),
                    threshold=float(row["threshold"]),
                    status=row["status"],
                    details=row.get("details", {}),
                    snapshot_at=row["snapshot_at"],
                )

                if gate.status == "pass":
                    passing.append(gate)
                elif gate.status in ("fail", "error"):
                    failing.append(gate)
                else:
                    passing.append(gate)  # warn treated as pass for now

        return passing, failing

    async def _persist_gate_metric(self, gate: GateMetricsSummary):
        """Persist gate metric to PostgreSQL table."""
        async with self._db_pool.acquire() as conn:
            await conn.execute(
                """
                INSERT INTO ai_eval_metrics_summary (
                    job_id, metric_name, value, threshold,
                    status, details, snapshot_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                """,
                gate.job_id,
                gate.metric_name,
                gate.value,
                gate.threshold,
                gate.status,
                json.dumps(gate.details),
                gate.snapshot_at,
            )

    # Dataset loading and agent execution (Task G2)
    async def _load_test_dataset(self, path: str) -> list[dict[str, Any]]:
        """Load a JSONL eval dataset (one sample per non-comment line)."""
        dataset_path = Path(path)
        if not dataset_path.is_file():
            raise FileNotFoundError(f"Eval dataset not found: {path}")
        samples: list[dict[str, Any]] = []
        for raw_line in dataset_path.read_text(encoding="utf-8").splitlines():
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue
            samples.append(json.loads(line))
        return samples

    async def _run_agent_on_query(
        self,
        *,
        user_query: str,
        task_type: str,
        entity_id: str,
    ) -> EvalRunResult:
        """Run the agent through the injected runner (Task G2).

        Fail-closed: without a runner the eval raises instead of returning a
        fabricated ``{"success": True}``.
        """
        if self._agent_runner is None:
            raise EvalRunnerUnavailableError(
                "EvaluationService has no agent runner configured; refusing to fabricate a successful eval result"
            )
        return await self._agent_runner.run(
            user_query=user_query,
            task_type=task_type,
            entity_id=entity_id,
        )

    async def ingest_run_from_ledger(self, run_id: str) -> EvalRunResult:
        """Sample a production run from ``ai_run_checkpoints`` (Task G4).

        The sidecar already reads/writes the ``ai_*`` control-plane tables,
        so no cross-language round trip is needed. Fail-closed: a run without
        checkpoints raises instead of yielding a fabricated sample.
        """
        async with self._db_pool.acquire() as conn:
            rows = await conn.fetch(
                "SELECT checkpoint_type, snapshot FROM ai_run_checkpoints WHERE run_id = $1 ORDER BY sequence_no",
                run_id,
            )
        if not rows:
            raise LookupError(f"No checkpoints found for run {run_id}; refusing to fabricate an eval sample")
        return build_eval_result_from_checkpoints([dict(row) for row in rows])

    # ------------------------------------------------------------------
    # Job queries for the Eval Lab HTTP surface (Task G5)
    # ------------------------------------------------------------------

    @staticmethod
    def _job_row_to_dict(row: dict[str, Any]) -> dict[str, Any]:
        """Normalize an ``ai_eval_jobs`` row for JSON responses."""
        metrics_config = row.get("metrics_config")
        if isinstance(metrics_config, str):
            try:
                metrics_config = json.loads(metrics_config)
            except ValueError:
                metrics_config = {}

        def _iso(value: Any) -> str | None:
            return value.isoformat() if hasattr(value, "isoformat") else value

        return {
            "job_id": str(row.get("job_id") or ""),
            "name": str(row.get("name") or ""),
            "description": str(row.get("description") or ""),
            "dataset_path": str(row.get("dataset_path") or ""),
            "status": str(row.get("status") or ""),
            "progress_percent": float(row.get("progress_percent") or 0.0),
            "total_runs": int(row.get("total_runs") or 0),
            "completed_runs": int(row.get("completed_runs") or 0),
            "metrics_config": metrics_config if isinstance(metrics_config, dict) else {},
            "created_at": _iso(row.get("created_at")),
            "started_at": _iso(row.get("started_at")),
            "completed_at": _iso(row.get("completed_at")),
            "error_message": str(row.get("error_message") or "") or None,
        }

    async def list_jobs(self, limit: int = 30) -> list[dict[str, Any]]:
        """List eval jobs, newest first (Task G5)."""
        capped = max(1, min(int(limit or 30), 200))
        async with self._db_pool.acquire() as conn:
            rows = await conn.fetch(
                "SELECT job_id, name, description, dataset_path, status, "
                "progress_percent, total_runs, completed_runs, metrics_config, "
                "created_at, started_at, completed_at, error_message "
                "FROM ai_eval_jobs ORDER BY created_at DESC LIMIT $1",
                capped,
            )
        return [self._job_row_to_dict(dict(row)) for row in rows]

    async def get_job_detail(self, job_id: UUID) -> dict[str, Any] | None:
        """Fetch one job plus its gate table (Task G5)."""
        async with self._db_pool.acquire() as conn:
            row = await conn.fetchrow(
                "SELECT job_id, name, description, dataset_path, status, "
                "progress_percent, total_runs, completed_runs, metrics_config, "
                "created_at, started_at, completed_at, error_message "
                "FROM ai_eval_jobs WHERE job_id = $1",
                job_id,
            )
            if row is None:
                return None
            gate_rows = await conn.fetch(
                "SELECT metric_name, value, threshold, status, details, snapshot_at "
                "FROM ai_eval_metrics_summary WHERE job_id = $1 ORDER BY snapshot_at, id",
                job_id,
            )
        detail = self._job_row_to_dict(dict(row))
        detail["gates"] = [
            {
                "metric_name": str(gate.get("metric_name") or ""),
                "value": float(gate.get("value") or 0.0),
                "threshold": float(gate.get("threshold") or 0.0),
                "status": str(gate.get("status") or ""),
            }
            for gate in (dict(g) for g in gate_rows)
        ]
        return detail

    async def cancel_job(self, job_id: UUID) -> dict[str, Any] | None:
        """Cancel an active job by marking it failed (Task G5)."""
        async with self._db_pool.acquire() as conn:
            row = await conn.fetchrow(
                "UPDATE ai_eval_jobs SET status = 'failed', "
                "completed_at = NOW(), error_message = 'cancelled by user' "
                "WHERE job_id = $1 AND status IN ('pending', 'running') "
                "RETURNING job_id, name, description, dataset_path, status, "
                "progress_percent, total_runs, completed_runs, metrics_config, "
                "created_at, started_at, completed_at, error_message",
                job_id,
            )
        return self._job_row_to_dict(dict(row)) if row is not None else None

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
