"""Security regression tests for outbound URL validation."""

from __future__ import annotations

import socket

import pytest

from src.infrastructure.ai.security.url_guard import UnsafeUrlError, validate_external_http_url


def test_validate_external_http_url_allows_public_https(monkeypatch):
    def fake_getaddrinfo(host, port, type=0):
        assert host == "api.openai.com"
        return [(socket.AF_INET, socket.SOCK_STREAM, socket.IPPROTO_TCP, "", ("93.184.216.34", port))]

    monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)

    assert validate_external_http_url("https://api.openai.com/v1", purpose="test") == "https://api.openai.com/v1"


@pytest.mark.parametrize(
    ("family", "address"),
    [
        (socket.AF_INET, "10.0.0.5"),
        (socket.AF_INET, "169.254.169.254"),
        (socket.AF_INET6, "::1"),
    ],
)
def test_validate_external_http_url_blocks_hostnames_that_resolve_private(monkeypatch, family, address):
    def fake_getaddrinfo(host, port, type=0):
        assert host == "api.rebinding.test"
        sockaddr = (address, port, 0, 0) if family == socket.AF_INET6 else (address, port)
        return [(family, socket.SOCK_STREAM, socket.IPPROTO_TCP, "", sockaddr)]

    monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)

    with pytest.raises(UnsafeUrlError):
        validate_external_http_url("https://api.rebinding.test/v1", purpose="test")


def test_validate_external_http_url_fails_closed_when_hostname_resolution_fails(monkeypatch):
    def fake_getaddrinfo(host, port, type=0):
        raise socket.gaierror("resolution failed")

    monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)

    with pytest.raises(UnsafeUrlError):
        validate_external_http_url("https://api.example.test/v1", purpose="test")


@pytest.mark.parametrize(
    "url",
    [
        "http://api.example.com/v1",
        "https://api.example.com:99999/v1",
        "https://127.0.0.1/v1",
        "https://localhost/v1",
        "https://169.254.169.254/latest/meta-data",
        "file:///etc/passwd",
    ],
)
def test_validate_external_http_url_blocks_ssrf_targets_by_default(url):
    with pytest.raises(UnsafeUrlError):
        validate_external_http_url(url, purpose="test")


def test_validate_external_http_url_requires_private_override_for_private_https(monkeypatch):
    monkeypatch.setenv("AI_SIDECAR_ALLOW_PRIVATE_HTTP_TARGETS", "1")
    assert validate_external_http_url("https://127.0.0.1/v1", purpose="test") == "https://127.0.0.1/v1"


def test_validate_external_http_url_private_override_skips_hostname_resolution(monkeypatch):
    def fail_getaddrinfo(host, port, type=0):
        raise AssertionError("private target override should skip DNS resolution")

    monkeypatch.setenv("AI_SIDECAR_ALLOW_PRIVATE_HTTP_TARGETS", "1")
    monkeypatch.setattr(socket, "getaddrinfo", fail_getaddrinfo)

    assert (
        validate_external_http_url("https://private.example.test/v1", purpose="test")
        == "https://private.example.test/v1"
    )
