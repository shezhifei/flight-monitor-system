"""运行时/配置通用工具函数。"""

from __future__ import annotations

import json
import os
from typing import Any

_PRODUCTION_ENV_VALUES = {"production", "prod", "staging", "stage"}


def is_production_environment() -> bool:
    """Return True when running in a production-like environment.

    Checks APP_ENV, APP_ENVIRONMENT, ENVIRONMENT, FLIGHT_ENV.
    Values that trigger production mode: production, prod, staging, stage.
    """
    for key in ("APP_ENV", "APP_ENVIRONMENT", "ENVIRONMENT", "FLIGHT_ENV"):
        value = str(os.environ.get(key, "")).strip().lower()
        if value in _PRODUCTION_ENV_VALUES:
            return True
    return False


def get_runtime_holder(module_name: str = "__main__") -> Any:
    """获取运行时容器 (惰性加载, 避免循环导入)。"""
    try:
        from src.infrastructure.runtime.providers import get_runtime_container

        return get_runtime_container()
    except (ImportError, AttributeError):
        return None


def parse_json_field(value: Any, default: Any = None) -> Any:
    """将字符串或字节解析为 JSON; 失败或类型不符时返回 default。"""
    if value is None:
        return default
    if isinstance(value, (dict, list)):
        return value
    if isinstance(value, (str, bytes, bytearray)):
        try:
            return json.loads(value)
        except (json.JSONDecodeError, TypeError, ValueError):
            return default
    return default


def decode_jsonb_or_raise(value: Any, field_name: str) -> Any:
    """Decode a JSONB field, raising on parse failure.

    Use for security-sensitive fields (allowed_tools, denied_tools,
    allowed_resources) where silently defaulting to empty would weaken
    ACL enforcement.  Non-security fields should use ``parse_json_field``
    with a default instead.
    """
    if value is None:
        return None
    if isinstance(value, (dict, list)):
        return value
    if isinstance(value, (str, bytes, bytearray)):
        try:
            return json.loads(value)
        except (json.JSONDecodeError, TypeError, ValueError) as exc:
            raise ValueError(f"JSONB decode failed for security field '{field_name}': {exc}") from exc
    raise ValueError(f"JSONB decode failed for security field '{field_name}': unexpected type {type(value).__name__}")
