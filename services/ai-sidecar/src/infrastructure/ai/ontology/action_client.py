"""Fail-closed HTTP client for Rust-owned ontology action execution.

The sidecar never executes ontology read/advisory actions itself: it
calls the Rust internal endpoints

* ``POST /internal/ai/v1/ontology/actions/read``
* ``POST /internal/ai/v1/ontology/actions/advisory``

which authenticate via Service Identity (path-scoped JWT) and recompute
the requester's permissions from Rust-persisted data (Task F1).

Failure policy (fail-closed):

* missing base URL          → ``OntologyActionClientError`` at construction
* missing JWT secret        → ``OntologyActionClientError`` at first call
* HTTP non-2xx              → ``OntologyActionClientError`` carrying the
  server ``error_code`` (e.g. ``TOOL_ACTOR_PERMISSION_DENIED``,
  ``AI_RUN_NOT_FOUND``)
* transport/timeout failure → ``OntologyActionClientError``

Callers must NOT swallow these errors and substitute stub entities.
"""

from __future__ import annotations

import os
from typing import Any

import httpx

from src.infrastructure.ai.ontology.schema_mirror import _validate_rust_api_url
from src.infrastructure.ai.service_identity import get_jwt_secret
from src.infrastructure.ai.service_identity_issuer import ServiceIdentityIssuer
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

READ_PATH = "/internal/ai/v1/ontology/actions/read"
ADVISORY_PATH = "/internal/ai/v1/ontology/actions/advisory"

#: Read-only SLO target is 500ms; the client timeout is slightly wider.
DEFAULT_TIMEOUT_SECONDS = 2.0

_BASE_URL_ENV_KEYS = ("AI_INTERNAL_API_URL", "RUST_API_BASE_URL", "AI_API_BASE_URL")


class OntologyActionClientError(RuntimeError):
    """Typed failure for ontology action client operations.

    Attributes:
        code: Stable client-side failure code (``no_rust_api_base_url``,
            ``no_service_identity_issuer``, ``transport_error``,
            ``ontology_action_rejected``, ``invalid_response``).
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


class OntologyActionClient:
    """Executes registered ontology read/advisory actions via the Rust API.

    This client only performs read/advisory calls; it never retries and
    never fabricates results. Write-semantics actions are not part of
    this surface — controlled writes go through the proposal path.
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
            raise OntologyActionClientError(
                "no_rust_api_base_url",
                "Rust API base URL is not configured "
                "(set AI_INTERNAL_API_URL / RUST_API_BASE_URL / AI_API_BASE_URL)",
            )
        self._base_url = _validate_rust_api_url(resolved)
        self._issuer = issuer
        self._timeout_seconds = timeout_seconds
        self._owns_client = client is None
        self._client = client or httpx.AsyncClient(timeout=timeout_seconds)

    async def read(
        self,
        *,
        run_id: str,
        action_name: str,
        arguments: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Execute a registered read action for the given run."""
        return await self._execute(READ_PATH, run_id, action_name, arguments)

    async def advisory(
        self,
        *,
        run_id: str,
        action_name: str,
        arguments: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Execute a registered advisory action for the given run."""
        return await self._execute(ADVISORY_PATH, run_id, action_name, arguments)

    async def aclose(self) -> None:
        if self._owns_client:
            await self._client.aclose()

    def _resolve_issuer(self) -> ServiceIdentityIssuer:
        if self._issuer is None:
            try:
                self._issuer = ServiceIdentityIssuer(get_jwt_secret())
            except RuntimeError as exc:
                raise OntologyActionClientError(
                    "no_service_identity_issuer",
                    f"cannot sign service identity token: {exc}",
                ) from exc
        return self._issuer

    async def _execute(
        self,
        path: str,
        run_id: str,
        action_name: str,
        arguments: dict[str, Any] | None,
    ) -> dict[str, Any]:
        issuer = self._resolve_issuer()
        url = f"{self._base_url}{path}"
        payload = {
            "run_id": run_id,
            "action_name": action_name,
            "arguments": arguments or {},
        }
        headers = issuer.headers_for_path(path)
        try:
            response = await self._client.post(
                url,
                json=payload,
                headers=headers,
                timeout=self._timeout_seconds,
            )
        except httpx.HTTPError as exc:
            logger.warning(
                "ontology_action_client_transport_error path=%s action=%s error_type=%s",
                path,
                action_name,
                type(exc).__name__,
            )
            raise OntologyActionClientError(
                "transport_error",
                f"ontology action request failed: {exc}",
            ) from exc

        if response.status_code // 100 != 2:
            error_code: str | None = None
            message = f"ontology action rejected with HTTP {response.status_code}"
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
                "ontology_action_client_rejected path=%s action=%s status=%s error_code=%s",
                path,
                action_name,
                response.status_code,
                error_code,
            )
            raise OntologyActionClientError(
                "ontology_action_rejected",
                message,
                status_code=response.status_code,
                error_code=error_code,
            )

        try:
            result = response.json()
        except ValueError as exc:
            raise OntologyActionClientError(
                "invalid_response",
                "ontology action response was not valid JSON",
                status_code=response.status_code,
            ) from exc
        if not isinstance(result, dict):
            raise OntologyActionClientError(
                "invalid_response",
                "ontology action response was not a JSON object",
                status_code=response.status_code,
            )
        return result


__all__ = [
    "ADVISORY_PATH",
    "DEFAULT_TIMEOUT_SECONDS",
    "READ_PATH",
    "OntologyActionClient",
    "OntologyActionClientError",
]
