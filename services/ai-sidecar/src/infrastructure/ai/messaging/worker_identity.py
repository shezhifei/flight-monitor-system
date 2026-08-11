"""Worker identity for multi-process Python sidecar consumers."""

from __future__ import annotations

import os
import socket


def _new_ulid() -> str:
    try:
        import ulid

        new_fn = getattr(ulid, "new", None)
        if callable(new_fn):
            return str(new_fn())
        ulid_cls = getattr(ulid, "ULID", None)
        if ulid_cls is not None:
            return str(ulid_cls())
    except Exception:  # pragma: no cover - optional dep guard  # noqa: BLE001
        pass
    import uuid

    return uuid.uuid4().hex[:26]


class WorkerIdentity:
    """Unique identity for a Python sidecar worker process.

    The identity is read from the ``WORKER_ID`` environment variable when
    present. Otherwise a deterministic identifier is generated from the
    hostname, process id and a ULID.
    """

    def __init__(self, worker_id: str | None = None) -> None:
        self._worker_id = worker_id or self._resolve()

    @property
    def worker_id(self) -> str:
        return self._worker_id

    def __str__(self) -> str:
        return self._worker_id

    def __repr__(self) -> str:
        return f"WorkerIdentity({self._worker_id!r})"

    @classmethod
    def _resolve(cls) -> str:
        env_id = os.environ.get("WORKER_ID", "").strip()
        if env_id:
            return env_id
        hostname = socket.gethostname()
        pid = os.getpid()
        return f"worker-{hostname}-{pid}-{_new_ulid()}"


__all__ = ["WorkerIdentity"]
