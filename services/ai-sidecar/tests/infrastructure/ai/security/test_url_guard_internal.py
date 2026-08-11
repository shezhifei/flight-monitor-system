import socket

import pytest
from src.infrastructure.ai.security.url_guard import (
    UnsafeUrlError,
    validate_internal_service_url,
)


class TestValidateInternalServiceUrl:
    def test_allows_loopback_when_enabled(self):
        result = validate_internal_service_url(
            "http://localhost:8080/api",
            purpose="test service",
            allow_loopback=True,
            require_tls=False,
        )
        assert result == "http://localhost:8080/api"

    def test_rejects_loopback_when_disabled(self):
        with pytest.raises(UnsafeUrlError, match="private or local"):
            validate_internal_service_url(
                "http://localhost:8080/api",
                purpose="test service",
                allow_loopback=False,
                require_tls=False,
            )

    def test_allows_https_by_default(self, monkeypatch):
        def fake_getaddrinfo(host, port, type=0):
            return [(socket.AF_INET, socket.SOCK_STREAM, socket.IPPROTO_TCP, "", ("93.184.216.34", port))]

        monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)

        result = validate_internal_service_url(
            "https://api.internal.example.com/v1",
            purpose="test service",
            allow_loopback=False,
        )
        assert result == "https://api.internal.example.com/v1"

    def test_rejects_http_when_tls_required(self):
        with pytest.raises(UnsafeUrlError, match="plain HTTP"):
            validate_internal_service_url(
                "http://api.internal.example.com/v1",
                purpose="test service",
                allow_loopback=False,
                require_tls=True,
            )

    def test_rejects_credentials(self):
        with pytest.raises(UnsafeUrlError, match="credentials"):
            validate_internal_service_url(
                "https://user:pass@localhost:8080/api",
                purpose="test service",
                allow_loopback=True,
            )

    def test_rejects_query_and_fragment(self):
        with pytest.raises(UnsafeUrlError, match="query strings"):
            validate_internal_service_url(
                "http://localhost:8080/api?q=1",
                purpose="test service",
                allow_loopback=True,
                require_tls=False,
            )

    def test_allows_specific_hosts(self):
        result = validate_internal_service_url(
            "http://trusted.internal:9000/",
            purpose="test service",
            allowed_hosts={"trusted.internal"},
            require_tls=False,
        )
        assert result == "http://trusted.internal:9000"
