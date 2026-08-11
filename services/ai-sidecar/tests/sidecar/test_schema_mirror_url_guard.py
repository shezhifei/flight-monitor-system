"""Regression tests for SchemaMirror outbound URL validation."""

from __future__ import annotations

import socket

import pytest

from src.infrastructure.ai.ontology.schema_mirror import SchemaMirror


def test_schema_mirror_allows_default_loopback_rust_api_url():
    mirror = SchemaMirror()

    assert mirror.rust_api_url == "http://localhost:8080"


def test_schema_mirror_rejects_metadata_service_url_by_default():
    with pytest.raises(ValueError, match="Unsafe Rust API base_url"):
        SchemaMirror("http://169.254.169.254/latest/meta-data")


def test_schema_mirror_validates_public_https_url(monkeypatch):
    def fake_getaddrinfo(host, port, type=0):
        assert host == "api.example.com"
        return [(socket.AF_INET, socket.SOCK_STREAM, socket.IPPROTO_TCP, "", ("93.184.216.34", port))]

    monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)

    mirror = SchemaMirror("https://api.example.com/v1/")

    assert mirror.rust_api_url == "https://api.example.com/v1"


def test_schema_mirror_builds_schema_snapshot_request(monkeypatch):
    captured = {}

    class FakeResponse:
        def raise_for_status(self):
            return None

        def json(self):
            return {"ontology_version": "flight-ops.v1", "objects": {}}

    def fake_get(url):
        captured["url"] = url
        return FakeResponse()

    monkeypatch.setattr("src.infrastructure.ai.ontology.schema_mirror.requests.get", fake_get)
    mirror = SchemaMirror()

    assert mirror.load_schema_snapshot() == {"ontology_version": "flight-ops.v1", "objects": {}}
    assert captured["url"] == "http://localhost:8080/api/v2/ai/ontology/schema"
