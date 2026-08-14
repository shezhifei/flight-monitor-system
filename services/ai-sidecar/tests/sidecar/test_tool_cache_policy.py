"""Tests for Tool result cache policy enforcement.

Covers:
- ToolResultCachePolicy.is_tool_cacheable logic
- disabled: no cache read/write
- enabled but tool not in cacheable_tools: no cache
- enabled + cacheable + hit: skip execute_read_only_tool
- enabled + cacheable + miss: execute tool + write cache with configured TTL
- concurrent runs with different policies don't interfere
- side-effect tool: never cached
"""

from __future__ import annotations

import asyncio
from unittest.mock import AsyncMock, patch

from src.infrastructure.ai.tools.tool_executor import ToolExecutor, ToolResultCachePolicy
from tests.sidecar.tool_executor_test_support import authorized_tool_executor


def _run(coro):
    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(coro)
    finally:
        loop.close()


# ---------------------------------------------------------------------------
# Tests: ToolResultCachePolicy
# ---------------------------------------------------------------------------


class TestToolResultCachePolicy:
    def test_disabled_policy_nothing_cacheable(self):
        policy = ToolResultCachePolicy(enabled=False, cacheable_tools=["flight_status_lookup"])
        assert policy.is_tool_cacheable("flight_status_lookup") is False

    def test_enabled_but_tool_not_in_list(self):
        policy = ToolResultCachePolicy(enabled=True, cacheable_tools=["flight_list_by_date"])
        assert policy.is_tool_cacheable("flight_status_lookup") is False

    def test_enabled_and_tool_in_list(self):
        policy = ToolResultCachePolicy(enabled=True, cacheable_tools=["flight_status_lookup", "weather_at_airport"])
        assert policy.is_tool_cacheable("flight_status_lookup") is True
        assert policy.is_tool_cacheable("weather_at_airport") is True
        assert policy.is_tool_cacheable("other_tool") is False

    def test_enabled_but_none_cacheable_tools(self):
        policy = ToolResultCachePolicy(enabled=True, cacheable_tools=None)
        assert policy.is_tool_cacheable("flight_status_lookup") is False

    def test_enabled_but_empty_cacheable_tools(self):
        policy = ToolResultCachePolicy(enabled=True, cacheable_tools=[])
        assert policy.is_tool_cacheable("flight_status_lookup") is False


# ---------------------------------------------------------------------------
# Tests: cache disabled
# ---------------------------------------------------------------------------


class TestToolCacheDisabled:
    def test_disabled_no_cache_read(self):
        """When cache disabled, get_tool_result should not be called."""
        mock_cache = AsyncMock()
        mock_cache.get_tool_result = AsyncMock(return_value=None)
        mock_cache.set_tool_result = AsyncMock()

        executor = authorized_tool_executor(cache_manager=mock_cache)
        policy = ToolResultCachePolicy(enabled=False, cacheable_tools=["flight_status_lookup"])

        with patch("src.infrastructure.ai.tools.tool_executor.execute_read_only_tool") as mock_exec:
            mock_exec.return_value = {"status": "on_time"}
            result = _run(
                executor.execute(
                    {
                        "tool_call_id": "tc-1",
                        "tool_name": "flight_status_lookup",
                        "arguments": {"flight_id": "CA123"},
                    },
                    "run-1",
                    cache_policy=policy,
                )
            )

        assert result.success is True
        mock_cache.get_tool_result.assert_not_called()
        mock_cache.set_tool_result.assert_not_called()

    def test_enabled_but_tool_not_cacheable_no_cache(self):
        """When tool not in cacheable_tools, cache should not be used."""
        mock_cache = AsyncMock()
        mock_cache.get_tool_result = AsyncMock(return_value=None)
        mock_cache.set_tool_result = AsyncMock()

        executor = ToolExecutor(cache_manager=mock_cache)
        policy = ToolResultCachePolicy(enabled=True, cacheable_tools=["flight_list_by_date"])

        with patch("src.infrastructure.ai.tools.tool_executor.execute_read_only_tool") as mock_exec:
            mock_exec.return_value = {"status": "on_time"}
            result = _run(
                executor.execute(
                    {
                        "tool_call_id": "tc-1",
                        "tool_name": "flight_status_lookup",
                        "arguments": {"flight_id": "CA123"},
                    },
                    "run-1",
                    cache_policy=policy,
                )
            )

        assert result.success is True
        mock_cache.get_tool_result.assert_not_called()
        mock_cache.set_tool_result.assert_not_called()


# ---------------------------------------------------------------------------
# Tests: cache enabled + cacheable
# ---------------------------------------------------------------------------


class TestToolCacheEnabled:
    def test_cache_hit_skips_execution(self):
        """When cache hit, execute_read_only_tool should not be called."""
        mock_cache = AsyncMock()
        mock_cache.get_tool_result = AsyncMock(return_value={"cached": True})

        executor = ToolExecutor(cache_manager=mock_cache)
        policy = ToolResultCachePolicy(enabled=True, cacheable_tools=["flight_status_lookup"])

        with patch("src.infrastructure.ai.tools.tool_executor.execute_read_only_tool") as mock_exec:
            mock_exec.return_value = {"live": True}
            result = _run(
                executor.execute(
                    {
                        "tool_call_id": "tc-1",
                        "tool_name": "flight_status_lookup",
                        "arguments": {"flight_id": "CA123"},
                    },
                    "run-1",
                    cache_policy=policy,
                )
            )

        assert result.success is True
        assert result.result == {"cached": True}
        mock_exec.assert_not_called()
        mock_cache.get_tool_result.assert_called_once()

    def test_cache_miss_executes_and_writes_cache(self):
        """When cache miss, tool executes and result is cached with configured TTL."""
        mock_cache = AsyncMock()
        mock_cache.get_tool_result = AsyncMock(return_value=None)
        mock_cache.set_tool_result = AsyncMock()

        executor = ToolExecutor(cache_manager=mock_cache)
        policy = ToolResultCachePolicy(enabled=True, cacheable_tools=["flight_status_lookup"], ttl=240)

        with patch("src.infrastructure.ai.tools.tool_executor.execute_read_only_tool") as mock_exec:
            mock_exec.return_value = {"flight": "CA123", "status": "on_time"}
            result = _run(
                executor.execute(
                    {
                        "tool_call_id": "tc-1",
                        "tool_name": "flight_status_lookup",
                        "arguments": {"flight_id": "CA123"},
                    },
                    "run-1",
                    cache_policy=policy,
                )
            )

        assert result.success is True
        assert result.result == {"flight": "CA123", "status": "on_time"}
        mock_exec.assert_called_once_with("flight_status_lookup", {"flight_id": "CA123"}, backend=None)
        mock_cache.set_tool_result.assert_called_once_with(
            tool_name="flight_status_lookup",
            args={"flight_id": "CA123"},
            result={"flight": "CA123", "status": "on_time"},
            ttl_seconds=240,  # from policy, not hardcoded
            entity_id=None,
        )

    def test_no_cache_manager_no_error(self):
        """When no cache_manager, tools should work normally without cache."""
        executor = ToolExecutor(cache_manager=None)
        policy = ToolResultCachePolicy(enabled=True, cacheable_tools=["flight_status_lookup"])

        with patch("src.infrastructure.ai.tools.tool_executor.execute_read_only_tool") as mock_exec:
            mock_exec.return_value = {"flight": "CA123"}
            result = _run(
                executor.execute(
                    {
                        "tool_call_id": "tc-1",
                        "tool_name": "flight_status_lookup",
                        "arguments": {"flight_id": "CA123"},
                    },
                    "run-1",
                    cache_policy=policy,
                )
            )

        assert result.success is True

    def test_no_policy_no_cache(self):
        """When cache_policy=None, no caching should occur."""
        mock_cache = AsyncMock()
        mock_cache.get_tool_result = AsyncMock(return_value=None)
        mock_cache.set_tool_result = AsyncMock()

        executor = ToolExecutor(cache_manager=mock_cache)

        with patch("src.infrastructure.ai.tools.tool_executor.execute_read_only_tool") as mock_exec:
            mock_exec.return_value = {"flight": "CA123"}
            result = _run(
                executor.execute(
                    {
                        "tool_call_id": "tc-1",
                        "tool_name": "flight_status_lookup",
                        "arguments": {"flight_id": "CA123"},
                    },
                    "run-1",
                    cache_policy=None,
                )
            )

        assert result.success is True
        mock_cache.get_tool_result.assert_not_called()
        mock_cache.set_tool_result.assert_not_called()


# ---------------------------------------------------------------------------
# Tests: write action never cached
# ---------------------------------------------------------------------------


class TestWriteActionNeverCached:
    def test_write_action_not_cached(self):
        """Write action tools should never be cached (they produce proposals)."""
        mock_cache = AsyncMock()
        mock_cache.get_tool_result = AsyncMock(return_value=None)
        mock_cache.set_tool_result = AsyncMock()

        executor = authorized_tool_executor(cache_manager=mock_cache)
        policy = ToolResultCachePolicy(enabled=True, cacheable_tools=["add_flight_note"])

        result = _run(
            executor.execute(
                {
                    "tool_call_id": "tc-1",
                    "tool_name": "add_flight_note",
                    "arguments": {"flight_id": "CA123", "note": "test"},
                },
                "run-1",
                cache_policy=policy,
            )
        )

        assert result.success is True
        assert result.proposal is not None
        mock_cache.get_tool_result.assert_not_called()
        mock_cache.set_tool_result.assert_not_called()


# ---------------------------------------------------------------------------
# Tests: concurrent runs with different policies
# ---------------------------------------------------------------------------


class TestConcurrentCachePolicy:
    def test_concurrent_different_policies_no_interference(self):
        """Two concurrent runs with different cache policies must not interfere.

        Run A: enabled=True, cacheable_tools=["flight_status_lookup"], ttl=240
        Run B: enabled=False

        Both use the same ToolExecutor instance.
        """
        mock_cache = AsyncMock()
        call_log = []

        async def mock_get_tool_result(tool_name, args, cacheable_tools, ttl_seconds, entity_id=None):
            call_log.append(("get", tool_name, ttl_seconds, entity_id))
            return None  # cache miss

        async def mock_set_tool_result(tool_name, args, result, ttl_seconds, entity_id=None):
            call_log.append(("set", tool_name, ttl_seconds, entity_id))

        mock_cache.get_tool_result = mock_get_tool_result
        mock_cache.set_tool_result = mock_set_tool_result

        executor = ToolExecutor(cache_manager=mock_cache)

        policy_a = ToolResultCachePolicy(enabled=True, cacheable_tools=["flight_status_lookup"], ttl=240)
        policy_b = ToolResultCachePolicy(enabled=False)

        with patch("src.infrastructure.ai.tools.tool_executor.execute_read_only_tool") as mock_exec:
            mock_exec.return_value = {"result": "ok"}

            async def run_a():
                return await executor.execute(
                    {
                        "tool_call_id": "tc-a",
                        "tool_name": "flight_status_lookup",
                        "arguments": {"flight_id": "A1"},
                    },
                    "run-a",
                    cache_policy=policy_a,
                )

            async def run_b():
                return await executor.execute(
                    {
                        "tool_call_id": "tc-b",
                        "tool_name": "flight_status_lookup",
                        "arguments": {"flight_id": "B1"},
                    },
                    "run-b",
                    cache_policy=policy_b,
                )

            async def run_both():
                return await asyncio.gather(run_a(), run_b())

            results = _run(run_both())

        assert all(r.success for r in results)

        # Run A should have used cache (get + set with ttl=240)
        get_a = [c for c in call_log if c[0] == "get" and c[1] == "flight_status_lookup" and c[2] == 240]
        set_a = [c for c in call_log if c[0] == "set" and c[1] == "flight_status_lookup" and c[2] == 240]
        assert len(get_a) == 1
        assert len(set_a) == 1

        # Run B should NOT have used cache (disabled)
        get_b_disabled = [c for c in call_log if c[0] == "get" and c[2] is None]
        set_b_disabled = [c for c in call_log if c[0] == "set" and c[2] is None]
        assert len(get_b_disabled) == 0
        assert len(set_b_disabled) == 0

    def test_concurrent_same_policy_respects_ttl(self):
        """Two concurrent runs with same policy but different TTLs must use their own TTL."""
        mock_cache = AsyncMock()
        call_log = []

        async def mock_get_tool_result(tool_name, args, cacheable_tools, ttl_seconds, entity_id=None):
            call_log.append(("get", tool_name, ttl_seconds, entity_id))
            return None

        async def mock_set_tool_result(tool_name, args, result, ttl_seconds, entity_id=None):
            call_log.append(("set", tool_name, ttl_seconds, entity_id))

        mock_cache.get_tool_result = mock_get_tool_result
        mock_cache.set_tool_result = mock_set_tool_result

        executor = ToolExecutor(cache_manager=mock_cache)

        policy_120 = ToolResultCachePolicy(enabled=True, cacheable_tools=["flight_status_lookup"], ttl=120)
        policy_360 = ToolResultCachePolicy(enabled=True, cacheable_tools=["flight_status_lookup"], ttl=360)

        with patch("src.infrastructure.ai.tools.tool_executor.execute_read_only_tool") as mock_exec:
            mock_exec.return_value = {"result": "ok"}

            async def run_both():
                async def run_120():
                    return await executor.execute(
                        {
                            "tool_call_id": "tc-120",
                            "tool_name": "flight_status_lookup",
                            "arguments": {"flight_id": "b"},
                        },
                        "run-120",
                        cache_policy=policy_120,
                    )

                async def run_360():
                    return await executor.execute(
                        {
                            "tool_call_id": "tc-360",
                            "tool_name": "flight_status_lookup",
                            "arguments": {"flight_id": "a"},
                        },
                        "run-360",
                        cache_policy=policy_360,
                    )

                return await asyncio.gather(run_360(), run_120())

            results = _run(run_both())

        assert all(r.success for r in results)

        # Each should use its own TTL
        set_120 = [c for c in call_log if c[0] == "set" and c[2] == 120]
        set_360 = [c for c in call_log if c[0] == "set" and c[2] == 360]
        assert len(set_120) == 1
        assert len(set_360) == 1
