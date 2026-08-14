"""AI 配置 API Key 加解密器。

将原 ``PostgresAIConfigStore`` 内联的 Fernet / base64 加解密逻辑抽取为独立、
可单测、可复用的组件，供 asyncpg 原生配置存储与未来其它存储后端共用。

安全语义：

* 加密密钥来自环境变量 ``AI_CONFIG_ENCRYPTION_KEY``。
* 若密钥为合法 Fernet key 则直接使用；否则对其做 ``sha256`` 派生后再
  ``urlsafe_b64encode`` 作为 Fernet key（兼容历史行为）。
* 默认 fail-closed：除非显式设置 ``AI_CONFIG_ALLOW_INSECURE_DEV_BASE64=true``，
  否则写入含 ``api_key`` 的配置时必须启用 Fernet 加密；缺失加密能力时直接抛错，
  避免明文 / base64 落库。``AI_CONFIG_REQUIRE_ENCRYPTION`` 仍可显式覆盖该行为。
* 解密时移除 ``_key_encrypted`` / ``_key_encoded`` / ``_key_encryption`` 元字段，
  绝不向上层返回内部存储标记。
"""

from __future__ import annotations

import base64
import hashlib
import os
from copy import deepcopy
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class ConfigEncryptor:
    """AI 实体配置中敏感字段（当前为 ``api_key``）的加解密器。

    无状态地持有一个 Fernet 实例（或在未配置密钥时为 ``None``），其行为完全由
    构造时读取的环境变量决定。可在进程内安全复用。
    """

    def __init__(self) -> None:
        self._require_encryption = self._resolve_require_encryption()
        self._fernet = self._init_fernet()
        self._encryption_enabled = self._fernet is not None

        if self._encryption_enabled:
            logger.info("ConfigEncryptor: API key encryption enabled (fernet)")
        elif self._require_encryption:
            logger.error(
                "ConfigEncryptor requires encrypted API key storage in the current runtime "
                "environment. Set AI_CONFIG_ENCRYPTION_KEY and install cryptography."
            )
        else:
            logger.warning(
                "ConfigEncryptor: API key encryption is not enabled. Insecure base64 fallback "
                "is active because AI_CONFIG_ALLOW_INSECURE_DEV_BASE64=true. This must never "
                "be used in production or for real credentials."
            )

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------
    @property
    def encryption_enabled(self) -> bool:
        """Whether fernet-based encryption is active."""
        return self._encryption_enabled

    @property
    def require_encryption(self) -> bool:
        """Whether the current runtime environment mandates encryption."""
        return self._require_encryption

    # ------------------------------------------------------------------
    # Initialization helpers
    # ------------------------------------------------------------------
    def _init_fernet(self):
        """Initialize fernet encryptor when an encryption key is configured."""
        raw_key = os.environ.get("AI_CONFIG_ENCRYPTION_KEY", "").strip()
        if not raw_key:
            return None

        try:
            from cryptography.fernet import Fernet
        except (
            Exception  # noqa: BLE001
        ) as exc:  # pragma: no cover - depends on optional dep
            logger.warning("AI_CONFIG_ENCRYPTION_KEY configured but cryptography is unavailable: %s", exc)
            return None

        key_bytes = raw_key.encode("utf-8")
        try:
            return Fernet(key_bytes)
        except Exception as exc:  # noqa: BLE001 - fernet key validation may fail in various ways
            logger.warning("fernet_key_validation_failed_fallback_to_sha256", exc_info=exc)
            derived_key = base64.urlsafe_b64encode(hashlib.sha256(key_bytes).digest())
            return Fernet(derived_key)

    @staticmethod
    def _to_bool(value: Any, default: bool = False) -> bool:
        if value is None:
            return bool(default)
        if isinstance(value, bool):
            return value
        if isinstance(value, (int, float)):
            return bool(value)
        text = str(value).strip().lower()
        if not text:
            return bool(default)
        if text in {"1", "true", "yes", "y", "on", "enabled"}:
            return True
        if text in {"0", "false", "no", "n", "off", "disabled"}:
            return False
        return bool(default)

    @staticmethod
    def _runtime_env() -> str:
        for key in ("APP_ENV", "APP_ENVIRONMENT", "ENVIRONMENT", "FLIGHT_ENV"):
            value = str(os.environ.get(key, "")).strip().lower()
            if value:
                return value
        return ""

    def _resolve_require_encryption(self) -> bool:
        # Explicit requirement flag takes precedence over insecure-dev opt-in.
        explicit_value = os.environ.get("AI_CONFIG_REQUIRE_ENCRYPTION")
        if explicit_value is not None:
            return self._to_bool(explicit_value, default=True)
        # Explicit insecure-dev opt-in disables the requirement (local development only).
        allow_insecure = os.environ.get("AI_CONFIG_ALLOW_INSECURE_DEV_BASE64")
        if allow_insecure is not None:
            return not self._to_bool(allow_insecure, default=False)
        # Default fail-closed: always require encryption unless explicitly opted out.
        return True

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------
    def decrypt_config(self, config: dict[str, Any]) -> dict[str, Any]:
        """Decrypt every api_key in the document and drop storage markers."""
        config_copy = deepcopy(config)
        encrypted = bool(config_copy.get("_key_encrypted"))
        encoded = bool(config_copy.get("_key_encoded"))
        self._transform_api_keys(config_copy, lambda value: self._decrypt_value(value, encrypted, encoded))
        config_copy.pop("_key_encoded", None)
        config_copy.pop("_key_encrypted", None)
        config_copy.pop("_key_encryption", None)
        return config_copy

    def encrypt_config(self, config: dict[str, Any]) -> dict[str, Any]:
        """Encrypt every api_key in the document and write storage markers."""
        config_copy = deepcopy(config)
        if not self._has_api_key(config_copy):
            return config_copy
        if not self._fernet and self._require_encryption:
            raise RuntimeError(
                "Encrypted AI config is required in this environment but fernet is "
                "unavailable. Set AI_CONFIG_ENCRYPTION_KEY and install cryptography."
            )
        self._transform_api_keys(config_copy, self._encrypt_value)
        if self._fernet:
            config_copy["_key_encrypted"] = True
            config_copy["_key_encryption"] = "fernet_v1"
            config_copy.pop("_key_encoded", None)
        else:
            config_copy["_key_encoded"] = True
            config_copy.pop("_key_encrypted", None)
            config_copy.pop("_key_encryption", None)
        return config_copy

    def _has_api_key(self, value: Any) -> bool:
        if isinstance(value, dict):
            api_key = value.get("api_key")
            if isinstance(api_key, str) and api_key:
                return True
            return any(self._has_api_key(child) for child in value.values())
        if isinstance(value, list):
            return any(self._has_api_key(item) for item in value)
        return False

    def _transform_api_keys(self, value: Any, transform) -> bool:
        found = False
        if isinstance(value, dict):
            api_key = value.get("api_key")
            if isinstance(api_key, str) and api_key:
                value["api_key"] = transform(api_key)
                found = True
            for key, child in value.items():
                if key == "api_key":
                    continue
                found = self._transform_api_keys(child, transform) or found
        elif isinstance(value, list):
            for item in value:
                found = self._transform_api_keys(item, transform) or found
        return found

    def _encrypt_value(self, key: str) -> str:
        if self._fernet:
            return self._fernet.encrypt(key.encode("utf-8")).decode("utf-8")
        return base64.b64encode(key.encode("utf-8")).decode("utf-8")

    def _decrypt_value(self, api_key: str, encrypted: bool, encoded: bool) -> str:
        if encrypted:
            if not self._fernet:
                logger.error("Cannot decrypt AI API key: encryption is enabled but fernet is unavailable")
                return ""
            try:
                return self._fernet.decrypt(str(api_key).encode("utf-8")).decode("utf-8")
            except Exception as exc:  # noqa: BLE001 - fernet decrypt may fail in various ways
                logger.warning("fernet_api_key_decrypt_failed", exc_info=exc)
                return ""
        if encoded:
            if self._require_encryption:
                logger.warning(
                    "Loaded legacy base64 AI API key while encrypted storage is required. "
                    "Please rotate and re-save AI config with fernet enabled."
                )
            try:
                return base64.b64decode(str(api_key)).decode("utf-8")
            except Exception as exc:  # noqa: BLE001 - base64 decode may fail in various ways
                logger.warning("base64_api_key_decode_failed", exc_info=exc)
                return ""
        return api_key


_default_encryptor: ConfigEncryptor | None = None


def get_config_encryptor() -> ConfigEncryptor:
    """Return a process-wide shared ``ConfigEncryptor`` instance."""
    global _default_encryptor
    if _default_encryptor is None:
        _default_encryptor = ConfigEncryptor()
    return _default_encryptor


def reset_config_encryptor() -> None:
    """Reset the shared encryptor (for testing only)."""
    global _default_encryptor
    _default_encryptor = None


__all__ = [
    "ConfigEncryptor",
    "get_config_encryptor",
    "reset_config_encryptor",
]
