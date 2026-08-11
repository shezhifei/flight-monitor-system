"""Tests for AIEntityManager status/config sanitization."""

from __future__ import annotations

import pytest

from src.infrastructure.ai.ai_entity import AIEntityConfig
from src.infrastructure.ai.services.ai_entity_manager import AIEntityManager


@pytest.fixture
def manager() -> AIEntityManager:
    return AIEntityManager()


@pytest.fixture
def config_with_secret() -> AIEntityConfig:
    config = AIEntityConfig(
        api_key="sk-secret-123",
        base_url="https://api.example.com/v1",
        default_model="gpt-4",
    )
    # Simulate extra sensitive fields that may be attached to a config instance.
    config.authorization = "Bearer token"
    config.secret = "top-secret"
    config.password = "hunter2"
    return config


class TestSanitizeConfigForStatus:
    def test_removes_sensitive_fields(self, manager: AIEntityManager, config_with_secret: AIEntityConfig):
        sanitized = manager._sanitize_config_for_status(config_with_secret)

        assert "api_key" not in sanitized
        assert "authorization" not in sanitized
        assert "secret" not in sanitized
        assert "password" not in sanitized

    def test_preserves_non_sensitive_fields(self, manager: AIEntityManager, config_with_secret: AIEntityConfig):
        sanitized = manager._sanitize_config_for_status(config_with_secret)

        assert sanitized["base_url"] == "https://api.example.com/v1"
        assert sanitized["default_model"] == "gpt-4"

    def test_has_api_key_is_true_when_key_present(self, manager: AIEntityManager, config_with_secret: AIEntityConfig):
        sanitized = manager._sanitize_config_for_status(config_with_secret)
        assert sanitized["has_api_key"] is True

    def test_has_api_key_is_false_when_key_missing(self, manager: AIEntityManager):
        config = AIEntityConfig(api_key=None, default_model="gpt-3.5-turbo")
        sanitized = manager._sanitize_config_for_status(config)
        assert sanitized["has_api_key"] is False

    def test_has_api_key_is_false_for_empty_key(self, manager: AIEntityManager):
        config = AIEntityConfig(api_key="", default_model="gpt-3.5-turbo")
        sanitized = manager._sanitize_config_for_status(config)
        assert sanitized["has_api_key"] is False
