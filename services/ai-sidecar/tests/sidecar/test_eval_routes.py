"""Eval Lab HTTP contract (Task G5).

The Rust control plane proxies ``/api/v2/ai/eval/*`` to the sidecar's
``/internal/ai/v1/eval/*`` router. These tests lock the contract over a fake
pool: service-identity auth, persistent create/list/detail/cancel envelopes,
and the fail-closed 503 when no Postgres pool is available.
"""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from src.infrastructure.ai import eval_routes
from src.infrastructure.ai.service_identity_issuer import ServiceIdentityIssuer

_SECRET = "eval-routes-test-secret-0123456789abcdef"
_BASE = "/internal/ai/v1/eval"


# ---------------------------------------------------------------------------
# Fake pool over an in-memory store
# ---------------------------------------------------------------------------


class _Store:
    def __init__(self) -> None:
        self.jobs: dict[Any, dict[str, Any]] = {}
        self.gates: list[dict[str, Any]] = []


class _FakeConn:
    def __init__(self, store: _Store):
        self._store = store

    async def execute(self, query: str, *args) -> str:
        if "INSERT INTO ai_eval_jobs" in query:
            self._store.jobs[args[0]] = {
                "job_id": args[0],
                "name": args[1],
                "description": args[2],
                "dataset_path": args[3],
                "metrics_config": args[4],
                "status": args[5],
                "progress_percent": args[6],
                "total_runs": args[7],
                "completed_runs": args[8],
                "created_at": datetime.now(UTC),
                "started_at": None,
                "completed_at": None,
                "error_message": None,
            }
        elif "INSERT INTO ai_eval_metrics_summary" in query:
            self._store.gates.append(
                {"job_id": args[0], "metric_name": args[1], "value": args[2],
                 "threshold": args[3], "status": args[4]}
            )
        return "OK"

    async def fetchrow(self, query: str, *args):
        if "UPDATE ai_eval_jobs" in query:
            job = self._store.jobs.get(args[0])
            if job is not None and job["status"] in ("pending", "running"):
                job["status"] = "failed"
                job["completed_at"] = datetime.now(UTC)
                job["error_message"] = "cancelled by user"
                return job
            return None
        if "FROM ai_eval_jobs" in query:
            return self._store.jobs.get(args[0])
        return None

    async def fetch(self, query: str, *args) -> list:
        if "ai_eval_metrics_summary" in query:
            return [gate for gate in self._store.gates if gate["job_id"] == args[0]]
        if "FROM ai_eval_jobs" in query:
            rows = sorted(self._store.jobs.values(), key=lambda job: job["created_at"], reverse=True)
            return rows[: int(args[0])]
        return []


class _FakePool:
    def __init__(self, store: _Store):
        self._store = store

    def acquire(self):
        pool = self

        class _Ctx:
            async def __aenter__(self):
                return _FakeConn(pool._store)

            async def __aexit__(self, *exc):
                return False

        return _Ctx()


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def store() -> _Store:
    return _Store()


@pytest.fixture
def client(monkeypatch, store: _Store) -> TestClient:
    monkeypatch.setenv("JWT_SECRET", _SECRET)
    monkeypatch.setattr(eval_routes, "_resolve_pool", lambda: _FakePool(store))
    monkeypatch.setattr(eval_routes, "_resolve_runner", lambda: None)
    app = FastAPI()
    app.include_router(eval_routes.router)
    return TestClient(app)


def _headers(path: str) -> dict[str, str]:
    return ServiceIdentityIssuer(_SECRET).headers_for_path(path)


# ---------------------------------------------------------------------------
# Auth
# ---------------------------------------------------------------------------


def test_missing_service_identity_is_rejected(client: TestClient) -> None:
    response = client.get(f"{_BASE}/jobs")
    assert response.status_code == 401


def test_wrong_path_token_is_rejected(client: TestClient) -> None:
    response = client.get(f"{_BASE}/jobs", headers=_headers("/internal/ai/v1/other"))
    assert response.status_code == 403


# ---------------------------------------------------------------------------
# Create / list / detail / cancel over the persistent tables
# ---------------------------------------------------------------------------


def test_create_list_detail_cancel_cycle(client: TestClient) -> None:
    created = client.post(
        f"{_BASE}/jobs",
        headers=_headers(f"{_BASE}/jobs"),
        json={
            "name": "query-ops gate run",
            "dataset_path": "docs/fixtures/agent_query_ops_eval.jsonl",
            "metrics_config": {"tool_accuracy_min": 0.95},
        },
    )
    assert created.status_code == 201
    payload = created.json()
    assert payload["success"] is True
    job_id = payload["data"]["job_id"]
    assert payload["data"]["status"] == "pending"

    listed = client.get(f"{_BASE}/jobs?limit=10", headers=_headers(f"{_BASE}/jobs"))
    assert listed.status_code == 200
    items = listed.json()["data"]["items"]
    assert [item["job_id"] for item in items] == [job_id]
    assert items[0]["dataset_path"] == "docs/fixtures/agent_query_ops_eval.jsonl"

    detail = client.get(f"{_BASE}/jobs/{job_id}", headers=_headers(f"{_BASE}/jobs/{job_id}"))
    assert detail.status_code == 200
    detail_data = detail.json()["data"]
    assert detail_data["status"] == "pending"
    assert detail_data["metrics_config"] == {"tool_accuracy_min": 0.95}
    assert detail_data["gates"] == []

    cancel_path = f"{_BASE}/jobs/{job_id}/cancel"
    cancelled = client.post(cancel_path, headers=_headers(cancel_path))
    assert cancelled.status_code == 200
    assert cancelled.json()["data"]["status"] == "failed"

    # Second cancel: the job is no longer active.
    recancel = client.post(cancel_path, headers=_headers(cancel_path))
    assert recancel.status_code == 409


def test_run_without_dataset_is_rejected(client: TestClient) -> None:
    response = client.post(
        f"{_BASE}/jobs",
        headers=_headers(f"{_BASE}/jobs"),
        json={"name": "no-dataset", "run": True},
    )
    assert response.status_code == 400
    assert response.json()["code"] == "DATASET_REQUIRED"


def test_unknown_job_returns_404_and_bad_uuid_422(client: TestClient) -> None:
    missing = client.get(
        f"{_BASE}/jobs/00000000-0000-0000-0000-000000000000",
        headers=_headers(f"{_BASE}/jobs/00000000-0000-0000-0000-000000000000"),
    )
    assert missing.status_code == 404

    invalid = client.get(f"{_BASE}/jobs/not-a-uuid", headers=_headers(f"{_BASE}/jobs/not-a-uuid"))
    assert invalid.status_code == 422


# ---------------------------------------------------------------------------
# Fail-closed without a pool
# ---------------------------------------------------------------------------


def test_missing_pool_answers_503(monkeypatch) -> None:
    monkeypatch.setenv("JWT_SECRET", _SECRET)
    monkeypatch.setattr(eval_routes, "_resolve_pool", lambda: None)
    monkeypatch.setattr(eval_routes, "_resolve_runner", lambda: None)
    app = FastAPI()
    app.include_router(eval_routes.router)
    degraded_client = TestClient(app)

    response = degraded_client.get(f"{_BASE}/jobs", headers=_headers(f"{_BASE}/jobs"))
    assert response.status_code == 503
    assert response.json()["code"] == "EVAL_DB_UNAVAILABLE"
