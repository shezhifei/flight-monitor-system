"""Tests for ServiceIdentityIssuer (JWT token generator).

Verifies that tokens issued by the Python side are accepted by the
Rust ``ServiceIdentity`` middleware (via the existing
``decode_service_identity`` validator). This is the critical contract
test for service-to-service authentication.
"""

from __future__ import annotations

import os
import time

import jwt
import pytest

from src.infrastructure.ai.service_identity import (
    SERVICE_AUDIENCE,
    SERVICE_IDENTITY_HEADER,
    SERVICE_IDENTITY_TTL_SECONDS,
    SERVICE_ISSUER,
    SERVICE_SUBJECT,
    PathMismatchError,
    decode_service_identity,
)
from src.infrastructure.ai.service_identity_issuer import ServiceIdentityIssuer

TEST_SECRET = "test-secret-for-issuer-unit-tests-32b"


@pytest.fixture()
def issuer() -> ServiceIdentityIssuer:
    return ServiceIdentityIssuer(TEST_SECRET)


class TestIssueForPath:
    """Tests for ``ServiceIdentityIssuer.issue_for_path``."""

    def test_generates_valid_hs256_token(self, issuer: ServiceIdentityIssuer) -> None:
        path = "/internal/ai/v1/jobs/lease"
        token = issuer.issue_for_path(path)
        assert isinstance(token, str)
        # HS256 tokens start with eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9
        header = jwt.get_unverified_header(token)
        assert header["alg"] == "HS256"

    def test_token_passes_existing_validator(self, issuer: ServiceIdentityIssuer) -> None:
        """Critical contract: tokens issued here must pass the Rust-side validator."""
        path = "/internal/ai/v1/jobs/lease"
        token = issuer.issue_for_path(path)
        claims = decode_service_identity(token, TEST_SECRET, path)
        assert claims.iss == SERVICE_ISSUER
        assert claims.sub == SERVICE_SUBJECT
        assert claims.aud == SERVICE_AUDIENCE
        assert claims.path == path

    def test_path_mismatch_rejected(self, issuer: ServiceIdentityIssuer) -> None:
        """Security constraint: token path must exactly match request path."""
        token = issuer.issue_for_path("/internal/ai/v1/jobs/lease")
        with pytest.raises(PathMismatchError):
            decode_service_identity(
                token, TEST_SECRET, "/internal/ai/v1/jobs/different"
            )

    def test_different_paths_produce_different_tokens(
        self, issuer: ServiceIdentityIssuer
    ) -> None:
        token1 = issuer.issue_for_path("/internal/ai/v1/jobs/lease")
        token2 = issuer.issue_for_path("/internal/ai/v1/jobs/abc/heartbeat")
        assert token1 != token2

    def test_ttl_is_set_correctly(self, issuer: ServiceIdentityIssuer) -> None:
        before = int(time.time())
        token = issuer.issue_for_path("/test")
        after = int(time.time())
        claims = decode_service_identity(token, TEST_SECRET, "/test")
        assert claims.iat >= before
        assert claims.iat <= after
        assert claims.exp == claims.iat + SERVICE_IDENTITY_TTL_SECONDS

    def test_custom_ttl(self) -> None:
        custom_issuer = ServiceIdentityIssuer(TEST_SECRET, ttl_seconds=120)
        token = custom_issuer.issue_for_path("/test")
        claims = decode_service_identity(token, TEST_SECRET, "/test")
        assert claims.exp == claims.iat + 120

    def test_dynamic_paths_with_job_id(self, issuer: ServiceIdentityIssuer) -> None:
        """Tokens for dynamic paths (with job_id/run_id) must validate correctly."""
        job_id = "01HXYZABCDEF123456789"
        path = f"/internal/ai/v1/jobs/{job_id}/heartbeat"
        token = issuer.issue_for_path(path)
        claims = decode_service_identity(token, TEST_SECRET, path)
        assert claims.path == path

    def test_all_six_api_endpoints_validate(self, issuer: ServiceIdentityIssuer) -> None:
        """All six internal AI API paths must produce valid tokens."""
        paths = [
            "/internal/ai/v1/jobs/lease",
            "/internal/ai/v1/jobs/job-123/heartbeat",
            "/internal/ai/v1/jobs/job-123/runs",
            "/internal/ai/v1/runs/run-456/events",
            "/internal/ai/v1/runs/run-456/complete",
            "/internal/ai/v1/runs/run-456/fail",
        ]
        for path in paths:
            token = issuer.issue_for_path(path)
            claims = decode_service_identity(token, TEST_SECRET, path)
            assert claims.path == path


class TestHeadersForPath:
    """Tests for ``ServiceIdentityIssuer.headers_for_path``."""

    def test_returns_correct_header_name(
        self, issuer: ServiceIdentityIssuer
    ) -> None:
        headers = issuer.headers_for_path("/test")
        assert SERVICE_IDENTITY_HEADER in headers

    def test_header_value_is_valid_token(
        self, issuer: ServiceIdentityIssuer
    ) -> None:
        path = "/internal/ai/v1/jobs/lease"
        headers = issuer.headers_for_path(path)
        token = headers[SERVICE_IDENTITY_HEADER]
        claims = decode_service_identity(token, TEST_SECRET, path)
        assert claims.path == path

    def test_no_unexpected_headers(self, issuer: ServiceIdentityIssuer) -> None:
        headers = issuer.headers_for_path("/test")
        assert set(headers.keys()) == {SERVICE_IDENTITY_HEADER}

    def test_shared_secret_with_validator(self) -> None:
        """The issuer and validator must share the same JWT_SECRET."""
        os.environ["JWT_SECRET"] = TEST_SECRET
        from src.infrastructure.ai.service_identity import get_jwt_secret

        shared_secret = get_jwt_secret()
        issuer = ServiceIdentityIssuer(shared_secret)
        token = issuer.issue_for_path("/test")
        # Validator uses the same shared secret from env
        claims = decode_service_identity(token, shared_secret, "/test")
        assert claims.path == "/test"
