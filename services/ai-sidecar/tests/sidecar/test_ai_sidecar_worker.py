"""Tests for the standalone AI sidecar worker entrypoint."""

from __future__ import annotations

import asyncio
import signal
from contextlib import asynccontextmanager
from typing import Any
from unittest.mock import patch

import pytest

from scripts.host.ai_sidecar_worker import _run_worker, main


@pytest.mark.asyncio
async def test_run_worker_awaits_shutdown_event() -> None:
    shutdown_event = asyncio.Event()
    entered = False
    exited = False

    @asynccontextmanager
    async def _fake_lifespan():
        nonlocal entered, exited
        entered = True
        try:
            yield
        finally:
            exited = True

    with patch("scripts.host.ai_sidecar_worker.ai_runtime_lifespan", _fake_lifespan):
        task = asyncio.create_task(_run_worker(shutdown_event))
        await asyncio.sleep(0.05)
        assert entered is True
        shutdown_event.set()
        await asyncio.wait_for(task, timeout=1.0)
    assert exited is True


@pytest.mark.asyncio
async def test_run_worker_uses_signal_when_no_event_provided() -> None:
    @asynccontextmanager
    async def _fake_lifespan():
        try:
            yield
        finally:
            pass

    registered: dict[int, Any] = {}

    def _patch_signal(sig, handler):
        registered[sig] = handler
        if sig == signal.SIGTERM:
            # Simulate signal arrival by invoking the registered handler.
            handler(signal.SIGTERM, None)
        return handler

    with (
        patch("scripts.host.ai_sidecar_worker.ai_runtime_lifespan", _fake_lifespan),
        patch("scripts.host.ai_sidecar_worker.signal.signal", side_effect=_patch_signal),
    ):
        task = asyncio.create_task(_run_worker())
        await asyncio.wait_for(task, timeout=1.0)

    assert signal.SIGTERM in registered


def test_main_returns_zero_on_keyboard_interrupt() -> None:
    with patch(
        "scripts.host.ai_sidecar_worker._run_worker",
        side_effect=KeyboardInterrupt(),
    ):
        assert main([]) == 0


def test_main_returns_one_on_fatal_error() -> None:
    with patch(
        "scripts.host.ai_sidecar_worker._run_worker",
        side_effect=RuntimeError("boom"),
    ):
        assert main([]) == 1
