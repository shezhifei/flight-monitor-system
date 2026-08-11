"""Tests for the AI runtime lifespan context manager."""

from __future__ import annotations

import asyncio
import contextlib
from unittest.mock import AsyncMock, patch

import pytest


@pytest.fixture(autouse=True)
def _reset_mq_singleton(monkeypatch):
    from src.infrastructure.ai.messaging.mq_runtime_bootstrap import reset_mq_runtime_components

    reset_mq_runtime_components()
    yield
    reset_mq_runtime_components()


@pytest.mark.asyncio
async def test_lifespan_bootstraps_and_starts_poller() -> None:
    from src.infrastructure.ai.ai_runtime_bootstrap import ai_runtime_lifespan

    bootstrap_mock = AsyncMock(return_value=True)
    task = asyncio.create_task(asyncio.sleep(10))
    start_mock = AsyncMock(return_value=task)

    async def _stop():
        task.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await task

    stop_mock = AsyncMock(side_effect=_stop)

    with (
        patch("src.infrastructure.ai.ai_runtime_bootstrap.bootstrap_ai_runtime_from_env", bootstrap_mock),
        patch("src.infrastructure.ai.ai_runtime_bootstrap.start_mq_poller_loop", start_mock),
        patch("src.infrastructure.ai.ai_runtime_bootstrap.stop_mq_poller_loop", stop_mock),
    ):
        async with ai_runtime_lifespan():
            pass

    bootstrap_mock.assert_awaited_once()
    start_mock.assert_awaited_once()
    stop_mock.assert_awaited_once()
    assert task.cancelled() or task.done()


@pytest.mark.asyncio
async def test_lifespan_does_not_crash_when_bootstrap_fails() -> None:
    from src.infrastructure.ai.ai_runtime_bootstrap import ai_runtime_lifespan

    bootstrap_mock = AsyncMock(side_effect=RuntimeError("no db"))
    stop_mock = AsyncMock()

    with (
        patch("src.infrastructure.ai.ai_runtime_bootstrap.bootstrap_ai_runtime_from_env", bootstrap_mock),
        patch("src.infrastructure.ai.ai_runtime_bootstrap.stop_mq_poller_loop", stop_mock),
    ):
        async with ai_runtime_lifespan():
            pass

    bootstrap_mock.assert_awaited_once()
    stop_mock.assert_awaited_once()


@pytest.mark.asyncio
async def test_lifespan_stops_poller_on_shutdown() -> None:
    from src.infrastructure.ai.ai_runtime_bootstrap import ai_runtime_lifespan

    start_mock = AsyncMock(return_value=asyncio.create_task(asyncio.sleep(10)))
    stop_mock = AsyncMock()

    with (
        patch("src.infrastructure.ai.ai_runtime_bootstrap.bootstrap_ai_runtime_from_env", AsyncMock()),
        patch("src.infrastructure.ai.ai_runtime_bootstrap.start_mq_poller_loop", start_mock),
        patch("src.infrastructure.ai.ai_runtime_bootstrap.stop_mq_poller_loop", stop_mock),
    ):
        async with ai_runtime_lifespan():
            pass

    start_mock.assert_awaited_once()
    stop_mock.assert_awaited_once()


@pytest.mark.asyncio
async def test_lifespan_yields_even_when_no_poller() -> None:
    from src.infrastructure.ai.ai_runtime_bootstrap import ai_runtime_lifespan

    start_mock = AsyncMock(return_value=None)
    stop_mock = AsyncMock()

    with (
        patch("src.infrastructure.ai.ai_runtime_bootstrap.bootstrap_ai_runtime_from_env", AsyncMock()),
        patch("src.infrastructure.ai.ai_runtime_bootstrap.start_mq_poller_loop", start_mock),
        patch("src.infrastructure.ai.ai_runtime_bootstrap.stop_mq_poller_loop", stop_mock),
    ):
        entered = False
        async with ai_runtime_lifespan():
            entered = True
        assert entered

    start_mock.assert_awaited_once()
    stop_mock.assert_awaited_once()
