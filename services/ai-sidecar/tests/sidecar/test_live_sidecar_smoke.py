"""Live Python sidecar smoke tests — opt-in via RUN_LIVE_AI_SIDECAR_SMOKE=1.

Starts a real uvicorn process on a random port (127.0.0.1:0) and exercises
the actual HTTP contract with a real HTTP client (httpx / requests).

These tests DO NOT replace TestClient/FakeSidecar tests; they verify the
full process boundary, including uvicorn startup, signal handling, and
real HTTP transport.

Opt-in only:
- Without RUN_LIVE_AI_SIDECAR_SMOKE=1 the module is skipped (pytest skipif).
- Rust mirror: `nl_query::test_live_sidecar_smoke_ai_runtime_contract` is
  `#[ignore]` and returns early when the env var is unset, so
  `cargo test -p fms-api nl_query -- --ignored` does not fail on live smoke.

Usage:
    set RUN_LIVE_AI_SIDECAR_SMOKE=1
    .\\.venv\\Scripts\\python.exe -m pytest tests\\sidecar\\test_live_sidecar_smoke.py -v

Rust live smoke:
    set RUN_LIVE_AI_SIDECAR_SMOKE=1
    cd backend && cargo test -p fms-api live_sidecar_smoke -- --ignored --nocapture
"""

from __future__ import annotations

import os
import signal
import socket
import subprocess
import sys
import time
from collections.abc import Generator
from pathlib import Path
from typing import Any

import jwt
import pytest

# Skip entire module unless opt-in
LIVE_ENABLED = os.environ.get("RUN_LIVE_AI_SIDECAR_SMOKE", "0") == "1"
pytestmark = pytest.mark.skipif(
    not LIVE_ENABLED,
    reason="Set RUN_LIVE_AI_SIDECAR_SMOKE=1 to enable live sidecar smoke tests",
)

_project_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(_project_root))

from src.infrastructure.ai.service_identity import (
    SERVICE_AUDIENCE,
    SERVICE_IDENTITY_HEADER,
    SERVICE_ISSUER,
    SERVICE_SUBJECT,
)


def _find_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _create_token(path: str, secret: str, expired: bool = False) -> str:
    now = int(time.time())
    payload = {
        "iss": SERVICE_ISSUER,
        "sub": SERVICE_SUBJECT,
        "aud": SERVICE_AUDIENCE,
        "iat": now,
        "exp": now - 10 if expired else now + 300,
        "path": path,
    }
    return jwt.encode(payload, secret, algorithm="HS256")


def _valid_envelope_body(**overrides: Any) -> dict[str, Any]:
    base: dict[str, Any] = {
        "contract_version": "ai-runtime.v1",
        "job_id": "job_live_smoke_001",
        "run_id": "run_live_smoke_001",
        "correlation_id": "corr_live_smoke",
        "requester": {"user_id": "user_1", "roles": ["ai:chat"]},
        "ontology": {
            "version": "flight-ops.v1",
            "allowed_object_types": ["Flight"],
            "allowed_actions": ["Flight.add_note"],
            "risk_ceiling": "medium",
        },
        "context": {
            "objects": [
                {
                    "object_type": "Flight",
                    "object_id": "FL123",
                    "data": {"flight_number": "CA1234", "status": "scheduled"},
                }
            ],
            "limits": {},
        },
        "task": {
            "task_type": "nl_query",
            "user_message": "给航班 CA1234 添加备注: Live smoke note",
        },
    }
    base.update(overrides)
    return base


@pytest.fixture(scope="module")
def live_sidecar_url() -> Generator[str, None, None]:
    """Start real uvicorn sidecar on a random port, yield URL, then teardown."""
    port = _find_free_port()
    env = os.environ.copy()
    env["API_HOST"] = "127.0.0.1"
    env["API_PORT"] = str(port)
    env["JWT_SECRET"] = "test-secret-for-live-smoke"
    # Ensure no LLM key so we test degraded path
    env.pop("OPENAI_API_KEY", None)

    entrypoint = _project_root / "scripts" / "host" / "ai_sidecar_entrypoint.py"
    proc = subprocess.Popen(
        [str(_project_root / ".venv" / "Scripts" / "python.exe"), str(entrypoint)],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=str(_project_root),
    )

    base_url = f"http://127.0.0.1:{port}"
    # Wait for sidecar to be ready
    import httpx

    deadline = time.time() + 15
    while time.time() < deadline:
        try:
            resp = httpx.get(f"{base_url}/internal/ai/v1/health", timeout=2)
            if resp.status_code == 200:
                break
        except Exception:  # noqa: BLE001 - health probe intentionally swallows transient errors
            pass
        time.sleep(0.3)
    else:
        proc.kill()
        stdout, stderr = proc.communicate(timeout=5)
        raise RuntimeError(
            f"Live sidecar did not start within 15s on port {port}.\n"
            f"stdout: {stdout.decode()}\nstderr: {stderr.decode()}"
        )

    try:
        yield base_url
    finally:
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)


@pytest.fixture(scope="module")
def http_client():
    import httpx

    with httpx.Client(timeout=10) as client:
        yield client


class TestLiveHealth:
    def test_health_no_auth_returns_200(self, live_sidecar_url: str, http_client):
        resp = http_client.get(f"{live_sidecar_url}/internal/ai/v1/health")
        assert resp.status_code == 200
        data = resp.json()
        assert data["status"] == "healthy"
        assert data["service"] == "ai-runtime"


class TestLiveRunsAuth:
    def test_runs_no_identity_returns_401(self, live_sidecar_url: str, http_client):
        resp = http_client.post(
            f"{live_sidecar_url}/internal/ai/v1/runs",
            json=_valid_envelope_body(),
        )
        assert resp.status_code == 401
        detail = resp.json().get("detail", {})
        assert detail.get("code") == "MISSING_SERVICE_IDENTITY"


class TestLiveRunsSuccess:
    def test_runs_valid_identity_returns_200_succeeded(self, live_sidecar_url: str, http_client):
        secret = "test-secret-for-live-smoke"
        token = _create_token("/internal/ai/v1/runs", secret)
        resp = http_client.post(
            f"{live_sidecar_url}/internal/ai/v1/runs",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["success"] is True
        assert data["status"] == "succeeded"
        assert data["run_id"] == "run_live_smoke_001"
        assert isinstance(data["reasoning_steps"], list)
        assert isinstance(data["evidence"], list)
        assert isinstance(data["proposals"], list)
        assert "metrics" in data

    def test_runs_degraded_without_llm_key(self, live_sidecar_url: str, http_client):
        """No OPENAI_API_KEY -> heuristic degraded success, NOT failure."""
        secret = "test-secret-for-live-smoke"
        token = _create_token("/internal/ai/v1/runs", secret)
        resp = http_client.post(
            f"{live_sidecar_url}/internal/ai/v1/runs",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["success"] is True
        assert data["status"] == "succeeded"
        assert data.get("degraded") is True
        assert data["limitations"]
        assert data["metrics"]["model"] == "heuristic-runtime-v1"


class TestLiveInvalidEnvelope:
    def test_runs_invalid_envelope_returns_failed(self, live_sidecar_url: str, http_client):
        secret = "test-secret-for-live-smoke"
        token = _create_token("/internal/ai/v1/runs", secret)
        body = _valid_envelope_body(run_id="", job_id="")
        resp = http_client.post(
            f"{live_sidecar_url}/internal/ai/v1/runs",
            json=body,
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["success"] is False
        assert data["status"] == "failed"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
