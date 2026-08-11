"""Tests for the multi-worker worker identity helper."""

from __future__ import annotations

import os
import socket

from src.infrastructure.ai.messaging.worker_identity import WorkerIdentity


class TestWorkerIdentity:
    def test_uses_explicit_worker_id(self, monkeypatch):
        monkeypatch.setenv("WORKER_ID", "stable-worker-1")
        identity = WorkerIdentity()

        assert identity.worker_id == "stable-worker-1"

    def test_uses_env_worker_id(self, monkeypatch):
        monkeypatch.setenv("WORKER_ID", "env-worker-7")
        identity = WorkerIdentity()

        assert identity.worker_id == "env-worker-7"

    def test_empty_env_falls_back_to_generated_id(self, monkeypatch):
        monkeypatch.setenv("WORKER_ID", "")
        identity = WorkerIdentity()

        assert identity.worker_id
        assert identity.worker_id != ""
        assert identity.worker_id.startswith("worker-")

    def test_generated_id_contains_hostname_and_pid(self, monkeypatch):
        monkeypatch.delenv("WORKER_ID", raising=False)
        identity = WorkerIdentity()

        hostname = socket.gethostname()
        pid = os.getpid()
        assert hostname in identity.worker_id
        assert str(pid) in identity.worker_id

    def test_explicit_id_overrides_env(self, monkeypatch):
        monkeypatch.setenv("WORKER_ID", "env-worker")
        identity = WorkerIdentity("explicit-worker")

        assert identity.worker_id == "explicit-worker"

    def test_string_representation(self, monkeypatch):
        monkeypatch.setenv("WORKER_ID", "repr-worker")
        identity = WorkerIdentity()

        assert str(identity) == "repr-worker"
        assert "repr-worker" in repr(identity)
