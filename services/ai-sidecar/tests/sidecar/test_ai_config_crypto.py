"""Tests for ConfigEncryptor (extracted AI config API-key crypto)."""

from __future__ import annotations

import base64

import pytest

from src.infrastructure.ai.config.ai_config_crypto import (
    ConfigEncryptor,
    get_config_encryptor,
    reset_config_encryptor,
)

CRYPTO_ENV_VARS = (
    "AI_CONFIG_ENCRYPTION_KEY",
    "AI_CONFIG_REQUIRE_ENCRYPTION",
    "AI_CONFIG_ALLOW_INSECURE_DEV_BASE64",
    "APP_ENV",
    "APP_ENVIRONMENT",
    "ENVIRONMENT",
    "FLIGHT_ENV",
)


@pytest.fixture(autouse=True)
def _clean_crypto_env(monkeypatch):
    """Ensure each test starts from a clean crypto-related environment."""
    for var in CRYPTO_ENV_VARS:
        monkeypatch.delenv(var, raising=False)
    reset_config_encryptor()
    yield
    reset_config_encryptor()


def _make_fernet_key() -> str:
    from cryptography.fernet import Fernet

    return Fernet.generate_key().decode("utf-8")


class TestFernetRoundTrip:
    def test_encrypt_then_decrypt_recovers_plaintext(self, monkeypatch):
        monkeypatch.setenv("AI_CONFIG_ENCRYPTION_KEY", _make_fernet_key())
        enc = ConfigEncryptor()
        assert enc.encryption_enabled is True

        encrypted = enc.encrypt_config({"api_key": "sk-secret-123", "base_url": "x"})
        # Ciphertext must not be the plaintext, and metadata markers are set.
        assert encrypted["api_key"] != "sk-secret-123"
        assert encrypted["_key_encrypted"] is True
        assert encrypted["_key_encryption"] == "fernet_v1"

        decrypted = enc.decrypt_config(encrypted)
        assert decrypted["api_key"] == "sk-secret-123"
        assert decrypted["base_url"] == "x"
        # Internal markers must never leak to callers.
        assert "_key_encrypted" not in decrypted
        assert "_key_encryption" not in decrypted
        assert "_key_encoded" not in decrypted

    def test_derived_key_when_raw_key_not_valid_fernet(self, monkeypatch):
        # A short, non-Fernet key triggers sha256-derived key path.
        monkeypatch.setenv("AI_CONFIG_ENCRYPTION_KEY", "not-a-fernet-key")
        enc = ConfigEncryptor()
        assert enc.encryption_enabled is True
        encrypted = enc.encrypt_config({"api_key": "sk-abc"})
        assert enc.decrypt_config(encrypted)["api_key"] == "sk-abc"

    def test_empty_api_key_is_left_untouched(self, monkeypatch):
        monkeypatch.setenv("AI_CONFIG_ENCRYPTION_KEY", _make_fernet_key())
        enc = ConfigEncryptor()
        out = enc.encrypt_config({"api_key": "", "x": 1})
        assert out["api_key"] == ""
        assert "_key_encrypted" not in out


class TestBase64Fallback:
    def test_base64_fallback_requires_insecure_dev_flag(self, monkeypatch):
        monkeypatch.setenv("AI_CONFIG_ALLOW_INSECURE_DEV_BASE64", "true")
        enc = ConfigEncryptor()
        assert enc.encryption_enabled is False
        assert enc.require_encryption is False

        encrypted = enc.encrypt_config({"api_key": "sk-plain"})
        assert encrypted["_key_encoded"] is True
        # Stored value is base64 of the plaintext.
        assert base64.b64decode(encrypted["api_key"]).decode("utf-8") == "sk-plain"

        decrypted = enc.decrypt_config(encrypted)
        assert decrypted["api_key"] == "sk-plain"
        assert "_key_encoded" not in decrypted

    def test_default_requires_encryption_without_key(self):
        enc = ConfigEncryptor()
        assert enc.encryption_enabled is False
        assert enc.require_encryption is True
        with pytest.raises(RuntimeError):
            enc.encrypt_config({"api_key": "sk-should-not-store-plain"})


class TestRequireEncryption:
    def test_production_env_requires_encryption(self, monkeypatch):
        monkeypatch.setenv("APP_ENV", "production")
        enc = ConfigEncryptor()
        assert enc.require_encryption is True
        assert enc.encryption_enabled is False  # no key provided
        with pytest.raises(RuntimeError):
            enc.encrypt_config({"api_key": "sk-should-not-store-plain"})

    def test_explicit_require_flag_overrides_insecure_opt_in(self, monkeypatch):
        monkeypatch.setenv("AI_CONFIG_ALLOW_INSECURE_DEV_BASE64", "true")
        monkeypatch.setenv("AI_CONFIG_REQUIRE_ENCRYPTION", "true")
        enc = ConfigEncryptor()
        assert enc.require_encryption is True
        with pytest.raises(RuntimeError):
            enc.encrypt_config({"api_key": "sk-x"})

    def test_explicit_false_flag_disables_requirement_in_prod(self, monkeypatch):
        monkeypatch.setenv("APP_ENV", "production")
        monkeypatch.setenv("AI_CONFIG_REQUIRE_ENCRYPTION", "false")
        enc = ConfigEncryptor()
        assert enc.require_encryption is False
        # Falls back to base64 without raising.
        out = enc.encrypt_config({"api_key": "sk-x"})
        assert out["_key_encoded"] is True


class TestSharedInstance:
    def test_get_config_encryptor_is_cached(self):
        a = get_config_encryptor()
        b = get_config_encryptor()
        assert a is b
        reset_config_encryptor()
        c = get_config_encryptor()
        assert c is not a
