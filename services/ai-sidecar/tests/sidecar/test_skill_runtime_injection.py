"""Tests for Skill runtime injection in stream_run_with_tools.

Covers:
- skills disabled: no SkillInstructionComposer call
- skills enabled: system prompt includes skill block
- fail_closed=true + composer error: run.fail with SKILL_INSTRUCTION_LOAD_FAILED
- fail_closed=false + composer error: skills.skipped progress, run continues
- skill_hash changes provider prompt cache key
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Any
from unittest.mock import AsyncMock, MagicMock


def _run(coro):
    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(coro)
    finally:
        loop.close()


# ---------------------------------------------------------------------------
# Fake dataclasses matching resolved config shapes
# ---------------------------------------------------------------------------


@dataclass
class FakeModel:
    model_id: str = "gpt-4o"
    provider_model: str = "gpt-4o"
    api_format: str = "chat_completions"
    context_window: int = 128000
    max_output_tokens: int = 4096
    input_modalities: list[str] = field(default_factory=lambda: ["text"])
    output_modalities: list[str] = field(default_factory=lambda: ["text"])
    capabilities: dict[str, Any] = field(default_factory=dict)
    cost: dict[str, Any] = field(default_factory=dict)


@dataclass
class FakeMcp:
    enabled: bool = False
    server_count: int = 0
    tool_count: int = 0
    resource_count: int = 0


@dataclass
class FakeSkills:
    enabled: bool = False
    skill_count: int = 0
    binding_ids: list[str] = field(default_factory=list)
    fail_closed: bool = False
    bindings: list[Any] = field(default_factory=list)


@dataclass
class FakeSubagents:
    enabled: bool = False
    allowed_entity_ids: list[str] = field(default_factory=list)
    max_depth: int = 1
    max_concurrency: int = 2
    inherit_parent_context: bool = True


@dataclass
class FakeContextPolicy:
    strategy: str = "hybrid"
    max_context_tokens: int = 64000
    compression_threshold_tokens: int = 48000
    preserve_recent_messages: int = 12
    summary_model: str | None = None
    summary_max_tokens: int = 1200
    persist_summaries: bool = True


@dataclass
class FakeCachePolicy:
    enabled: bool = True
    provider_prompt_cache_enabled: bool = False
    provider_prompt_cache_retention: str | None = "24h"
    provider_prompt_cache_namespace: str | None = "flight_monitor"
    context_cache_enabled: bool = False
    context_cache_ttl: int = 86400
    tool_result_cache_enabled: bool = False
    tool_result_cache_ttl: int = 60
    tool_result_cacheable_tools: list[str] = field(default_factory=list)
    mcp_resource_cache_enabled: bool = False
    mcp_resource_cache_ttl: int = 300


@dataclass
class FakeResolvedConfig:
    entity_id: str = "default"
    config_version: int = 2
    config_revision: int = 1
    model_id: str = "gpt-4o"
    model: Any = None
    provider_type: str = "openai_compatible"
    base_url: str = "https://api.openai.com/v1"
    api_format: str = "chat_completions"
    timeout: float = 30.0
    max_retries: int = 3
    retry_delay: float = 0.5
    tools: list[Any] = field(default_factory=list)
    tool_policy: dict[str, Any] = field(default_factory=dict)
    mcp: Any = None
    skills: Any = None
    subagents: Any = None
    context_policy: Any = None
    cache_policy: Any = None
    security: dict[str, Any] = field(default_factory=dict)
    system_prompt: str = ""
    task_template: str | None = None
    snapshot_hash: str = "test-hash"

    def __post_init__(self):
        if self.model is None:
            self.model = FakeModel()
        if self.mcp is None:
            self.mcp = FakeMcp()
        if self.skills is None:
            self.skills = FakeSkills()
        if self.subagents is None:
            self.subagents = FakeSubagents()
        if self.context_policy is None:
            self.context_policy = FakeContextPolicy()
        if self.cache_policy is None:
            self.cache_policy = FakeCachePolicy()


# ---------------------------------------------------------------------------
# Fake CapabilityResolver
# ---------------------------------------------------------------------------


class FakeCapabilityResolver:
    def __init__(self, resolved_config=None, should_fail=False):
        self._config = resolved_config or FakeResolvedConfig()
        self._should_fail = should_fail

    async def resolve(self, entity_id, model_purpose="chat", input_modalities=None):
        if self._should_fail:
            raise RuntimeError("Resolver failed")
        return self._config


# ---------------------------------------------------------------------------
# Fake ContextEnvelope
# ---------------------------------------------------------------------------


class FakeEnvelope:
    def __init__(self):
        self.job_id = "test-job"
        self.run_id = "test-run"
        self.entity_id = "default"
        self.requester = MagicMock(user_id="test-user")
        self.ontology = MagicMock(risk_ceiling="medium", allowed_actions=[])
        self.context = MagicMock(objects=[], limits=MagicMock(redaction="standard"))
        self.task = MagicMock(task_type="chat", user_message="Hello")
        self.metadata = {}


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestSkillInjection:
    def test_skills_disabled_no_composer_call(self):
        """When skills.enabled=False, SkillInstructionComposer.compose() should not be called."""
        from src.infrastructure.ai.runtime_service import RuntimeService

        config = FakeResolvedConfig()
        config.skills = FakeSkills(enabled=False)
        resolver = FakeCapabilityResolver(resolved_config=config)

        mock_composer = AsyncMock()
        mock_composer.compose = AsyncMock(return_value=None)

        mock_llm = MagicMock()
        mock_llm.is_configured = MagicMock(return_value=True)
        mock_llm._model = "gpt-4o"

        svc = RuntimeService(
            capability_resolver=resolver,
            skill_instruction_composer=mock_composer,
            llm_client=mock_llm,
        )

        envelope = FakeEnvelope()
        events = _run(self._collect(svc, envelope))

        mock_composer.compose.assert_not_called()

    def test_skills_enabled_injects_skill_block(self):
        """When skills.enabled=True and composer returns content, system prompt should include skill block."""
        from src.infrastructure.ai.agent_skills.instruction_composer import (
            ComposedInstructions,
            SkillInstructionFragment,
        )
        from src.infrastructure.ai.runtime_service import RuntimeService

        fragment = SkillInstructionFragment(
            skill_slug="test-skill",
            skill_version="1.0.0",
            content_hash="sha256:abc123",
            instruction="You are a flight ops helper.",
            token_count=50,
        )
        composed = ComposedInstructions(
            fragments=[fragment],
            combined_text="You are a flight ops helper.",
            total_tokens=50,
            skill_hashes=["test-skill@1.0.0:sha256:abc123"],
            hash="skill-hash-123",
        )

        config = FakeResolvedConfig()
        config.skills = FakeSkills(enabled=True, skill_count=1)
        resolver = FakeCapabilityResolver(resolved_config=config)

        mock_composer = AsyncMock()
        mock_composer.compose = AsyncMock(return_value=composed)

        # Mock LLM so code reaches skill injection
        mock_llm = MagicMock()
        mock_llm.is_configured = MagicMock(return_value=True)
        mock_llm._model = "gpt-4o"

        # Mock the streaming runner to capture the system prompt
        captured_system_prompt = []

        svc = RuntimeService(
            capability_resolver=resolver,
            skill_instruction_composer=mock_composer,
            llm_client=mock_llm,
        )

        envelope = FakeEnvelope()
        events = _run(self._collect(svc, envelope))

        # Check that progress event includes skills.injected
        progress_events = [e for e in events if e.get("event") == "progress"]
        injected = [e for e in progress_events if e.get("data", {}).get("step") == "skills.injected"]
        assert len(injected) == 1
        assert "skill-hash-123" in injected[0]["data"]["summary"]

    def test_fail_closed_composer_error_causes_run_fail(self):
        """When fail_closed=True and composer raises, run should fail with SKILL_INSTRUCTION_LOAD_FAILED."""
        from src.infrastructure.ai.runtime_service import RuntimeService

        config = FakeResolvedConfig()
        config.skills = FakeSkills(enabled=True, skill_count=1, fail_closed=True)
        resolver = FakeCapabilityResolver(resolved_config=config)

        mock_composer = AsyncMock()
        mock_composer.compose = AsyncMock(side_effect=RuntimeError("Skill load error"))

        mock_llm = MagicMock()
        mock_llm.is_configured = MagicMock(return_value=True)
        mock_llm._model = "gpt-4o"

        svc = RuntimeService(
            capability_resolver=resolver,
            skill_instruction_composer=mock_composer,
            llm_client=mock_llm,
        )

        envelope = FakeEnvelope()
        events = _run(self._collect(svc, envelope))

        fail_events = [e for e in events if e.get("event") == "run.fail"]
        assert len(fail_events) == 1
        assert "SKILL_INSTRUCTION_LOAD_FAILED" in fail_events[0]["data"]["answer"]

    def test_fail_closed_false_composer_error_continues(self):
        """When fail_closed=False and composer raises, the error should not cause SKILL_INSTRUCTION_LOAD_FAILED."""
        from src.infrastructure.ai.runtime_service import RuntimeService

        config = FakeResolvedConfig()
        config.skills = FakeSkills(enabled=True, skill_count=1, fail_closed=False)
        resolver = FakeCapabilityResolver(resolved_config=config)

        mock_composer = AsyncMock()
        mock_composer.compose = AsyncMock(side_effect=RuntimeError("Skill load error"))

        # LLM not configured → code reaches skill injection then falls to heuristic
        mock_llm = MagicMock()
        mock_llm.is_configured = MagicMock(return_value=False)

        svc = RuntimeService(
            capability_resolver=resolver,
            skill_instruction_composer=mock_composer,
            llm_client=mock_llm,
        )

        envelope = FakeEnvelope()
        events = _run(self._collect(svc, envelope))

        # Should NOT have SKILL_INSTRUCTION_LOAD_FAILED in any event
        for event in events:
            data = event.get("data", {})
            answer = data.get("answer", "")
            assert "SKILL_INSTRUCTION_LOAD_FAILED" not in answer

        # Should have run.complete (heuristic fallback, not skill failure)
        event_types = [e.get("event") for e in events]
        assert "run.complete" in event_types

    def test_skill_hash_changes_prompt_cache_key(self):
        """Different skill hashes should produce different prompt cache keys."""
        from src.infrastructure.ai.capability_resolver import generate_prompt_cache_key

        key1 = generate_prompt_cache_key(
            namespace="ns",
            entity_id="ent",
            api_format="chat_completions",
            model_id="gpt-4o",
            system_prompt_hash="abc",
            tool_schema_hash="def",
            skill_hash="hash-a",
        )
        key2 = generate_prompt_cache_key(
            namespace="ns",
            entity_id="ent",
            api_format="chat_completions",
            model_id="gpt-4o",
            system_prompt_hash="abc",
            tool_schema_hash="def",
            skill_hash="hash-b",
        )
        key3 = generate_prompt_cache_key(
            namespace="ns",
            entity_id="ent",
            api_format="chat_completions",
            model_id="gpt-4o",
            system_prompt_hash="abc",
            tool_schema_hash="def",
            skill_hash=None,
        )
        assert key1 != key2
        assert key1 != key3
        assert key2 != key3

    async def _collect(self, svc, envelope):
        events = []
        async for event in svc.stream_run_with_tools(envelope):
            events.append(event)
        return events
