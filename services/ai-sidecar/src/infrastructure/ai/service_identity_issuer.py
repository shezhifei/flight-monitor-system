"""Service Identity JWT issuer for outbound requests to the Rust internal AI API.

Mirrors the Rust ``build_service_identity`` function (see
``services/api-server/crates/api/src/services/python_sidecar_proxy.rs``)
to sign HS256 JWT tokens. The Rust ``ServiceIdentity`` middleware
validates these tokens on every ``/internal/ai/v1/*`` request.

HS256 only — matches the Rust ``Header::default()`` implementation.
For asymmetric algorithms (RS256/ES256), both sides must be upgraded
together; the validator in :mod:`src.infrastructure.ai.service_identity`
already supports ``JWT_ALGORITHM``.

The ``path`` claim must **exactly** match ``req.path()`` on the Rust
side (no query string, no trailing slash). Each request path is
different, so a fresh token is issued per request. HS256 signing is
HMAC-SHA256, which is microsecond-grade — no caching is needed.
"""

from __future__ import annotations

import time

import jwt

from src.infrastructure.ai.service_identity import (
    SERVICE_AUDIENCE,
    SERVICE_IDENTITY_HEADER,
    SERVICE_IDENTITY_TTL_SECONDS,
    SERVICE_ISSUER,
    SERVICE_SUBJECT,
)
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class ServiceIdentityIssuer:
    """Issues HS256-signed JWT tokens for Rust internal API authentication.

    The issuer is stateless and safe to share across concurrent tasks.
    A new token is generated for each ``path`` on every call.
    """

    def __init__(
        self,
        secret: str,
        *,
        ttl_seconds: int = SERVICE_IDENTITY_TTL_SECONDS,
    ) -> None:
        self._secret = secret
        self._ttl_seconds = ttl_seconds

    def issue_for_path(self, path: str) -> str:
        """Sign a JWT token valid for the given request path.

        Args:
            path: The exact request path (e.g. ``/internal/ai/v1/jobs/lease``).
                Must match ``req.path()`` on the Rust side.
        """
        now = int(time.time())
        payload = {
            "iss": SERVICE_ISSUER,
            "sub": SERVICE_SUBJECT,
            "aud": SERVICE_AUDIENCE,
            "iat": now,
            "exp": now + self._ttl_seconds,
            "path": path,
        }
        return jwt.encode(payload, self._secret, algorithm="HS256")

    def headers_for_path(self, path: str) -> dict[str, str]:
        """Return HTTP headers with a fresh ``X-Service-Identity`` token."""
        return {SERVICE_IDENTITY_HEADER: self.issue_for_path(path)}


__all__ = ["ServiceIdentityIssuer"]
