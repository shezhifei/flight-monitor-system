"""Tests for Service Identity Authentication Module."""

from __future__ import annotations

import os
import time
from unittest.mock import MagicMock

import jwt
import pytest
from fastapi.testclient import TestClient

from src.infrastructure.ai.service_identity import (
    ALLOWED_HEALTH_PATHS,
    SERVICE_AUDIENCE,
    SERVICE_IDENTITY_HEADER,
    SERVICE_IDENTITY_TTL_SECONDS,
    SERVICE_ISSUER,
    SERVICE_SUBJECT,
    ExpiredTokenError,
    InvalidTokenError,
    PathMismatchError,
    ServiceIdentityClaims,
    decode_service_identity,
    extract_service_identity_from_request,
    require_service_identity,
)
from tests.sidecar.canonical_entrypoint import app

os.environ["JWT_SECRET"] = "test-secret-for-unit-tests"


class TestServiceIdentityClaims:
    def test_claims_model_validation(self):
        now = int(time.time())
        claims = ServiceIdentityClaims(
            iss=SERVICE_ISSUER,
            sub=SERVICE_SUBJECT,
            aud=SERVICE_AUDIENCE,
            iat=now,
            exp=now + 60,
            path="/internal/ai/v1/runs",
        )
        assert claims.iss == SERVICE_ISSUER
        assert claims.sub == SERVICE_SUBJECT
        assert claims.aud == SERVICE_AUDIENCE
        assert claims.path == "/internal/ai/v1/runs"


class TestDecodeServiceIdentity:
    def setup_method(self):
        self.secret = "test-secret-for-unit-tests"
        self.now = int(time.time())

    def _create_token(self, **overrides):
        payload = {
            "iss": SERVICE_ISSUER,
            "sub": SERVICE_SUBJECT,
            "aud": SERVICE_AUDIENCE,
            "iat": self.now,
            "exp": self.now + SERVICE_IDENTITY_TTL_SECONDS,
            "path": "/internal/ai/v1/runs",
        }
        payload.update(overrides)
        return jwt.encode(payload, self.secret, algorithm="HS256")

    def test_valid_token_passes(self):
        token = self._create_token()
        claims = decode_service_identity(token, self.secret, "/internal/ai/v1/runs")
        assert claims.iss == SERVICE_ISSUER
        assert claims.sub == SERVICE_SUBJECT
        assert claims.aud == SERVICE_AUDIENCE
        assert claims.path == "/internal/ai/v1/runs"

    def test_expired_token_raises(self):
        token = self._create_token(
            iat=self.now - SERVICE_IDENTITY_TTL_SECONDS - 100,
            exp=self.now - 100,
        )
        with pytest.raises(ExpiredTokenError):
            decode_service_identity(token, self.secret, "/internal/ai/v1/runs")

    def test_invalid_audience_raises(self):
        token = self._create_token(aud="wrong-audience")
        with pytest.raises(InvalidTokenError) as exc_info:
            decode_service_identity(token, self.secret, "/internal/ai/v1/runs")
        assert "Invalid audience" in str(exc_info.value)

    def test_invalid_issuer_raises(self):
        token = self._create_token(iss="wrong-issuer")
        with pytest.raises(InvalidTokenError) as exc_info:
            decode_service_identity(token, self.secret, "/internal/ai/v1/runs")
        assert "Invalid issuer" in str(exc_info.value)

    def test_invalid_subject_raises(self):
        token = self._create_token(sub="wrong-subject")
        with pytest.raises(InvalidTokenError) as exc_info:
            decode_service_identity(token, self.secret, "/internal/ai/v1/runs")
        assert "Invalid subject" in str(exc_info.value)

    def test_path_mismatch_raises(self):
        token = self._create_token(path="/internal/ai/v1/runs")
        with pytest.raises(PathMismatchError):
            decode_service_identity(token, self.secret, "/different/path")

    def test_invalid_signature_raises(self):
        token = self._create_token()
        with pytest.raises(InvalidTokenError) as exc_info:
            decode_service_identity(token, "wrong-secret", "/internal/ai/v1/runs")
        assert "Invalid signature" in str(exc_info.value)

    def test_malformed_token_raises(self):
        with pytest.raises(InvalidTokenError):
            decode_service_identity("not.a.valid.jwt.token", self.secret, "/internal/ai/v1/runs")

    def test_completely_wrong_token_raises(self):
        with pytest.raises(InvalidTokenError):
            decode_service_identity("completely-wrong", self.secret, "/internal/ai/v1/runs")


class TestExtractServiceIdentity:
    def test_extracts_header(self):
        mock_request = MagicMock()
        mock_request.headers = {SERVICE_IDENTITY_HEADER: "test-token"}
        token = extract_service_identity_from_request(mock_request)
        assert token == "test-token"

    def test_returns_none_when_missing(self):
        mock_request = MagicMock()
        mock_request.headers = {}
        token = extract_service_identity_from_request(mock_request)
        assert token is None


class TestAllowedHealthPaths:
    def test_health_path_in_allowed_list(self):
        assert "/internal/ai/v1/health" in ALLOWED_HEALTH_PATHS

    def test_runs_path_not_in_allowed_list(self):
        assert "/internal/ai/v1/runs" not in ALLOWED_HEALTH_PATHS


class TestRequireServiceIdentity:
    def setup_method(self):
        self.secret = "test-secret-for-unit-tests"
        self.now = int(time.time())

    def _create_token(self, path: str, **overrides):
        payload = {
            "iss": SERVICE_ISSUER,
            "sub": SERVICE_SUBJECT,
            "aud": SERVICE_AUDIENCE,
            "iat": self.now,
            "exp": self.now + SERVICE_IDENTITY_TTL_SECONDS,
            "path": path,
        }
        payload.update(overrides)
        return jwt.encode(payload, self.secret, algorithm="HS256")

    def test_health_path_allows_no_token(self):
        from fastapi import HTTPException

        mock_request = MagicMock()
        mock_request.url.path = "/internal/ai/v1/health"
        mock_request.headers = {}

        try:
            claims = require_service_identity(mock_request)
            assert claims.path == "/internal/ai/v1/health"
        except HTTPException:
            pytest.fail("Health path should not require service identity")

    def test_protected_path_rejects_missing_token(self):
        from fastapi import HTTPException

        mock_request = MagicMock()
        mock_request.url.path = "/internal/ai/v1/runs"
        mock_request.headers = {}

        with pytest.raises(HTTPException) as exc_info:
            require_service_identity(mock_request)
        assert exc_info.value.status_code == 401
        assert "MISSING_SERVICE_IDENTITY" in str(exc_info.value.detail)

    def test_protected_path_accepts_valid_token(self):
        mock_request = MagicMock()
        mock_request.url.path = "/internal/ai/v1/runs"
        mock_request.headers = {SERVICE_IDENTITY_HEADER: self._create_token("/internal/ai/v1/runs")}

        claims = require_service_identity(mock_request)
        assert claims.path == "/internal/ai/v1/runs"

    def test_protected_path_rejects_wrong_path_in_token(self):
        from fastapi import HTTPException

        mock_request = MagicMock()
        mock_request.url.path = "/internal/ai/v1/runs"
        mock_request.headers = {SERVICE_IDENTITY_HEADER: self._create_token("/different/path")}

        with pytest.raises(HTTPException) as exc_info:
            require_service_identity(mock_request)
        assert exc_info.value.status_code == 403
        assert "PATH_MISMATCH" in str(exc_info.value.detail)


class TestContractWithFastAPI:
    """Contract tests using FastAPI TestClient for the sidecar endpoints."""

    def setup_method(self):
        self.secret = "test-secret-for-unit-tests"
        self.now = int(time.time())

    def _create_token(self, path: str, expired: bool = False):
        payload = {
            "iss": SERVICE_ISSUER,
            "sub": SERVICE_SUBJECT,
            "aud": SERVICE_AUDIENCE,
            "iat": self.now,
            "exp": self.now - 100 if expired else self.now + SERVICE_IDENTITY_TTL_SECONDS,
            "path": path,
        }
        return jwt.encode(payload, self.secret, algorithm="HS256")

    def test_health_no_token_returns_200(self):
        client = TestClient(app)
        response = client.get("/internal/ai/v1/health")
        assert response.status_code == 200
        assert response.json()["status"] == "healthy"

    def test_ontology_schema_no_token_returns_401(self):
        client = TestClient(app)
        response = client.get("/internal/ai/v1/ontology/schema")
        assert response.status_code == 401

    def test_ontology_schema_valid_token_returns_200(self):
        from src.infrastructure.ai.ontology.schema_mirror import schema_mirror

        # Pre-populate cache to avoid external HTTP call
        schema_mirror._schema_cache = {"version": "1.0.0", "objects": {}}

        client = TestClient(app)
        token = self._create_token("/internal/ai/v1/ontology/schema")
        response = client.get("/internal/ai/v1/ontology/schema", headers={SERVICE_IDENTITY_HEADER: token})
        assert response.status_code == 200
        assert response.json()["version"] == "1.0.0"

    def test_runs_no_token_returns_401(self):
        client = TestClient(app)
        response = client.post("/internal/ai/v1/runs", json={})
        assert response.status_code == 401

    def test_runs_path_mismatch_returns_403(self):
        client = TestClient(app)
        token = self._create_token("/different/path")
        response = client.post("/internal/ai/v1/runs", json={}, headers={SERVICE_IDENTITY_HEADER: token})
        assert response.status_code == 403

    def test_runs_path_mismatch_response_does_not_echo_paths(self):
        client = TestClient(app)
        token = self._create_token("/different/path")
        response = client.post(
            "/internal/ai/v1/runs",
            json={},
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert response.status_code == 403
        response_text = response.text
        assert "/different/path" not in response_text
        assert "/internal/ai/v1/runs" not in response_text

    def test_runs_valid_token_passes(self):
        client = TestClient(app)
        token = self._create_token("/internal/ai/v1/runs")
        response = client.post(
            "/internal/ai/v1/runs",
            json={
                "contract_version": "ai-runtime.v1",
                "job_id": "job-test-123",
                "run_id": "run-test-456",
                "requester": {"user_id": "test-user", "roles": ["admin"]},
                "task": {"task_type": "nl_query", "user_message": "test"},
                "ontology": {"allowed_actions": [], "risk_ceiling": "low"},
                "context": {"objects": []},
            },
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert "answer" in data

    def test_public_api_v2_is_not_exposed_by_sidecar(self):
        client = TestClient(app)
        response = client.get("/api/v2/ai/nl-query")
        assert response.status_code == 404


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
