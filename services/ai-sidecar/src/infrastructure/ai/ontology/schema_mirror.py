from __future__ import annotations

import os
from typing import Any
from urllib.parse import urlsplit

import requests

from src.infrastructure.ai.security.url_guard import (
    _is_loopback_host,
    validate_internal_service_url,
)

RUST_API_ALLOW_INSECURE_HTTP_ENV = "AI_SIDECAR_ALLOW_INSECURE_RUST_API_HTTP"

# 与 Rust `fms_domain::ontology::schema_export::FLIGHT_OPS_ONTOLOGY_VERSION` 保持一致。
FLIGHT_OPS_ONTOLOGY_VERSION = "flight-ops.v1"


def _env_truthy(name: str) -> bool:
    return os.environ.get(name, "").strip().lower() in {"1", "true", "yes", "on"}


def _validate_rust_api_url(rust_api_url: str) -> str:
    parsed = urlsplit(str(rust_api_url or "").strip())
    is_loopback = bool(parsed.hostname) and _is_loopback_host(parsed.hostname)
    return validate_internal_service_url(
        rust_api_url,
        purpose="Rust API base_url",
        allow_loopback=True,
        require_tls=not (is_loopback or _env_truthy(RUST_API_ALLOW_INSECURE_HTTP_ENV)),
    )


class SchemaMirror:
    """Read-only mirror of the ontology *schema* exported by Rust.

    This component ONLY fetches the schema snapshot (object types,
    action signatures) for validation and prompt rendering. It must
    NEVER be used as an action executor: executing registered ontology
    actions goes through ``action_client.OntologyActionClient`` against
    the Rust internal endpoints, which own authorization.
    """

    def __init__(self, rust_api_url: str = "http://localhost:8080"):
        self.rust_api_url = _validate_rust_api_url(rust_api_url)
        self._schema_cache: dict[str, Any] | None = None

    def load_schema_snapshot(self, version: str = FLIGHT_OPS_ONTOLOGY_VERSION) -> dict[str, Any]:
        """Fetch the schema from Rust backend and cache it."""
        response = requests.get(f"{self.rust_api_url}/api/v2/ai/ontology/schema")
        response.raise_for_status()
        self._schema_cache = response.json()
        return self._schema_cache

    def get_cached_schema_snapshot(self) -> dict[str, Any] | None:
        """Return the current snapshot without performing network I/O."""
        return self._schema_cache

    def get_action_schema(self, object_type: str, action_name: str) -> dict[str, Any] | None:
        if not self._schema_cache:
            self.load_schema_snapshot()

        objects = self._schema_cache.get("objects", {})
        obj_def = objects.get(object_type, {})
        actions = obj_def.get("actions", {})
        return actions.get(action_name)


# Default instance
schema_mirror = SchemaMirror()
