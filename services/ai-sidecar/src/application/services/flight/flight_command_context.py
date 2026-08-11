"""Flight command context primitives."""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass, field
from enum import StrEnum


class FlightWriteSource(StrEnum):
    API_USER = "api_user"
    EXTERNAL_SYNC = "external_sync"
    SYSTEM_JOB = "system_job"
    MIGRATION = "migration"


@dataclass(frozen=True)
class FlightCommandContext:
    actor_id: str = "system"
    is_admin: bool = False
    permissions: frozenset[str] = field(default_factory=frozenset)
    source: FlightWriteSource = FlightWriteSource.API_USER
    request_id: str | None = None

    @classmethod
    def build(
        cls,
        *,
        actor_id: str,
        is_admin: bool,
        permissions: Iterable[str] | None,
        source: FlightWriteSource = FlightWriteSource.API_USER,
        request_id: str | None = None,
    ) -> FlightCommandContext:
        normalized_actor = str(actor_id or "system").strip() or "system"
        normalized_permissions = frozenset(str(item).strip() for item in (permissions or []) if str(item).strip())
        normalized_request_id = str(request_id).strip() if request_id else None
        return cls(
            actor_id=normalized_actor,
            is_admin=bool(is_admin),
            permissions=normalized_permissions,
            source=source,
            request_id=normalized_request_id,
        )


__all__ = ["FlightCommandContext", "FlightWriteSource"]
