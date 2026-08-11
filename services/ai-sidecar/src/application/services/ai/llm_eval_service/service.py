"""LLM evaluation service for multi-profile benchmark runs.

This service keeps evaluation jobs in memory and never persists raw API keys.
"""

from __future__ import annotations

import asyncio
import json
import math
import statistics
import time
from collections.abc import Sequence
from pathlib import Path
from typing import Any, ClassVar

import httpx
import yaml

from src.application.services.ai.llm_eval_constants import DEFAULT_EVAL_SUITE, SUPPORTED_EVAL_SUITES
from src.domain.utils.time_utils import utc_now
from src.infrastructure.ai.openai_client import (
    Message,
    MessageRole,
    OpenAIClient,
    OpenAIClientConfig,
    ResponsesAPIResponse,
)
from src.infrastructure.ai.tools.query_tools import QUERY_TOOLS
from src.infrastructure.common.exceptions import LLM_EXCEPTIONS
from src.infrastructure.logging.core import get_logger
from src.shared.id_generator import generate_id

from .models import ArgExpectation, EvalCaseDefinition, RuntimeProfile

logger = get_logger(__name__)


class LLMEvalService:
    """In-memory LLM evaluation orchestrator."""

    SUPPORTED_SUITES: ClassVar[set[str]] = set(SUPPORTED_EVAL_SUITES)
    DEFAULT_CASE_FILE = Path(__file__).resolve().parents[5] / "config" / "llm_eval_cases.yaml"

    def __init__(self, max_retained_jobs: int = 30, cases_file_path: str | None = None):
        self._jobs: dict[str, dict[str, Any]] = {}
        self._runtime_profiles: dict[str, dict[str, RuntimeProfile]] = {}
        self._tasks: dict[str, asyncio.Task] = {}
        self._cancel_events: dict[str, asyncio.Event] = {}
        self._lock = asyncio.Lock()
        self._max_retained_jobs = max(5, int(max_retained_jobs))
        self._cases_file_path = Path(cases_file_path).resolve() if cases_file_path else self.DEFAULT_CASE_FILE

    async def create_job(
        self,
        *,
        profiles: Sequence[dict[str, Any]],
        options: dict[str, Any],
        owner_id: str | None,
        owner_roles: list[str] | None,
    ) -> dict[str, Any]:
        """Create and start a benchmark job."""
        normalized_profiles = self._normalize_profiles(list(profiles))
        normalized_options = self._normalize_options(options)
        cases = self._build_suite(normalized_options["suite"])

        if not normalized_profiles:
            raise ValueError("profiles cannot be empty")
        if not cases:
            raise ValueError("no evaluation cases available for selected suite")

        attempts_per_profile = len(cases) * normalized_options["repeat_count"]
        total_attempts = attempts_per_profile * len(normalized_profiles)

        job_id = generate_id("eval")
        created_at = utc_now().isoformat()

        public_profiles: list[dict[str, Any]] = []
        runtime_profile_map: dict[str, RuntimeProfile] = {}
        for profile in normalized_profiles:
            runtime_profile = RuntimeProfile(
                profile_id=profile["profile_id"],
                name=profile["name"],
                base_url=profile["base_url"],
                api_key=profile["api_key"],
                model=profile["model"],
                timeout=float(profile["timeout"]),
                max_retries=int(profile["max_retries"]),
                retry_delay=float(profile["retry_delay"]),
                reasoning_effort=profile.get("reasoning_effort"),
                max_completion_tokens=profile.get("max_completion_tokens"),
                api_mode=str(profile.get("api_mode") or "chat"),
                instructions=profile.get("instructions"),
                reasoning_summary=profile.get("reasoning_summary"),
                store=profile.get("store"),
                include=profile.get("include"),
            )
            runtime_profile_map[runtime_profile.profile_id] = runtime_profile

            public_profiles.append(
                {
                    "profile_id": runtime_profile.profile_id,
                    "name": runtime_profile.name,
                    "base_url": runtime_profile.base_url,
                    "model": runtime_profile.model,
                    "timeout": runtime_profile.timeout,
                    "max_retries": runtime_profile.max_retries,
                    "retry_delay": runtime_profile.retry_delay,
                    "api_key_masked": self._mask_api_key(runtime_profile.api_key),
                    "status": "pending",
                    "progress": {
                        "completed_attempts": 0,
                        "total_attempts": attempts_per_profile,
                        "percentage": 0.0,
                    },
                    "metrics": None,
                    "cases": [],
                    "error_message": None,
                }
            )

        job = {
            "job_id": job_id,
            "status": "pending",
            "created_at": created_at,
            "started_at": None,
            "finished_at": None,
            "owner": {
                "user_id": owner_id,
                "roles": list(owner_roles or []),
            },
            "options": normalized_options,
            "suite": {
                "suite_id": normalized_options["suite"],
                "total_cases": len(cases),
                "case_ids": [case.case_id for case in cases],
            },
            "progress": {
                "completed_attempts": 0,
                "total_attempts": total_attempts,
                "percentage": 0.0,
            },
            "profiles": public_profiles,
            "ranking": [],
            "error_message": None,
        }

        cancel_event = asyncio.Event()

        async with self._lock:
            self._jobs[job_id] = job
            self._runtime_profiles[job_id] = runtime_profile_map
            self._cancel_events[job_id] = cancel_event
            task = asyncio.create_task(self._run_job(job_id))
            self._tasks[job_id] = task
            self._prune_jobs_locked()

        return {
            "job_id": job_id,
            "status": "pending",
            "created_at": created_at,
        }

    async def list_jobs(self, limit: int = 20) -> list[dict[str, Any]]:
        """List recent jobs in reverse creation order."""
        safe_limit = max(1, min(int(limit), 100))
        async with self._lock:
            jobs = sorted(
                self._jobs.values(),
                key=lambda item: item.get("created_at") or "",
                reverse=True,
            )[:safe_limit]
            return [self._job_snapshot(item, include_profiles=False) for item in jobs]

    async def get_job(self, job_id: str) -> dict[str, Any] | None:
        """Get a full job snapshot."""
        async with self._lock:
            job = self._jobs.get(job_id)
            if not job:
                return None
            return self._job_snapshot(job, include_profiles=True)

    async def cancel_job(self, job_id: str) -> bool:
        """Cancel an active job."""
        task: asyncio.Task | None = None
        async with self._lock:
            job = self._jobs.get(job_id)
            if not job:
                return False
            if job["status"] in {"completed", "failed", "cancelled"}:
                return False

            job["status"] = "cancelling"
            cancel_event = self._cancel_events.get(job_id)
            if cancel_event:
                cancel_event.set()
            task = self._tasks.get(job_id)

        if task:
            task.cancel()
        return True

    async def compare_job_profiles(
        self,
        job_id: str,
        left_profile_id: str | None,
        right_profile_id: str | None,
    ) -> dict[str, Any] | None:
        """Compare two profiles within a completed/running job."""
        async with self._lock:
            job = self._jobs.get(job_id)
            if not job:
                return None

            profiles = list(job.get("profiles") or [])
            if len(profiles) < 2:
                raise ValueError("at least two profiles are required for comparison")

            profile_map = {item["profile_id"]: item for item in profiles}
            if left_profile_id and left_profile_id not in profile_map:
                raise ValueError(f"left profile not found: {left_profile_id}")
            if right_profile_id and right_profile_id not in profile_map:
                raise ValueError(f"right profile not found: {right_profile_id}")

            ranking = list(job.get("ranking") or [])
            if not left_profile_id or not right_profile_id:
                if len(ranking) >= 2:
                    left_profile_id = left_profile_id or ranking[0]["profile_id"]
                    right_profile_id = right_profile_id or ranking[1]["profile_id"]
                else:
                    left_profile_id = left_profile_id or profiles[0]["profile_id"]
                    right_profile_id = right_profile_id or profiles[1]["profile_id"]

            left = profile_map[left_profile_id]
            right = profile_map[right_profile_id]
            return self._build_compare_payload(left, right)

    async def _run_job(self, job_id: str) -> None:
        """Background job runner."""
        start_time = utc_now().isoformat()
        cases: list[EvalCaseDefinition] = []
        runtime_profiles: list[RuntimeProfile] = []
        options: dict[str, Any] = {}

        async with self._lock:
            job = self._jobs.get(job_id)
            if not job:
                return
            job["status"] = "running"
            job["started_at"] = start_time
            options = dict(job.get("options") or {})
            suite_id = str(job.get("suite", {}).get("suite_id", "quick"))
            cases = self._build_suite(suite_id)
            runtime_profiles = list((self._runtime_profiles.get(job_id) or {}).values())

        try:
            profile_semaphore = asyncio.Semaphore(max(1, int(options.get("profile_concurrency", 1))))

            async def _run_profile_guard(profile: RuntimeProfile):
                async with profile_semaphore:
                    await self._run_profile(job_id, profile, cases, options)

            await asyncio.gather(*(_run_profile_guard(profile) for profile in runtime_profiles))

            async with self._lock:
                job = self._jobs.get(job_id)
                if not job:
                    return
                if job["status"] in {"cancelling", "cancelled"}:
                    job["status"] = "cancelled"
                else:
                    job["status"] = "completed"
                job["finished_at"] = utc_now().isoformat()
                job["ranking"] = self._build_ranking(job.get("profiles") or [])

        except asyncio.CancelledError:
            async with self._lock:
                job = self._jobs.get(job_id)
                if job:
                    job["status"] = "cancelled"
                    job["finished_at"] = utc_now().isoformat()
            raise
        except Exception as exc:  # noqa: BLE001 - background loop must not die on any error
            logger.error(f"LLM eval job failed ({job_id}): {exc}")
            async with self._lock:
                job = self._jobs.get(job_id)
                if job:
                    job["status"] = "failed"
                    job["finished_at"] = utc_now().isoformat()
                    job["error_message"] = str(exc)
        finally:
            async with self._lock:
                self._runtime_profiles.pop(job_id, None)
                self._cancel_events.pop(job_id, None)
                self._tasks.pop(job_id, None)

    async def _run_profile(
        self,
        job_id: str,
        profile: RuntimeProfile,
        cases: list[EvalCaseDefinition],
        options: dict[str, Any],
    ) -> None:
        """Run benchmark for one profile."""
        cancel_event = self._cancel_events.get(job_id)
        if cancel_event and cancel_event.is_set():
            return

        await self._update_profile_status(job_id, profile.profile_id, "running")

        repeat_count = max(1, int(options.get("repeat_count", 1)))
        case_concurrency = max(1, int(options.get("case_concurrency", 1)))
        enable_tool_routing = bool(options.get("enable_tool_routing", True))

        attempts_by_case: dict[str, list[dict[str, Any]]] = {case.case_id: [] for case in cases}
        case_index = {case.case_id: case for case in cases}

        client: OpenAIClient | None = None
        try:
            client = OpenAIClient(
                config=OpenAIClientConfig(
                    api_key=profile.api_key,
                    base_url=profile.base_url,
                    default_model=profile.model,
                    timeout=profile.timeout,
                    max_retries=profile.max_retries,
                    retry_delay=profile.retry_delay,
                )
            )

            attempt_jobs: list[tuple[EvalCaseDefinition, int]] = []
            for case in cases:
                for attempt_index in range(repeat_count):
                    attempt_jobs.append((case, attempt_index + 1))

            semaphore = asyncio.Semaphore(case_concurrency)

            use_responses_api = profile.api_mode == "responses"

            async def _run_attempt(case: EvalCaseDefinition, attempt_index: int):
                async with semaphore:
                    if cancel_event and cancel_event.is_set():
                        raise asyncio.CancelledError()

                    if use_responses_api:
                        if enable_tool_routing:
                            result = await self._execute_tool_case_responses(client, profile, case)
                        else:
                            result = await self._execute_text_case_responses(client, profile, case)
                    elif enable_tool_routing:
                        result = await self._execute_tool_case(client, profile, case)
                    else:
                        result = await self._execute_text_case(client, profile, case)

                    result["attempt_index"] = attempt_index
                    attempts_by_case[case.case_id].append(result)
                    await self._increment_progress(job_id, profile.profile_id)

            await asyncio.gather(*(_run_attempt(case, idx) for case, idx in attempt_jobs))

            case_results: list[dict[str, Any]] = []
            for case_id, attempts in attempts_by_case.items():
                definition = case_index[case_id]
                ordered_attempts = sorted(attempts, key=lambda item: int(item["attempt_index"]))
                case_results.append(self._aggregate_case_results(definition, ordered_attempts))

            metrics = self._aggregate_profile_metrics(case_results, repeat_count)
            await self._set_profile_result(
                job_id,
                profile.profile_id,
                status="completed",
                metrics=metrics,
                case_results=sorted(case_results, key=lambda item: item["case_id"]),
                error_message=None,
            )

        except asyncio.CancelledError:
            await self._set_profile_result(
                job_id,
                profile.profile_id,
                status="cancelled",
                metrics=None,
                case_results=[],
                error_message="cancelled",
            )
            raise
        except Exception as exc:  # noqa: BLE001 - background loop must not die on any error
            logger.error(f"LLM eval profile failed ({profile.profile_id}): {exc}")
            await self._set_profile_result(
                job_id,
                profile.profile_id,
                status="failed",
                metrics=None,
                case_results=[],
                error_message=str(exc),
            )
        finally:
            if client is not None:
                await self._safe_close_client(client)

    async def _execute_tool_case(
        self,
        client: OpenAIClient,
        profile: RuntimeProfile,
        case: EvalCaseDefinition,
    ) -> dict[str, Any]:
        """Execute one case using OpenAI function-calling style."""
        started = time.perf_counter()
        response = None

        system_prompt = (
            "你是测试模式下的工具路由器。必须从提供的 tools 中选择最合适的一个并给出参数。不要直接回答业务结论。"
        )

        extra_kwargs = self._build_reasoning_kwargs(profile)

        try:
            response = await client.chat_completion(
                messages=[
                    Message(role=MessageRole.SYSTEM, content=system_prompt),
                    Message(role=MessageRole.USER, content=case.prompt),
                ],
                model=profile.model,
                stream=False,
                tools=QUERY_TOOLS,
                tool_choice="auto",
                **extra_kwargs,
            )
            observed_tool, observed_arguments, assistant_content = self._extract_tool_call_response(response)
            evaluation = self._evaluate_case(case, observed_tool, observed_arguments)
            latency_ms = int((time.perf_counter() - started) * 1000)
            usage = self._extract_usage(response)
            reasoning_content = self._extract_reasoning_content(response)

            return {
                "success": evaluation["success"],
                "case_score": evaluation["case_score"],
                "tool_match": evaluation["tool_match"],
                "arg_required_score": evaluation["arg_required_score"],
                "arg_value_score": evaluation["arg_value_score"],
                "observed_tool": observed_tool,
                "observed_arguments": observed_arguments,
                "signature": self._build_signature(case, observed_tool, observed_arguments),
                "latency_ms": latency_ms,
                "error_message": None,
                "fallback_used": False,
                "fallback_reason": None,
                "usage": usage,
                "assistant_excerpt": assistant_content[:400] if assistant_content else "",
                "reasoning_content": reasoning_content,
            }
        except LLM_EXCEPTIONS as exc:
            if self._should_fallback_to_text_mode(exc):
                fallback_result = await self._execute_text_case(client, profile, case)
                fallback_result["fallback_used"] = True
                fallback_result["fallback_reason"] = self._build_exception_text(exc)
                return fallback_result

            latency_ms = int((time.perf_counter() - started) * 1000)
            return {
                "success": False,
                "case_score": 0.0,
                "tool_match": False,
                "arg_required_score": 0.0,
                "arg_value_score": 0.0,
                "observed_tool": None,
                "observed_arguments": {},
                "signature": f"__error__:{type(exc).__name__}",
                "latency_ms": latency_ms,
                "error_message": self._build_exception_text(exc),
                "fallback_used": False,
                "fallback_reason": None,
                "usage": self._extract_usage(response),
                "assistant_excerpt": "",
                "reasoning_content": None,
            }

    async def _execute_text_case(
        self,
        client: OpenAIClient,
        profile: RuntimeProfile,
        case: EvalCaseDefinition,
    ) -> dict[str, Any]:
        """Execute one case with text-only constrained JSON output."""
        started = time.perf_counter()
        response = None

        routing_rules = "\n".join(
            [f"- {tool_name}" for tool_name in sorted({tool["function"]["name"] for tool in QUERY_TOOLS})]
        )
        system_prompt = (
            "你是工具路由评测器。"
            "根据用户问题输出 JSON，格式必须为 "
            '{"tool": "<tool_name>", "arguments": { ... }}。'
            "不得输出其他文本。"
            f"可选工具:\n{routing_rules}"
        )

        extra_kwargs = self._build_reasoning_kwargs(profile)

        try:
            response = await client.chat_completion(
                messages=[
                    Message(role=MessageRole.SYSTEM, content=system_prompt),
                    Message(role=MessageRole.USER, content=case.prompt),
                ],
                model=profile.model,
                stream=False,
                **extra_kwargs,
            )

            content = self._extract_response_content(response)
            parsed = self._parse_json_like_text(content)
            observed_tool = str(parsed.get("tool") or "").strip() or None
            observed_arguments = parsed.get("arguments") if isinstance(parsed.get("arguments"), dict) else {}

            evaluation = self._evaluate_case(case, observed_tool, observed_arguments)
            latency_ms = int((time.perf_counter() - started) * 1000)
            usage = self._extract_usage(response)
            reasoning_content = self._extract_reasoning_content(response)

            return {
                "success": evaluation["success"],
                "case_score": evaluation["case_score"],
                "tool_match": evaluation["tool_match"],
                "arg_required_score": evaluation["arg_required_score"],
                "arg_value_score": evaluation["arg_value_score"],
                "observed_tool": observed_tool,
                "observed_arguments": observed_arguments,
                "signature": self._build_signature(case, observed_tool, observed_arguments),
                "latency_ms": latency_ms,
                "error_message": None,
                "fallback_used": False,
                "fallback_reason": None,
                "usage": usage,
                "assistant_excerpt": content[:400],
                "reasoning_content": reasoning_content,
            }
        except LLM_EXCEPTIONS as exc:
            latency_ms = int((time.perf_counter() - started) * 1000)
            return {
                "success": False,
                "case_score": 0.0,
                "tool_match": False,
                "arg_required_score": 0.0,
                "arg_value_score": 0.0,
                "observed_tool": None,
                "observed_arguments": {},
                "signature": f"__error__:{type(exc).__name__}",
                "latency_ms": latency_ms,
                "error_message": self._build_exception_text(exc),
                "fallback_used": False,
                "fallback_reason": None,
                "usage": self._extract_usage(response),
                "assistant_excerpt": "",
                "reasoning_content": None,
            }

    # Responses API execution paths

    def _build_responses_kwargs(self, profile: RuntimeProfile) -> dict[str, Any]:
        """Build extra kwargs for responses_create based on profile settings."""
        kwargs: dict[str, Any] = {}
        if profile.instructions:
            kwargs["instructions"] = profile.instructions
        if profile.store is not None:
            kwargs["store"] = profile.store
        if profile.include:
            kwargs["include"] = profile.include

        # Reasoning: Responses API uses nested {effort, summary} object
        if profile.reasoning_effort or profile.reasoning_summary:
            reasoning: dict[str, str] = {}
            if profile.reasoning_effort:
                reasoning["effort"] = profile.reasoning_effort
            if profile.reasoning_summary:
                reasoning["summary"] = profile.reasoning_summary
            kwargs["reasoning"] = reasoning

        if profile.max_completion_tokens is not None:
            kwargs["max_output_tokens"] = profile.max_completion_tokens
        return kwargs

    async def _execute_tool_case_responses(
        self,
        client: OpenAIClient,
        profile: RuntimeProfile,
        case: EvalCaseDefinition,
    ) -> dict[str, Any]:
        """Execute one case using the Responses API with function tools."""
        started = time.perf_counter()
        resp: ResponsesAPIResponse | None = None

        system_instructions = (
            profile.instructions
            or "你是测试模式下的工具路由器。必须从提供的 tools 中选择最合适的一个并给出参数。不要直接回答业务结论。"
        )

        responses_tools = self._convert_tools_for_responses(QUERY_TOOLS)
        extra_kwargs = self._build_responses_kwargs(profile)
        extra_kwargs.pop("instructions", None)

        try:
            resp = await client.responses_create(
                model=profile.model,
                input=case.prompt,
                instructions=system_instructions,
                tools=responses_tools,
                tool_choice="auto",
                stream=False,
                **extra_kwargs,
            )
            observed_tool, observed_arguments, assistant_content = self._extract_tool_call_from_responses(resp)
            evaluation = self._evaluate_case(case, observed_tool, observed_arguments)
            latency_ms = int((time.perf_counter() - started) * 1000)
            usage = self._extract_responses_usage(resp)
            reasoning_content = self._extract_responses_reasoning(resp)

            return {
                "success": evaluation["success"],
                "case_score": evaluation["case_score"],
                "tool_match": evaluation["tool_match"],
                "arg_required_score": evaluation["arg_required_score"],
                "arg_value_score": evaluation["arg_value_score"],
                "observed_tool": observed_tool,
                "observed_arguments": observed_arguments,
                "signature": self._build_signature(case, observed_tool, observed_arguments),
                "latency_ms": latency_ms,
                "error_message": None,
                "fallback_used": False,
                "fallback_reason": None,
                "usage": usage,
                "assistant_excerpt": assistant_content[:400] if assistant_content else "",
                "reasoning_content": reasoning_content,
            }
        except LLM_EXCEPTIONS as exc:
            if self._should_fallback_to_text_mode(exc):
                fallback_result = await self._execute_text_case_responses(client, profile, case)
                fallback_result["fallback_used"] = True
                fallback_result["fallback_reason"] = self._build_exception_text(exc)
                return fallback_result

            latency_ms = int((time.perf_counter() - started) * 1000)
            return {
                "success": False,
                "case_score": 0.0,
                "tool_match": False,
                "arg_required_score": 0.0,
                "arg_value_score": 0.0,
                "observed_tool": None,
                "observed_arguments": {},
                "signature": f"__error__:{type(exc).__name__}",
                "latency_ms": latency_ms,
                "error_message": self._build_exception_text(exc),
                "fallback_used": False,
                "fallback_reason": None,
                "usage": self._extract_responses_usage(resp),
                "assistant_excerpt": "",
                "reasoning_content": None,
            }

    async def _execute_text_case_responses(
        self,
        client: OpenAIClient,
        profile: RuntimeProfile,
        case: EvalCaseDefinition,
    ) -> dict[str, Any]:
        """Execute one case using the Responses API with text-only constrained JSON output."""
        started = time.perf_counter()
        resp: ResponsesAPIResponse | None = None

        routing_rules = "\n".join([f"- {tn}" for tn in sorted({t["function"]["name"] for t in QUERY_TOOLS})])
        system_instructions = (
            profile.instructions
            or "\u4f60\u662f\u5de5\u5177\u8def\u7531\u8bc4\u6d4b\u5668\u3002"
            "\u6839\u636e\u7528\u6237\u95ee\u9898\u8f93\u51fa JSON\uff0c\u683c\u5f0f\u5fc5\u987b\u4e3a "
            '{"tool": "<tool_name>", "arguments": { ... }}\u3002'
            "\u4e0d\u5f97\u8f93\u51fa\u5176\u4ed6\u6587\u672c\u3002"
            f"\u53ef\u9009\u5de5\u5177:\n{routing_rules}"
        )

        extra_kwargs = self._build_responses_kwargs(profile)
        extra_kwargs.pop("instructions", None)

        try:
            resp = await client.responses_create(
                model=profile.model,
                input=case.prompt,
                instructions=system_instructions,
                stream=False,
                **extra_kwargs,
            )

            content = resp.output_text if resp else ""
            parsed = self._parse_json_like_text(content)
            observed_tool = str(parsed.get("tool") or "").strip() or None
            observed_arguments = parsed.get("arguments") if isinstance(parsed.get("arguments"), dict) else {}

            evaluation = self._evaluate_case(case, observed_tool, observed_arguments)
            latency_ms = int((time.perf_counter() - started) * 1000)
            usage = self._extract_responses_usage(resp)
            reasoning_content = self._extract_responses_reasoning(resp)

            return {
                "success": evaluation["success"],
                "case_score": evaluation["case_score"],
                "tool_match": evaluation["tool_match"],
                "arg_required_score": evaluation["arg_required_score"],
                "arg_value_score": evaluation["arg_value_score"],
                "observed_tool": observed_tool,
                "observed_arguments": observed_arguments,
                "signature": self._build_signature(case, observed_tool, observed_arguments),
                "latency_ms": latency_ms,
                "error_message": None,
                "fallback_used": False,
                "fallback_reason": None,
                "usage": usage,
                "assistant_excerpt": content[:400],
                "reasoning_content": reasoning_content,
            }
        except LLM_EXCEPTIONS as exc:
            latency_ms = int((time.perf_counter() - started) * 1000)
            return {
                "success": False,
                "case_score": 0.0,
                "tool_match": False,
                "arg_required_score": 0.0,
                "arg_value_score": 0.0,
                "observed_tool": None,
                "observed_arguments": {},
                "signature": f"__error__:{type(exc).__name__}",
                "latency_ms": latency_ms,
                "error_message": self._build_exception_text(exc),
                "fallback_used": False,
                "fallback_reason": None,
                "usage": self._extract_responses_usage(resp),
                "assistant_excerpt": "",
                "reasoning_content": None,
            }

    # Responses API extractors

    def _extract_tool_call_from_responses(
        self,
        resp: ResponsesAPIResponse | None,
    ) -> tuple[str | None, dict[str, Any], str]:
        """Extract tool call from Responses API output items.

        In the Responses API, tool calls appear as ``type: "function_call"``
        items in ``output[]`` with ``name`` and ``arguments`` at top level.
        """
        if resp is None:
            return None, {}, ""

        assistant_content = resp.output_text
        for item in resp.output or []:
            if item.get("type") == "function_call":
                tool_name = str(item.get("name") or "").strip() or None
                arguments_raw = item.get("arguments")
                parsed_arguments = self._parse_arguments(arguments_raw)
                return tool_name, parsed_arguments, assistant_content

        # Fallback: try to parse JSON from output_text
        parsed_from_text = self._parse_json_like_text(assistant_content)
        text_tool = str(parsed_from_text.get("tool") or "").strip() or None
        text_arguments = parsed_from_text.get("arguments")
        if text_tool and isinstance(text_arguments, dict):
            return text_tool, text_arguments, assistant_content

        return None, {}, assistant_content

    def _extract_responses_usage(self, resp: Any) -> dict[str, int]:
        """Extract usage from a Responses API response.

        The Responses API uses ``input_tokens`` / ``output_tokens`` instead of
        ``prompt_tokens`` / ``completion_tokens``.  We normalize to the same
        dict shape used by the Chat Completions path so downstream aggregation
        works unchanged.
        """
        if resp is None:
            return {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0, "reasoning_tokens": 0}

        usage = getattr(resp, "usage", None)
        if not isinstance(usage, dict):
            return {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0, "reasoning_tokens": 0}

        input_tokens = int(usage.get("input_tokens", 0) or usage.get("prompt_tokens", 0) or 0)
        output_tokens = int(usage.get("output_tokens", 0) or usage.get("completion_tokens", 0) or 0)
        total_tokens = int(usage.get("total_tokens", 0) or 0) or (input_tokens + output_tokens)

        # reasoning_tokens may be at top level or nested in output_tokens_details
        reasoning_tokens = int(usage.get("reasoning_tokens", 0) or 0)
        if not reasoning_tokens:
            details = usage.get("output_tokens_details") or usage.get("completion_tokens_details")
            if isinstance(details, dict):
                reasoning_tokens = int(details.get("reasoning_tokens", 0) or 0)

        return {
            "prompt_tokens": input_tokens,
            "completion_tokens": output_tokens,
            "total_tokens": total_tokens,
            "reasoning_tokens": reasoning_tokens,
        }

    @staticmethod
    def _extract_responses_reasoning(resp: Any) -> str | None:
        """Extract reasoning chain from Responses API output items.

        Reasoning appears as ``type: "reasoning"`` items in ``output[]``
        with ``summary[]`` sub-items containing text.
        """
        if resp is None:
            return None

        output = getattr(resp, "output", None) or []
        parts: list[str] = []
        for item in output:
            if item.get("type") != "reasoning":
                continue
            for summary_item in item.get("summary") or []:
                text = summary_item.get("text") or ""
                if text.strip():
                    parts.append(text.strip())

        return "\n".join(parts) if parts else None

    @staticmethod
    def _convert_tools_for_responses(chat_tools: list[dict]) -> list[dict]:
        """Convert Chat Completions tool format to Responses API function format.

        Chat Completions: ``{"type": "function", "function": {"name": ..., ...}}``
        Responses API:    ``{"type": "function", "name": ..., ...}``
        """
        converted: list[dict] = []
        for tool in chat_tools:
            if tool.get("type") != "function":
                converted.append(tool)
                continue
            func = tool.get("function") or {}
            converted.append(
                {
                    "type": "function",
                    "name": func.get("name", ""),
                    "description": func.get("description", ""),
                    "parameters": func.get("parameters", {}),
                }
            )
        return converted

    async def _increment_progress(self, job_id: str, profile_id: str) -> None:
        async with self._lock:
            job = self._jobs.get(job_id)
            if not job:
                return

            progress = job["progress"]
            progress["completed_attempts"] += 1
            total_attempts = max(1, int(progress["total_attempts"]))
            progress["percentage"] = round(progress["completed_attempts"] / total_attempts * 100, 2)

            for profile in job.get("profiles") or []:
                if profile.get("profile_id") != profile_id:
                    continue
                profile_progress = profile.get("progress") or {}
                profile_progress["completed_attempts"] = int(profile_progress.get("completed_attempts", 0)) + 1
                profile_total = max(1, int(profile_progress.get("total_attempts", 1)))
                profile_progress["percentage"] = round(
                    profile_progress["completed_attempts"] / profile_total * 100,
                    2,
                )
                profile["progress"] = profile_progress
                break

    async def _update_profile_status(self, job_id: str, profile_id: str, status: str) -> None:
        async with self._lock:
            job = self._jobs.get(job_id)
            if not job:
                return
            for profile in job.get("profiles") or []:
                if profile.get("profile_id") == profile_id:
                    profile["status"] = status
                    break

    async def _set_profile_result(
        self,
        job_id: str,
        profile_id: str,
        *,
        status: str,
        metrics: dict[str, Any] | None,
        case_results: list[dict[str, Any]],
        error_message: str | None,
    ) -> None:
        async with self._lock:
            job = self._jobs.get(job_id)
            if not job:
                return
            for profile in job.get("profiles") or []:
                if profile.get("profile_id") != profile_id:
                    continue
                profile["status"] = status
                profile["metrics"] = metrics
                profile["cases"] = case_results
                profile["error_message"] = error_message
                if profile.get("progress"):
                    profile["progress"]["percentage"] = 100.0
                break

    def _normalize_profiles(self, profiles: list[dict[str, Any]]) -> list[dict[str, Any]]:
        normalized: list[dict[str, Any]] = []
        seen_ids: set[str] = set()

        for index, raw_profile in enumerate(profiles):
            profile_id = str(raw_profile.get("profile_id") or f"profile_{index + 1}").strip()
            if not profile_id:
                raise ValueError("profile_id cannot be empty")
            if profile_id in seen_ids:
                raise ValueError(f"duplicate profile_id: {profile_id}")
            seen_ids.add(profile_id)

            base_url = str(raw_profile.get("base_url") or "").strip()
            api_key = str(raw_profile.get("api_key") or "").strip()
            model = str(raw_profile.get("model") or "").strip()
            if not base_url:
                raise ValueError(f"profile {profile_id}: base_url is required")
            if not api_key:
                raise ValueError(f"profile {profile_id}: api_key is required")
            if not model:
                raise ValueError(f"profile {profile_id}: model is required")

            normalized.append(
                {
                    "profile_id": profile_id,
                    "name": str(raw_profile.get("name") or profile_id).strip() or profile_id,
                    "base_url": base_url,
                    "api_key": api_key,
                    "model": model,
                    "timeout": float(raw_profile.get("timeout", 30.0)),
                    "max_retries": int(raw_profile.get("max_retries", 2)),
                    "retry_delay": float(raw_profile.get("retry_delay", 0.5)),
                    "reasoning_effort": raw_profile.get("reasoning_effort"),
                    "max_completion_tokens": raw_profile.get("max_completion_tokens"),
                    # Responses API fields
                    "api_mode": str(raw_profile.get("api_mode") or "chat").strip().lower(),
                    "instructions": raw_profile.get("instructions"),
                    "reasoning_summary": raw_profile.get("reasoning_summary"),
                    "store": raw_profile.get("store"),
                    "include": raw_profile.get("include"),
                }
            )

        return normalized

    def _normalize_options(self, raw_options: dict[str, Any]) -> dict[str, Any]:
        suite = str(raw_options.get("suite", DEFAULT_EVAL_SUITE)).strip().lower()
        if suite not in self.SUPPORTED_SUITES:
            raise ValueError(f"unsupported suite: {suite}")

        return {
            "suite": suite,
            "repeat_count": max(1, min(int(raw_options.get("repeat_count", 2)), 10)),
            "profile_concurrency": max(1, min(int(raw_options.get("profile_concurrency", 1)), 10)),
            "case_concurrency": max(1, min(int(raw_options.get("case_concurrency", 1)), 16)),
            "enable_tool_routing": bool(raw_options.get("enable_tool_routing", True)),
        }

    def _build_suite(self, suite: str) -> list[EvalCaseDefinition]:
        suite_name = str(suite or "").strip().lower()
        if suite_name not in self.SUPPORTED_SUITES:
            raise ValueError(f"unsupported suite: {suite_name}")

        case_library = self._load_case_library()
        selected_cases = [case for case in case_library if suite_name in case.suites]
        if not selected_cases:
            raise ValueError(f"no cases configured for suite: {suite_name}")

        return selected_cases

    def _load_case_library(self) -> list[EvalCaseDefinition]:
        raw_cases = self._load_case_payloads()
        parsed_cases: list[EvalCaseDefinition] = []
        seen_ids: set[str] = set()

        for index, raw_case in enumerate(raw_cases):
            case = self._parse_case_definition(raw_case, index)
            if case.case_id in seen_ids:
                raise ValueError(f"duplicate case_id in eval cases: {case.case_id}")
            seen_ids.add(case.case_id)
            parsed_cases.append(case)

        if not parsed_cases:
            raise ValueError("eval cases are empty")

        return parsed_cases

    def _load_case_payloads(self) -> list[dict[str, Any]]:
        if not self._cases_file_path.exists():
            raise ValueError(f"eval cases file not found: {self._cases_file_path}")

        try:
            content = self._cases_file_path.read_text(encoding="utf-8")
            loaded = yaml.safe_load(content)
        except Exception as exc:
            raise ValueError(f"failed to read eval cases file: {exc}") from exc

        if loaded is None:
            return []

        if isinstance(loaded, dict):
            raw_cases = loaded.get("cases")
        elif isinstance(loaded, list):
            raw_cases = loaded
        else:
            raise ValueError("eval cases file must be a map with 'cases' or a list")

        if not isinstance(raw_cases, list):
            raise ValueError("eval cases 'cases' must be a list")

        return raw_cases

    def _parse_case_definition(self, raw_case: dict[str, Any], index: int) -> EvalCaseDefinition:
        if not isinstance(raw_case, dict):
            raise ValueError(f"case[{index}] must be an object")

        case_id = str(raw_case.get("case_id") or "").strip()
        prompt = str(raw_case.get("prompt") or "").strip()
        if not case_id:
            raise ValueError(f"case[{index}] missing case_id")
        if not prompt:
            raise ValueError(f"case[{index}] missing prompt")

        eval_type = str(raw_case.get("eval_type") or "tool_routing").strip()
        expected_behavior = str(raw_case.get("expected_behavior") or "tool_call").strip()

        expected_tools_raw = raw_case.get("expected_tools")
        if not isinstance(expected_tools_raw, list):
            raise ValueError(f"case[{case_id}] expected_tools must be a list")

        expected_tools = [str(item).strip() for item in expected_tools_raw if str(item).strip()]
        if not expected_tools and expected_behavior not in ("fallback", "requires_approval"):
            raise ValueError(
                f"case[{case_id}] expected_tools cannot be empty unless behavior is fallback or requires_approval"
            )

        available_tools = {tool["function"]["name"] for tool in QUERY_TOOLS}
        invalid_tools = [name for name in expected_tools if name not in available_tools]
        if invalid_tools:
            logger.debug(f"case[{case_id}] uses tools not in QUERY_TOOLS: {invalid_tools}")

        tags_raw = raw_case.get("tags") or []
        if not isinstance(tags_raw, list):
            raise ValueError(f"case[{case_id}] tags must be a list")
        tags = [str(tag).strip() for tag in tags_raw if str(tag).strip()]

        suites_raw = raw_case.get("suites", ["quick", "standard", "full"])
        if isinstance(suites_raw, str):
            suites_raw = [suites_raw]
        if not isinstance(suites_raw, list) or not suites_raw:
            raise ValueError(f"case[{case_id}] suites must be a non-empty list")

        suites: list[str] = []
        for raw_suite in suites_raw:
            suite_name = str(raw_suite or "").strip().lower()
            if not suite_name:
                continue
            if suite_name not in self.SUPPORTED_SUITES:
                raise ValueError(f"case[{case_id}] has unsupported suite: {suite_name}")
            if suite_name not in suites:
                suites.append(suite_name)

        if not suites:
            raise ValueError(f"case[{case_id}] suites cannot be empty")

        expectations_raw = raw_case.get("expectations") or []
        if not isinstance(expectations_raw, list):
            raise ValueError(f"case[{case_id}] expectations must be a list")

        expectations = [
            self._parse_expectation(case_id, expectation_raw, expectation_index)
            for expectation_index, expectation_raw in enumerate(expectations_raw)
        ]

        return EvalCaseDefinition(
            case_id=case_id,
            prompt=prompt,
            expected_tools=expected_tools,
            expectations=expectations,
            tags=tags,
            suites=suites,
            eval_type=eval_type,
            expected_behavior=expected_behavior,
        )

    def _parse_expectation(self, case_id: str, raw_expectation: Any, index: int) -> ArgExpectation:
        if not isinstance(raw_expectation, dict):
            raise ValueError(f"case[{case_id}] expectation[{index}] must be an object")

        key = str(raw_expectation.get("key") or "").strip()
        if not key:
            raise ValueError(f"case[{case_id}] expectation[{index}] missing key")

        one_of_raw = raw_expectation.get("one_of")
        one_of: list[Any] | None
        if one_of_raw is None:
            one_of = None
        elif isinstance(one_of_raw, list):
            one_of = one_of_raw
        else:
            one_of = [one_of_raw]

        contains_value = raw_expectation.get("contains")
        contains = str(contains_value) if contains_value is not None else None

        min_value_raw = raw_expectation.get("min_value")
        min_value: float | None = None
        if min_value_raw is not None:
            try:
                min_value = float(min_value_raw)
            except (TypeError, ValueError) as exc:
                raise ValueError(f"case[{case_id}] expectation[{index}] min_value must be numeric") from exc

        return ArgExpectation(
            key=key,
            required=bool(raw_expectation.get("required", True)),
            expected=raw_expectation.get("expected"),
            contains=contains,
            one_of=one_of,
            min_value=min_value,
        )

    def _evaluate_case(
        self,
        case: EvalCaseDefinition,
        observed_tool: str | None,
        observed_arguments: dict[str, Any],
    ) -> dict[str, Any]:
        if case.expected_behavior == "fallback":
            success = not bool(observed_tool)
            return {
                "success": success,
                "case_score": 1.0 if success else 0.0,
                "tool_match": success,
                "arg_required_score": 1.0 if success else 0.0,
                "arg_value_score": 1.0 if success else 0.0,
            }

        tool_match = bool(observed_tool and observed_tool in set(case.expected_tools))

        if case.eval_type == "text2sql":
            sql = str(observed_arguments.get("sql", "")).upper()
            sql_score = 0.0
            if tool_match and sql:
                sql_score = 1.0
                for exp in case.expectations:
                    if exp.key == "sql_not_contains" and exp.contains and str(exp.contains).upper() in sql:
                        sql_score -= 0.5
                    elif exp.key != "sql_not_contains" and exp.contains and str(exp.contains).upper() not in sql:
                        sql_score -= 0.2
                    if exp.expected and str(exp.expected).upper() not in sql:
                        sql_score -= 0.2
            sql_score = max(0.0, round(sql_score, 4))
            success = sql_score >= 0.85 and tool_match
            return {
                "success": success,
                "case_score": sql_score,
                "tool_match": tool_match,
                "arg_required_score": sql_score,
                "arg_value_score": sql_score,
            }

        required_expectations = [item for item in case.expectations if item.required]
        required_hits = 0
        for expectation in required_expectations:
            if self._has_value(observed_arguments.get(expectation.key)):
                required_hits += 1

        value_expectations = [
            item
            for item in case.expectations
            if item.expected is not None
            or item.contains is not None
            or item.one_of is not None
            or item.min_value is not None
        ]
        value_hits = 0
        for expectation in value_expectations:
            if self._match_expectation(expectation, observed_arguments.get(expectation.key)):
                value_hits += 1

        arg_required_score = required_hits / len(required_expectations) if required_expectations else 1.0
        arg_value_score = value_hits / len(value_expectations) if value_expectations else 1.0

        case_score = round(
            (0.6 if tool_match else 0.0) + (0.25 * arg_required_score) + (0.15 * arg_value_score),
            4,
        )
        success = case_score >= 0.85

        return {
            "success": success,
            "case_score": case_score,
            "tool_match": tool_match,
            "arg_required_score": round(arg_required_score, 4),
            "arg_value_score": round(arg_value_score, 4),
        }

    def _match_expectation(self, expectation: ArgExpectation, value: Any) -> bool:
        if not self._has_value(value):
            return False

        if expectation.expected is not None and value != expectation.expected:
            return False
        if expectation.contains is not None and expectation.contains not in str(value):
            return False
        if expectation.one_of is not None and value not in expectation.one_of:
            return False
        if expectation.min_value is not None:
            try:
                numeric = float(value)
            except (TypeError, ValueError):
                return False
            if numeric < float(expectation.min_value):
                return False

        return True

    def _aggregate_case_results(
        self,
        case: EvalCaseDefinition,
        attempts: list[dict[str, Any]],
    ) -> dict[str, Any]:
        success_count = sum(1 for item in attempts if item.get("success"))
        error_count = sum(1 for item in attempts if item.get("error_message"))
        fallback_count = sum(1 for item in attempts if item.get("fallback_used"))
        score_values = [float(item.get("case_score", 0.0)) for item in attempts]
        latency_values = [int(item.get("latency_ms", 0)) for item in attempts]

        signature_counter: dict[str, int] = {}
        successful_attempts = [item for item in attempts if item.get("success")]
        for item in successful_attempts:
            signature = str(item.get("signature") or "")
            signature_counter[signature] = signature_counter.get(signature, 0) + 1

        consistency = 0.0
        if successful_attempts and signature_counter:
            consistency = max(signature_counter.values()) / len(successful_attempts)

        return {
            "case_id": case.case_id,
            "prompt": case.prompt,
            "expected_tools": case.expected_tools,
            "tags": case.tags,
            "suites": case.suites,
            "attempts": attempts,
            "summary": {
                "attempt_count": len(attempts),
                "success_count": success_count,
                "error_count": error_count,
                "fallback_count": fallback_count,
                "success_rate": round(success_count / len(attempts), 4) if attempts else 0.0,
                "error_rate": round(error_count / len(attempts), 4) if attempts else 0.0,
                "fallback_rate": round(fallback_count / len(attempts), 4) if attempts else 0.0,
                "avg_score": round(statistics.fmean(score_values), 4) if score_values else 0.0,
                "avg_latency_ms": round(statistics.fmean(latency_values), 2) if latency_values else 0.0,
                "p95_latency_ms": self._percentile(latency_values, 95),
                "consistency": round(consistency, 4),
            },
        }

    def _aggregate_profile_metrics(
        self,
        case_results: list[dict[str, Any]],
        repeat_count: int,
    ) -> dict[str, Any]:
        all_attempts: list[dict[str, Any]] = []
        for case_item in case_results:
            all_attempts.extend(case_item.get("attempts") or [])

        if not all_attempts:
            return {
                "generalization_score": 0.0,
                "stability_score": 0.0,
                "final_score": 0.0,
                "success_rate": 0.0,
                "tool_selection_accuracy": 0.0,
                "arg_accuracy": 0.0,
                "avg_latency_ms": 0.0,
                "p95_latency_ms": 0,
                "consistency": 0.0,
                "total_attempts": 0,
                "successful_attempts": 0,
            }

        total_attempts = len(all_attempts)
        successful_attempts = sum(1 for item in all_attempts if item.get("success"))
        error_attempts = sum(1 for item in all_attempts if item.get("error_message"))
        fallback_attempts = sum(1 for item in all_attempts if item.get("fallback_used"))
        success_rate = successful_attempts / total_attempts

        tool_acc = statistics.fmean([1.0 if item.get("tool_match") else 0.0 for item in all_attempts])
        arg_acc = statistics.fmean(
            [
                (0.6 * float(item.get("arg_required_score", 0.0))) + (0.4 * float(item.get("arg_value_score", 0.0)))
                for item in all_attempts
            ]
        )
        avg_case_score = statistics.fmean([float(item.get("case_score", 0.0)) for item in all_attempts])

        latencies = [int(item.get("latency_ms", 0)) for item in all_attempts]
        avg_latency = statistics.fmean(latencies)
        p95_latency = self._percentile(latencies, 95)

        case_consistency = [float(item.get("summary", {}).get("consistency", 0.0)) for item in case_results]
        consistency = statistics.fmean(case_consistency) if case_consistency else 1.0

        # Latency score: auto-detect reasoning model by checking if any attempt
        # returned reasoning_content; if so, relax P95 target from 8s to 30s.
        has_reasoning = any(item.get("reasoning_content") for item in all_attempts)
        latency_target_ms = 30000.0 if has_reasoning else 8000.0
        latency_score = max(0.0, 1.0 - (float(p95_latency) / latency_target_ms))

        generalization_score = avg_case_score * 100.0
        stability_score = (0.4 * success_rate + 0.3 * consistency + 0.3 * latency_score) * 100.0
        final_score = (0.55 * generalization_score) + (0.45 * stability_score)

        # Reasoning token aggregation
        total_reasoning_tokens = sum(int((item.get("usage") or {}).get("reasoning_tokens", 0)) for item in all_attempts)

        return {
            "generalization_score": round(generalization_score, 2),
            "stability_score": round(stability_score, 2),
            "final_score": round(final_score, 2),
            "success_rate": round(success_rate * 100.0, 2),
            "tool_selection_accuracy": round(tool_acc * 100.0, 2),
            "arg_accuracy": round(arg_acc * 100.0, 2),
            "avg_latency_ms": round(avg_latency, 2),
            "p95_latency_ms": int(p95_latency),
            "consistency": round(consistency * 100.0, 2),
            "total_attempts": total_attempts,
            "successful_attempts": successful_attempts,
            "error_attempts": error_attempts,
            "error_rate": round(error_attempts / total_attempts * 100.0, 2),
            "fallback_attempts": fallback_attempts,
            "fallback_rate": round(fallback_attempts / total_attempts * 100.0, 2),
            "repeat_count": int(repeat_count),
            "latency_target_ms": int(latency_target_ms),
            "has_reasoning": has_reasoning,
            "total_reasoning_tokens": total_reasoning_tokens,
        }

    def _should_fallback_to_text_mode(self, exc: Exception) -> bool:
        hint_tokens = {
            "tool",
            "tools",
            "tool_choice",
            "function",
            "functions",
            "function_call",
            "unsupported",
            "not support",
            "not supported",
            "invalid_parameter",
        }

        message = self._build_exception_text(exc).lower()
        if any(token in message for token in hint_tokens):
            return True

        if isinstance(exc, httpx.HTTPStatusError):
            status_code = int(exc.response.status_code)
            if status_code in {400, 404, 405, 422}:
                response_text = (exc.response.text or "").lower()
                if any(token in response_text for token in hint_tokens):
                    return True

        return False

    @staticmethod
    def _build_exception_text(exc: Exception) -> str:
        if isinstance(exc, httpx.HTTPStatusError):
            status_code = exc.response.status_code
            response_text = (exc.response.text or "").strip()
            trimmed = response_text[:600]
            if trimmed:
                return f"HTTP {status_code}: {trimmed}"
            return f"HTTP {status_code}: {exc!s}"
        return str(exc)

    # Reasoning model helpers

    @staticmethod
    def _build_reasoning_kwargs(profile: RuntimeProfile) -> dict[str, Any]:
        """Build extra kwargs for reasoning model parameters."""
        kwargs: dict[str, Any] = {}
        if profile.reasoning_effort:
            kwargs["reasoning_effort"] = profile.reasoning_effort
        if profile.max_completion_tokens is not None:
            kwargs["max_completion_tokens"] = profile.max_completion_tokens
        return kwargs

    @staticmethod
    def _extract_reasoning_content(response: Any) -> str | None:
        """Extract reasoning/thinking chain from the first choice message.

        Providers may use ``reasoning_content`` (DeepSeek, Qwen) or
        ``reasoning`` (some OpenAI-compatible gateways) on the message object.
        """
        choices = getattr(response, "choices", None) or []
        if not choices:
            return None
        first_choice = choices[0]
        message: Any
        if isinstance(first_choice, dict):
            message = first_choice.get("message", {})
        else:
            message = getattr(first_choice, "message", None)

        if isinstance(message, dict):
            rc = message.get("reasoning_content") or message.get("reasoning")
        else:
            rc = getattr(message, "reasoning_content", None) or getattr(message, "reasoning", None)

        if rc and isinstance(rc, str) and rc.strip():
            return rc.strip()
        return None

    def _build_ranking(self, profiles: list[dict[str, Any]]) -> list[dict[str, Any]]:
        ranked_rows: list[dict[str, Any]] = []
        for profile in profiles:
            metrics = profile.get("metrics") or {}
            if not metrics:
                continue
            ranked_rows.append(
                {
                    "profile_id": profile.get("profile_id"),
                    "name": profile.get("name"),
                    "model": profile.get("model"),
                    "final_score": float(metrics.get("final_score", 0.0)),
                    "generalization_score": float(metrics.get("generalization_score", 0.0)),
                    "stability_score": float(metrics.get("stability_score", 0.0)),
                    "success_rate": float(metrics.get("success_rate", 0.0)),
                    "error_rate": float(metrics.get("error_rate", 0.0)),
                    "fallback_rate": float(metrics.get("fallback_rate", 0.0)),
                    "p95_latency_ms": int(metrics.get("p95_latency_ms", 0)),
                    "has_reasoning": bool(metrics.get("has_reasoning", False)),
                    "total_reasoning_tokens": int(metrics.get("total_reasoning_tokens", 0)),
                }
            )

        ranked_rows.sort(key=lambda item: item["final_score"], reverse=True)
        for index, row in enumerate(ranked_rows):
            row["rank"] = index + 1
        return ranked_rows

    def _build_compare_payload(self, left: dict[str, Any], right: dict[str, Any]) -> dict[str, Any]:
        left_metrics = left.get("metrics") or {}
        right_metrics = right.get("metrics") or {}

        numeric_keys = [
            "final_score",
            "generalization_score",
            "stability_score",
            "success_rate",
            "error_rate",
            "fallback_rate",
            "tool_selection_accuracy",
            "arg_accuracy",
            "avg_latency_ms",
            "p95_latency_ms",
            "consistency",
        ]

        deltas: dict[str, Any] = {}
        for key in numeric_keys:
            left_value = float(left_metrics.get(key, 0.0) or 0.0)
            right_value = float(right_metrics.get(key, 0.0) or 0.0)
            deltas[key] = round(right_value - left_value, 2)

        left_case_map = {
            item["case_id"]: item
            for item in (left.get("cases") or [])
            if isinstance(item, dict) and item.get("case_id")
        }
        right_case_map = {
            item["case_id"]: item
            for item in (right.get("cases") or [])
            if isinstance(item, dict) and item.get("case_id")
        }
        case_ids = sorted(set(left_case_map.keys()) | set(right_case_map.keys()))

        case_deltas: list[dict[str, Any]] = []
        regressions: list[str] = []
        improvements: list[str] = []

        for case_id in case_ids:
            left_summary = left_case_map.get(case_id, {}).get("summary", {})
            right_summary = right_case_map.get(case_id, {}).get("summary", {})

            left_score = float(left_summary.get("avg_score", 0.0))
            right_score = float(right_summary.get("avg_score", 0.0))
            left_success = float(left_summary.get("success_rate", 0.0))
            right_success = float(right_summary.get("success_rate", 0.0))

            case_deltas.append(
                {
                    "case_id": case_id,
                    "left_avg_score": round(left_score, 4),
                    "right_avg_score": round(right_score, 4),
                    "delta_score": round(right_score - left_score, 4),
                    "left_success_rate": round(left_success * 100.0, 2),
                    "right_success_rate": round(right_success * 100.0, 2),
                }
            )

            if left_success > 0 and right_success <= 0:
                regressions.append(case_id)
            if right_success > 0 and left_success <= 0:
                improvements.append(case_id)

        return {
            "left": {
                "profile_id": left.get("profile_id"),
                "name": left.get("name"),
                "model": left.get("model"),
                "metrics": left_metrics,
            },
            "right": {
                "profile_id": right.get("profile_id"),
                "name": right.get("name"),
                "model": right.get("model"),
                "metrics": right_metrics,
            },
            "metric_deltas": deltas,
            "case_deltas": case_deltas,
            "regression_cases": regressions,
            "improvement_cases": improvements,
        }

    def _extract_tool_call_response(
        self,
        response: Any,
    ) -> tuple[str | None, dict[str, Any], str]:
        first_choice = None
        choices = getattr(response, "choices", None) or []
        if choices:
            first_choice = choices[0]

        message: Any
        if isinstance(first_choice, dict):
            message = first_choice.get("message", {})
        else:
            message = getattr(first_choice, "message", None)

        if isinstance(message, dict):
            content = str(message.get("content") or "")
            tool_calls = message.get("tool_calls") or []
        else:
            content = str(getattr(message, "content", "") or "")
            tool_calls = getattr(message, "tool_calls", []) or []

        if tool_calls:
            first_call = tool_calls[0]
            if isinstance(first_call, dict):
                function_block = first_call.get("function", {})
                tool_name = str(function_block.get("name") or "").strip() or None
                arguments_raw = function_block.get("arguments")
            else:
                function_obj = getattr(first_call, "function", None)
                tool_name = str(getattr(function_obj, "name", "") or "").strip() or None
                arguments_raw = getattr(function_obj, "arguments", "{}")

            parsed_arguments = self._parse_arguments(arguments_raw)
            return tool_name, parsed_arguments, content

        parsed_from_text = self._parse_json_like_text(content)
        text_tool = str(parsed_from_text.get("tool") or "").strip() or None
        text_arguments = parsed_from_text.get("arguments")
        if text_tool and isinstance(text_arguments, dict):
            return text_tool, text_arguments, content

        return None, {}, content

    def _extract_response_content(self, response: Any) -> str:
        choices = getattr(response, "choices", None) or []
        if not choices:
            return ""
        first_choice = choices[0]
        if isinstance(first_choice, dict):
            return str(first_choice.get("message", {}).get("content") or "")
        message = getattr(first_choice, "message", None)
        return str(getattr(message, "content", "") or "")

    def _extract_usage(self, response: Any) -> dict[str, int]:
        usage = getattr(response, "usage", None)
        if not isinstance(usage, dict):
            return {
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "total_tokens": 0,
                "reasoning_tokens": 0,
            }

        # reasoning_tokens may be at top level or nested in completion_tokens_details
        reasoning_tokens = int(usage.get("reasoning_tokens", 0) or 0)
        if not reasoning_tokens:
            details = usage.get("completion_tokens_details")
            if isinstance(details, dict):
                reasoning_tokens = int(details.get("reasoning_tokens", 0) or 0)

        return {
            "prompt_tokens": int(usage.get("prompt_tokens", 0) or 0),
            "completion_tokens": int(usage.get("completion_tokens", 0) or 0),
            "total_tokens": int(usage.get("total_tokens", 0) or 0),
            "reasoning_tokens": reasoning_tokens,
        }

    def _parse_arguments(self, raw_arguments: Any) -> dict[str, Any]:
        if raw_arguments is None:
            return {}
        if isinstance(raw_arguments, dict):
            return raw_arguments
        if isinstance(raw_arguments, str):
            raw_text = raw_arguments.strip()
            if not raw_text:
                return {}
            try:
                parsed = json.loads(raw_text)
                return parsed if isinstance(parsed, dict) else {}
            except json.JSONDecodeError:
                return {}
        return {}

    def _parse_json_like_text(self, content: str) -> dict[str, Any]:
        raw = (content or "").strip()
        if not raw:
            return {}

        try:
            parsed = json.loads(raw)
            return parsed if isinstance(parsed, dict) else {}
        except json.JSONDecodeError:
            pass

        start = raw.find("{")
        end = raw.rfind("}")
        if start == -1 or end == -1 or end <= start:
            return {}

        candidate = raw[start : end + 1]
        try:
            parsed = json.loads(candidate)
            return parsed if isinstance(parsed, dict) else {}
        except json.JSONDecodeError:
            return {}

    def _build_signature(
        self,
        case: EvalCaseDefinition,
        observed_tool: str | None,
        observed_arguments: dict[str, Any],
    ) -> str:
        if not observed_tool:
            return "__no_tool__"

        signature_keys = sorted({expectation.key for expectation in case.expectations})
        if not signature_keys:
            signature_keys = sorted(observed_arguments.keys())

        compact_args = {key: observed_arguments.get(key) for key in signature_keys if key in observed_arguments}
        serialized_args = json.dumps(compact_args, ensure_ascii=False, sort_keys=True, default=str)
        return f"{observed_tool}|{serialized_args}"

    def _job_snapshot(self, job: dict[str, Any], include_profiles: bool) -> dict[str, Any]:
        snapshot = {
            "job_id": job.get("job_id"),
            "status": job.get("status"),
            "created_at": job.get("created_at"),
            "started_at": job.get("started_at"),
            "finished_at": job.get("finished_at"),
            "suite": job.get("suite"),
            "options": job.get("options"),
            "progress": job.get("progress"),
            "ranking": job.get("ranking"),
            "error_message": job.get("error_message"),
        }
        if include_profiles:
            snapshot["profiles"] = job.get("profiles")
        else:
            snapshot["profiles"] = [
                {
                    "profile_id": item.get("profile_id"),
                    "name": item.get("name"),
                    "model": item.get("model"),
                    "status": item.get("status"),
                    "progress": item.get("progress"),
                    "metrics": item.get("metrics"),
                    "error_message": item.get("error_message"),
                }
                for item in (job.get("profiles") or [])
            ]
        return snapshot

    async def _safe_close_client(self, client: OpenAIClient) -> None:
        close_fn = getattr(client, "close", None)
        if not callable(close_fn):
            return
        try:
            result = close_fn()
            if asyncio.iscoroutine(result):
                await result
        except Exception as exc:  # noqa: BLE001 - cleanup must not raise
            logger.debug(f"Ignore OpenAIClient close error: {exc}")

    def _mask_api_key(self, value: str) -> str:
        raw = str(value or "").strip()
        if not raw:
            return ""
        if len(raw) <= 10:
            return "*" * len(raw)
        return f"{raw[:4]}{'*' * (len(raw) - 8)}{raw[-4:]}"

    @staticmethod
    def _has_value(value: Any) -> bool:
        if value is None:
            return False
        if isinstance(value, str):
            return bool(value.strip())
        return True

    @staticmethod
    def _percentile(values: list[int], pct: int) -> int:
        if not values:
            return 0
        ordered = sorted(int(item) for item in values)
        if len(ordered) == 1:
            return ordered[0]
        rank = max(0.0, min(100.0, float(pct))) / 100.0 * (len(ordered) - 1)
        lower = math.floor(rank)
        upper = math.ceil(rank)
        if lower == upper:
            return ordered[lower]
        ratio = rank - lower
        value = ordered[lower] + (ordered[upper] - ordered[lower]) * ratio
        return round(value)

    def _prune_jobs_locked(self) -> None:
        if len(self._jobs) <= self._max_retained_jobs:
            return

        sorted_jobs = sorted(
            self._jobs.values(),
            key=lambda item: item.get("created_at") or "",
        )
        removable = len(self._jobs) - self._max_retained_jobs

        removed = 0
        for job in sorted_jobs:
            if removed >= removable:
                break
            job_id = job.get("job_id")
            if not job_id:
                continue
            if job.get("status") in {"running", "pending", "cancelling"}:
                continue

            self._jobs.pop(job_id, None)
            self._runtime_profiles.pop(job_id, None)
            self._cancel_events.pop(job_id, None)
            task = self._tasks.pop(job_id, None)
            if task and not task.done():
                task.cancel()
            removed += 1


__all__ = ["LLMEvalService"]
