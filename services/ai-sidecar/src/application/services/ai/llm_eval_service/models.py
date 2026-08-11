"""Data models for the LLM evaluation service."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class ArgExpectation:
    key: str
    required: bool = True
    expected: Any | None = None
    contains: str | None = None
    one_of: list[Any] | None = None
    min_value: float | None = None


@dataclass(frozen=True)
class EvalCaseDefinition:
    case_id: str
    prompt: str
    expected_tools: list[str]
    expectations: list[ArgExpectation]
    tags: list[str]
    suites: list[str]
    eval_type: str = "tool_routing"
    expected_behavior: str = "tool_call"


@dataclass(frozen=True)
class RuntimeProfile:
    profile_id: str
    name: str
    base_url: str
    api_key: str
    model: str
    timeout: float
    max_retries: int
    retry_delay: float
    reasoning_effort: str | None = None
    max_completion_tokens: int | None = None
    # Responses API fields
    api_mode: str = "chat"
    instructions: str | None = None
    reasoning_summary: str | None = None
    store: bool | None = None
    include: list[str] | None = None
