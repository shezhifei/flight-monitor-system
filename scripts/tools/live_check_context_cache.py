"""Live integration check for Track A: context-cache transcript reuse against a
REAL Redis instance.

Drives the real ``RuntimeService.stream_run_with_tools`` with a real
``AiCacheManager`` backed by a real ``redis.asyncio`` client. Only the LLM stream
runner is stubbed (no OPENAI key in this env) and the capability resolver returns a
config with the context cache enabled. This exercises the actual production code
path my Track A change touches — assembly, post-response transcript write, and
next-turn read+inject — against real Redis, which the unit tests could only fake.

Run: ./.venv/Scripts/python.exe scripts/tools/live_check_context_cache.py
Requires: Redis reachable at REDIS_URL (default redis://localhost:6379/0).
"""

from __future__ import annotations

import asyncio
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SIDE = ROOT / "services" / "ai-sidecar"
sys.path.insert(0, str(SIDE))
sys.path.insert(0, str(SIDE / "tests" / "sidecar"))

from test_skill_runtime_injection import FakeCachePolicy, FakeResolvedConfig  # noqa: E402

from src.infrastructure.ai.cache_manager import AiCacheManager  # noqa: E402
from src.infrastructure.ai.context_envelope import (  # noqa: E402
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.runtime_service import RuntimeService  # noqa: E402


@dataclass
class _FakeResult:
    text: str
    model: str = "gpt-4o"
    usage: Optional[Dict[str, Any]] = None


@dataclass
class _FakeEvent:
    type: str
    result: Optional[_FakeResult] = None
    text_delta: Optional[str] = None
    tool_call: Optional[Dict[str, Any]] = None


def _runner_factory(capture: Dict[str, Any], answer: str):
    class _FakeRunner:
        def __init__(self, *a, **k):
            pass

        async def stream_chat_with_tools(self, *, messages, **kwargs):
            capture["messages"] = messages
            yield _FakeEvent(type="completed", result=_FakeResult(text=answer))

    return _FakeRunner


class _Resolver:
    def __init__(self, cfg):
        self._cfg = cfg

    async def resolve(self, entity_id, model_purpose="chat", input_modalities=None):
        return self._cfg


class _LLM:
    _model = "gpt-4o"

    def is_configured(self):
        return True


def _envelope(msg: str, conv: str) -> ContextEnvelope:
    return ContextEnvelope(
        job_id="live-job",
        run_id=f"live-{msg}",
        correlation_id=conv,
        requester=EnvelopeRequester(user_id="live-user"),
        ontology=EnvelopeOntology(),
        context=EnvelopeContext(),
        task=EnvelopeTask(task_type="chat", user_message=msg),
    )


def _roles(messages) -> List[str]:
    return [getattr(m.role, "value", m.role) for m in messages]


async def _drive(svc, env, capture, answer):
    factory = _runner_factory(capture, answer)
    with patch("src.infrastructure.ai.runtime_service.LLMStreamRunner", factory):
        async for _ in svc.stream_run_with_tools(env):
            pass


async def main() -> int:
    redis_url = os.environ.get("REDIS_URL", "redis://localhost:6379/0")
    import redis.asyncio as redis

    client = redis.from_url(redis_url, decode_responses=True)
    pong = await client.ping()
    print(f"[infra] Redis {redis_url} ping -> {pong}")

    conv = "live-conv-trackA"
    key = f"ai:context:default:{conv}"
    await client.delete(key)  # clean slate

    cache = AiCacheManager(redis_client=client)
    cfg = FakeResolvedConfig()
    cfg.cache_policy = FakeCachePolicy(enabled=True, context_cache_enabled=True)
    svc = RuntimeService(
        capability_resolver=_Resolver(cfg),
        llm_client=_LLM(),
        cache_manager=cache,
    )

    failures = []

    # Turn 1: no prior; should write a real transcript [user, assistant] to Redis.
    cap1: Dict[str, Any] = {}
    await _drive(svc, _envelope("Q1", conv), cap1, answer="A1")
    t1_roles = _roles(cap1.get("messages", []))
    print(f"[turn1] assembled roles = {t1_roles}")
    if t1_roles != ["system", "user"]:
        failures.append(f"turn1 expected [system,user], got {t1_roles}")

    raw = await client.get(key)
    print(f"[redis] {key} = {raw}")
    if not raw:
        failures.append("turn1 did NOT persist a transcript to real Redis")
    else:
        import json
        stored = json.loads(raw)["messages"]
        roles = [m["role"] for m in stored]
        print(f"[redis] stored roles = {roles}")
        if "system" in roles:
            failures.append(f"stored transcript leaked system role: {roles}")
        if roles != ["user", "assistant"]:
            failures.append(f"stored transcript expected [user,assistant], got {roles}")
        if stored and stored[-1].get("content") != "A1":
            failures.append("stored assistant turn missing the answer text")

    # Turn 2: same conversation; should read turn1 transcript from Redis and inject it.
    cap2: Dict[str, Any] = {}
    await _drive(svc, _envelope("Q2", conv), cap2, answer="A2")
    t2_roles = _roles(cap2.get("messages", []))
    t2_contents = [m.content for m in cap2.get("messages", [])]
    print(f"[turn2] assembled roles = {t2_roles}")
    print(f"[turn2] assembled contents = {t2_contents}")
    if t2_roles != ["system", "user", "assistant", "user"]:
        failures.append(f"turn2 expected [system,user,assistant,user], got {t2_roles}")
    if t2_contents[1:] != ["Q1", "A1", "Q2"]:
        failures.append(f"turn2 prior not injected from Redis: {t2_contents[1:]}")

    await client.delete(key)
    await client.aclose()

    if failures:
        print("\nLIVE CHECK 1 FAILED:")
        for f in failures:
            print("  - " + f)
        return 1
    print("\nLIVE CHECK 1 PASSED: Track A context-cache reuse verified against real Redis.")
    return 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
