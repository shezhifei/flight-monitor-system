"""Read-only solver candidate tool for ``dispatch_ops`` (Task I1).

The dispatch replan solver itself runs deterministically in the browser
(OR-Tools WASM); the agent loop only needs the *model snapshot* the
solver consumes, shaped into grounded candidates. The sidecar fetches
that snapshot from the Rust internal endpoint

* ``POST /internal/ai/v1/dispatch/replan-snapshot``

which authenticates via Service Identity (path-scoped JWT) and
recomputes the requester's ``dispatch:read`` permission from
Rust-persisted data (same trust model as the ontology internal
endpoints, Task F1).

Read-only by construction: the mutating ``replan-apply`` surface stays
user-JWT-only and is never exposed on the agent tool face.

Failure policy (fail-closed, mirrors ``ontology.action_client``):

* missing base URL          → ``SolverCandidateClientError`` at construction
* missing JWT secret        → ``SolverCandidateClientError`` at first call
* HTTP non-2xx              → ``SolverCandidateClientError`` carrying the
  server ``error_code`` (e.g. ``TOOL_ACTOR_PERMISSION_DENIED``,
  ``AI_RUN_NOT_FOUND``)
* transport/timeout failure → ``SolverCandidateClientError``

Callers must NOT swallow these errors and substitute stub candidates.
"""

from __future__ import annotations

import os
from datetime import UTC, datetime
from typing import Any

import httpx

from src.infrastructure.ai.ontology.schema_mirror import _validate_rust_api_url
from src.infrastructure.ai.service_identity import get_jwt_secret
from src.infrastructure.ai.service_identity_issuer import ServiceIdentityIssuer
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

REPLAN_SNAPSHOT_PATH = "/internal/ai/v1/dispatch/replan-snapshot"

#: Agent-facing tool name; the Rust internal face records the same name
#: in the authorization context.
SOLVER_TOOL_NAME = "dispatch.list_solver_candidates"

SOLVER_TOOL_NAMES: tuple[str, ...] = (SOLVER_TOOL_NAME,)

#: Read-only SLO target is 500ms; the client timeout is slightly wider.
DEFAULT_TIMEOUT_SECONDS = 2.0

_BASE_URL_ENV_KEYS = ("AI_INTERNAL_API_URL", "RUST_API_BASE_URL", "AI_API_BASE_URL")

#: Candidate id keys in order of preference, matching the snapshot shapes
#: produced by ``public_replan_snapshot_payload``.
_ORDER_ID_KEYS = ("dispatch_order_id", "order_id", "id")


def is_solver_tool(tool_name: str) -> bool:
    """Check if a tool belongs to the read-only solver candidate surface."""
    return tool_name in SOLVER_TOOL_NAMES


class SolverCandidateClientError(RuntimeError):
    """Typed failure for solver candidate client operations.

    Attributes:
        code: Stable client-side failure code (``no_rust_api_base_url``,
            ``no_service_identity_issuer``, ``transport_error``,
            ``solver_snapshot_rejected``, ``invalid_response``).
        status_code: HTTP status returned by Rust, when available.
        error_code: Server-provided ``error_code`` (e.g.
            ``TOOL_ACTOR_PERMISSION_DENIED``), when available.
    """

    def __init__(
        self,
        code: str,
        message: str,
        *,
        status_code: int | None = None,
        error_code: str | None = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.status_code = status_code
        self.error_code = error_code if error_code is not None else code


def _resolve_rust_api_base_url() -> str | None:
    """Same resolution order as the job worker bootstrap."""
    for key in _BASE_URL_ENV_KEYS:
        value = os.environ.get(key, "").strip()
        if value:
            return value.rstrip("/")
    return None


class SolverCandidateClient:
    """Fetches the deterministic replan snapshot via the Rust internal API.

    Read-only by construction: this client never retries, never writes,
    and never fabricates results.
    """

    def __init__(
        self,
        *,
        base_url: str | None = None,
        issuer: ServiceIdentityIssuer | None = None,
        client: httpx.AsyncClient | None = None,
        timeout_seconds: float = DEFAULT_TIMEOUT_SECONDS,
    ) -> None:
        resolved = (base_url or "").strip() or _resolve_rust_api_base_url()
        if not resolved:
            raise SolverCandidateClientError(
                "no_rust_api_base_url",
                "Rust API base URL is not configured (set AI_INTERNAL_API_URL / RUST_API_BASE_URL / AI_API_BASE_URL)",
            )
        self._base_url = _validate_rust_api_url(resolved)
        self._issuer = issuer
        self._timeout_seconds = timeout_seconds
        self._owns_client = client is None
        self._client = client or httpx.AsyncClient(timeout=timeout_seconds)

    async def replan_snapshot(
        self,
        *,
        run_id: str,
        window_start: str,
        window_end: str,
        strategy: str | None = None,
        order_ids: list[str] | None = None,
    ) -> dict[str, Any]:
        """Fetch the replan snapshot covering ``[window_start, window_end]``."""
        issuer = self._resolve_issuer()
        url = f"{self._base_url}{REPLAN_SNAPSHOT_PATH}"
        payload: dict[str, Any] = {
            "run_id": run_id,
            "window_start": window_start,
            "window_end": window_end,
        }
        if strategy:
            payload["strategy"] = strategy
        if order_ids:
            payload["order_ids"] = list(order_ids)
        headers = issuer.headers_for_path(REPLAN_SNAPSHOT_PATH)
        try:
            response = await self._client.post(
                url,
                json=payload,
                headers=headers,
                timeout=self._timeout_seconds,
            )
        except httpx.HTTPError as exc:
            logger.warning(
                "solver_candidate_client_transport_error path=%s error_type=%s",
                REPLAN_SNAPSHOT_PATH,
                type(exc).__name__,
            )
            raise SolverCandidateClientError(
                "transport_error",
                f"solver snapshot request failed: {exc}",
            ) from exc

        if response.status_code // 100 != 2:
            error_code: str | None = None
            message = f"solver snapshot rejected with HTTP {response.status_code}"
            try:
                body = response.json()
                if isinstance(body, dict):
                    error_code = body.get("error_code")
                    server_message = body.get("error")
                    if isinstance(server_message, str) and server_message:
                        message = server_message
            except ValueError:
                pass
            logger.warning(
                "solver_candidate_client_rejected path=%s status=%s error_code=%s",
                REPLAN_SNAPSHOT_PATH,
                response.status_code,
                error_code,
            )
            raise SolverCandidateClientError(
                "solver_snapshot_rejected",
                message,
                status_code=response.status_code,
                error_code=error_code,
            )

        try:
            result = response.json()
        except ValueError as exc:
            raise SolverCandidateClientError(
                "invalid_response",
                "solver snapshot response was not valid JSON",
                status_code=response.status_code,
            ) from exc
        if not isinstance(result, dict):
            raise SolverCandidateClientError(
                "invalid_response",
                "solver snapshot response was not a JSON object",
                status_code=response.status_code,
            )
        return result

    async def aclose(self) -> None:
        if self._owns_client:
            await self._client.aclose()

    def _resolve_issuer(self) -> ServiceIdentityIssuer:
        if self._issuer is None:
            try:
                self._issuer = ServiceIdentityIssuer(get_jwt_secret())
            except RuntimeError as exc:
                raise SolverCandidateClientError(
                    "no_service_identity_issuer",
                    f"cannot sign service identity token: {exc}",
                ) from exc
        return self._issuer


def _order_object_id(order: dict[str, Any]) -> str:
    for key in _ORDER_ID_KEYS:
        value = order.get(key)
        if value:
            return str(value)
    return ""


class SolverTools:
    """Shapes the Rust replan snapshot into grounded solver candidates.

    Candidates are the snapshot's ``orders`` (falling back to
    ``optimizable_orders``), each carrying ``source`` / ``object_id`` /
    ``as_of`` so grounding and freshness hooks can see provenance.
    """

    def __init__(self, *, client: Any) -> None:
        self._client = client

    async def list_solver_candidates(
        self,
        *,
        run_id: str,
        window_start: str,
        window_end: str,
        strategy: str | None = None,
        order_ids: list[str] | None = None,
    ) -> dict[str, Any]:
        payload = await self._client.replan_snapshot(
            run_id=run_id,
            window_start=window_start,
            window_end=window_end,
            strategy=strategy,
            order_ids=order_ids,
        )
        as_of = datetime.now(UTC).isoformat()

        orders = payload.get("orders")
        if not isinstance(orders, list) or not orders:
            fallback = payload.get("optimizable_orders")
            orders = fallback if isinstance(fallback, list) else []

        wanted = set(order_ids) if order_ids else None
        candidates: list[dict[str, Any]] = []
        for order in orders:
            if not isinstance(order, dict):
                continue
            object_id = _order_object_id(order)
            if wanted is not None and object_id not in wanted:
                continue
            candidate = dict(order)
            candidate["object_id"] = object_id
            candidate["source"] = SOLVER_TOOL_NAME
            candidate["as_of"] = as_of
            candidates.append(candidate)

        return {
            "source": SOLVER_TOOL_NAME,
            "as_of": as_of,
            "snapshot_id": payload.get("snapshot_id"),
            "window": {"start": window_start, "end": window_end},
            "strategy": strategy,
            "candidates": candidates,
            "evidence": {
                "source": SOLVER_TOOL_NAME,
                "object_id": payload.get("snapshot_id") or "",
                "as_of": as_of,
            },
        }


SOLVER_TOOL_DEFINITIONS: list[dict[str, Any]] = [
    {
        "name": SOLVER_TOOL_NAME,
        "description": (
            "List deterministic solver candidates for dispatch replanning over a time "
            "window. Read-only: returns the replan model snapshot (grounded candidates "
            "with object_id/as_of) that the browser solver consumes; it never applies "
            "any change."
        ),
        "parameters": {
            "type": "object",
            "properties": {
                "window_start": {
                    "type": "string",
                    "description": "ISO-8601 UTC start of the dispatch window",
                },
                "window_end": {
                    "type": "string",
                    "description": "ISO-8601 UTC end of the dispatch window (after window_start)",
                },
                "strategy": {
                    "type": "string",
                    "enum": ["stability", "balanced", "efficiency"],
                    "description": "Replan strategy preference; defaults to balanced",
                },
                "order_ids": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Optional filter narrowing candidates to these dispatch order ids",
                },
            },
            "required": ["window_start", "window_end"],
        },
        "category": "dispatch",
        "operation_level": "l0_read",
        "risk_level": "low",
        "cacheable": False,
        "side_effect": False,
    }
]


__all__ = [
    "DEFAULT_TIMEOUT_SECONDS",
    "REPLAN_SNAPSHOT_PATH",
    "SOLVER_TOOL_DEFINITIONS",
    "SOLVER_TOOL_NAME",
    "SOLVER_TOOL_NAMES",
    "SolverCandidateClient",
    "SolverCandidateClientError",
    "SolverTools",
    "is_solver_tool",
]
