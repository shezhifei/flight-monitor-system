"""Tests for the async AI job worker (ADR-0004).

Tests cover:
* ``AiJobWorkerConfig`` path helpers and URL construction.
* ``AiJobWorkerError`` status_code propagation.
* ``is_terminal_status`` classification.
* Bootstrap degrade-closed behavior when config is missing.
* Worker 409 handling (run already terminal).
"""

from __future__ import annotations

import asyncio
import os
from unittest.mock import AsyncMock, MagicMock, patch

import httpx
import pytest

from src.infrastructure.ai.messaging.ai_job_worker import (
    AiJobWorker,
    AiJobWorkerConfig,
    AiJobWorkerError,
    is_terminal_status,
)
from src.infrastructure.ai.messaging.ai_job_worker_bootstrap import (
    build_ai_job_worker_from_env,
    reset_ai_job_worker,
)
from src.infrastructure.ai.service_identity_issuer import ServiceIdentityIssuer

TEST_SECRET = "test-secret-for-worker-unit-tests-32b"


@pytest.fixture(autouse=True)
def _reset_worker_singleton():
    """Ensure each test starts with a clean bootstrap singleton."""
    reset_ai_job_worker()
    yield
    reset_ai_job_worker()


@pytest.fixture()
def config() -> AiJobWorkerConfig:
    return AiJobWorkerConfig(
        base_url="http://localhost:8080",
        worker_id="test-worker",
    )


@pytest.fixture()
def issuer() -> ServiceIdentityIssuer:
    return ServiceIdentityIssuer(TEST_SECRET)


class TestAiJobWorkerConfig:
    """Tests for ``AiJobWorkerConfig`` path helpers."""

    def test_lease_path_constant(self) -> None:
        assert AiJobWorkerConfig.LEASE_PATH == "/internal/ai/v1/jobs/lease"

    def test_heartbeat_path(self, config: AiJobWorkerConfig) -> None:
        assert config.heartbeat_path("job-123") == "/internal/ai/v1/jobs/job-123/heartbeat"

    def test_runs_path(self, config: AiJobWorkerConfig) -> None:
        assert config.runs_path("job-123") == "/internal/ai/v1/jobs/job-123/runs"

    def test_events_path(self, config: AiJobWorkerConfig) -> None:
        assert config.events_path("run-456") == "/internal/ai/v1/runs/run-456/events"

    def test_complete_path(self, config: AiJobWorkerConfig) -> None:
        assert config.complete_path("run-456") == "/internal/ai/v1/runs/run-456/complete"

    def test_fail_path(self, config: AiJobWorkerConfig) -> None:
        assert config.fail_path("run-456") == "/internal/ai/v1/runs/run-456/fail"

    def test_url_strips_trailing_slash(self) -> None:
        config = AiJobWorkerConfig(
            base_url="http://localhost:8080/",
            worker_id="w",
        )
        assert config.url("/internal/ai/v1/jobs/lease") == "http://localhost:8080/internal/ai/v1/jobs/lease"

    def test_url_no_trailing_slash(self, config: AiJobWorkerConfig) -> None:
        assert config.url("/test") == "http://localhost:8080/test"

    def test_frozen(self, config: AiJobWorkerConfig) -> None:
        """Config must be immutable (frozen dataclass)."""
        with pytest.raises(AttributeError):
            config.worker_id = "other"

    def test_lease_path_is_classvar(self) -> None:
        """LEASE_PATH is a ClassVar, not an instance field."""
        import dataclasses
        fields = {f.name for f in dataclasses.fields(AiJobWorkerConfig)}
        assert "LEASE_PATH" not in fields


class TestIsTerminalStatus:
    """Tests for ``is_terminal_status``."""

    @pytest.mark.parametrize("status", ["succeeded", "failed_terminal", "cancelled"])
    def test_terminal_statuses(self, status: str) -> None:
        assert is_terminal_status(status) is True

    @pytest.mark.parametrize("status", ["pending", "running", "claimed", "", "SUCCEEDED"])
    def test_non_terminal_statuses(self, status: str) -> None:
        assert is_terminal_status(status) is False


class TestAiJobWorkerError:
    """Tests for ``AiJobWorkerError``."""

    def test_status_code_default_none(self) -> None:
        err = AiJobWorkerError("transport error")
        assert err.status_code is None

    def test_status_code_set(self) -> None:
        err = AiJobWorkerError("HTTP_409", status_code=409)
        assert err.status_code == 409

    def test_message_preserved(self) -> None:
        err = AiJobWorkerError("HTTP_500: internal", status_code=500)
        assert "HTTP_500" in str(err)


class TestBootstrapDegradeClosed:
    """Tests for bootstrap degrade-closed behavior."""

    def test_returns_none_when_base_url_missing(self, monkeypatch: pytest.MonkeyPatch) -> None:
        for key in ("AI_INTERNAL_API_URL", "RUST_API_BASE_URL", "AI_API_BASE_URL"):
            monkeypatch.delenv(key, raising=False)
        monkeypatch.setenv("JWT_SECRET", TEST_SECRET)
        assert build_ai_job_worker_from_env() is None

    def test_returns_none_when_jwt_secret_missing(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setenv("AI_INTERNAL_API_URL", "http://localhost:8080")
        for key in ("JWT_SECRET", "JWT_SECRET_KEY"):
            monkeypatch.delenv(key, raising=False)
        # Also need to clear the lru_cache on get_jwt_secret
        from src.infrastructure.ai.service_identity import get_jwt_secret
        get_jwt_secret.cache_clear()
        assert build_ai_job_worker_from_env() is None
        get_jwt_secret.cache_clear()

    def test_returns_worker_when_configured(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setenv("AI_INTERNAL_API_URL", "http://localhost:8080")
        monkeypatch.setenv("JWT_SECRET", TEST_SECRET)
        monkeypatch.setenv("WORKER_ID", "test-worker")
        from src.infrastructure.ai.service_identity import get_jwt_secret
        get_jwt_secret.cache_clear()
        worker = build_ai_job_worker_from_env()
        assert worker is not None
        assert worker.config.worker_id == "test-worker"
        assert worker.config.base_url == "http://localhost:8080"
        get_jwt_secret.cache_clear()

    def test_singleton_caches_worker(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setenv("AI_INTERNAL_API_URL", "http://localhost:8080")
        monkeypatch.setenv("JWT_SECRET", TEST_SECRET)
        monkeypatch.setenv("WORKER_ID", "test-worker")
        from src.infrastructure.ai.service_identity import get_jwt_secret
        get_jwt_secret.cache_clear()
        worker1 = build_ai_job_worker_from_env()
        worker2 = build_ai_job_worker_from_env()
        assert worker1 is worker2
        get_jwt_secret.cache_clear()


class TestWorkerRequestHandling:
    """Tests for worker HTTP request and error handling logic."""

    @pytest.fixture()
    def worker(
        self, config: AiJobWorkerConfig, issuer: ServiceIdentityIssuer
    ) -> AiJobWorker:
        return AiJobWorker(config, issuer)

    @pytest.mark.asyncio
    async def test_complete_run_handles_409_as_success(
        self, worker: AiJobWorker
    ) -> None:
        """409 from complete_run means run is already terminal — treat as success."""
        mock_response = MagicMock()
        mock_response.status_code = 409
        mock_response.text = "already terminal"
        mock_response.content = b'{"error": "already terminal"}'

        with patch.object(
            worker._client, "request", new=AsyncMock(return_value=mock_response)
        ):
            # Should not raise — 409 is handled internally
            await worker._complete_run("run-1", {"answer": "test"})

    @pytest.mark.asyncio
    async def test_fail_run_handles_409_as_success(
        self, worker: AiJobWorker
    ) -> None:
        """409 from fail_run means run is already terminal — treat as success."""
        mock_response = MagicMock()
        mock_response.status_code = 409
        mock_response.text = "already terminal"
        mock_response.content = b'{"error": "already terminal"}'

        with patch.object(
            worker._client, "request", new=AsyncMock(return_value=mock_response)
        ):
            await worker._fail_run("run-1", "ERROR", "test error")

    @pytest.mark.asyncio
    async def test_forward_event_swallows_errors(self, worker: AiJobWorker) -> None:
        """Event forwarding failures should not propagate (best-effort)."""
        with patch.object(
            worker, "_request", new=AsyncMock(side_effect=AiJobWorkerError("HTTP_500"))
        ):
            # Should not raise
            await worker._forward_event("run-1", "progress", {"step": "test"})

    @pytest.mark.asyncio
    async def test_lease_returns_none_when_no_job(self, worker: AiJobWorker) -> None:
        """When data is null (no pending job), return None."""
        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.content = b'{"success": true, "data": null}'
        mock_response.json.return_value = {"success": True, "data": None}

        with patch.object(
            worker._client, "request", new=AsyncMock(return_value=mock_response)
        ):
            result = await worker._lease_one_job()
            assert result is None

    @pytest.mark.asyncio
    async def test_lease_returns_job_when_available(self, worker: AiJobWorker) -> None:
        job = {"job_id": "job-1", "status": "claimed"}
        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.content = b'{"success": true, "data": {"job_id": "job-1"}}'
        mock_response.json.return_value = {"success": True, "data": job}

        with patch.object(
            worker._client, "request", new=AsyncMock(return_value=mock_response)
        ):
            result = await worker._lease_one_job()
            assert result == job

    @pytest.mark.asyncio
    async def test_shutdown_stops_lease_loop(self, worker: AiJobWorker) -> None:
        """The lease loop should exit when shutdown event is set."""
        shutdown = asyncio.Event()
        shutdown.set()  # Pre-set shutdown

        # The loop should exit immediately without leasing
        with patch.object(
            worker, "_lease_one_job", new=AsyncMock()
        ) as _mock_lease:
            await worker.run(shutdown)
            # _lease_one_job should never be called because shutdown is already set
            _mock_lease.assert_not_called()

    @pytest.mark.asyncio
    async def test_aclose_closes_owned_client(self, config: AiJobWorkerConfig, issuer: ServiceIdentityIssuer) -> None:
        """aclose should close the HTTP client when owned."""
        worker = AiJobWorker(config, issuer)  # No client injected → owns client
        with patch.object(worker._client, "aclose", new=AsyncMock()) as mock_aclose:
            await worker.aclose()
            mock_aclose.assert_called_once()

    @pytest.mark.asyncio
    async def test_aclose_does_not_close_injected_client(
        self, config: AiJobWorkerConfig, issuer: ServiceIdentityIssuer
    ) -> None:
        """aclose should NOT close the HTTP client when injected (test owns it)."""
        injected_client = httpx.AsyncClient()
        worker = AiJobWorker(config, issuer, client=injected_client)
        with patch.object(injected_client, "aclose", new=AsyncMock()) as mock_aclose:
            await worker.aclose()
            mock_aclose.assert_not_called()
        await injected_client.aclose()
