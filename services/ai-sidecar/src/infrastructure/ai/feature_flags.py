"""Infrastructure-safe AI feature flag helpers."""

from __future__ import annotations

import os
from collections.abc import Mapping
from typing import Any

from src.infrastructure.logging.core import get_logger
from src.infrastructure.runtime.providers import get_runtime_config_manager

logger = get_logger(__name__)

AI_FEATURE_FLAG_DEFAULTS: dict[str, bool] = {
    "AI_APPROVAL_DIFF_V1": True,
    "AI_TODO_AGENT_GRAPH_V1": False,
    "AI_KB_FTS_V1": False,
    "AI_QUERY_UNIFIED_V1": True,
    "AI_MEMORY_PROFILE_V1": True,
    "AI_REPORT_SCHEMA_V1": True,
    "AI_CONTEXT_BUDGET_V1": True,
    "AI_GLOBAL_PUSH_V1": True,
    "AI_PROMPT_CACHE_V1": True,
    "AI_RESPONSES_SESSION_CHAIN_V1": False,
}


def _to_bool(value: Any, default: bool) -> bool:
    if isinstance(value, bool):
        return value
    if value is None:
        return bool(default)
    if isinstance(value, (int, float)):
        return bool(value)
    normalized = str(value).strip().lower()
    if normalized in {"1", "true", "yes", "y", "on", "enabled"}:
        return True
    if normalized in {"0", "false", "no", "n", "off", "disabled"}:
        return False
    return bool(default)


def _normalize_flag_name(flag_name: str) -> str:
    return str(flag_name or "").strip().upper()


def _candidate_paths(flag_name: str) -> list[str]:
    raw = str(flag_name or "").strip()
    if not raw:
        return []
    lowered = raw.lower()
    return [
        f"feature_flags.{raw}",
        f"feature_flags.{lowered}",
        f"feature_flags.{raw}.enabled",
        f"feature_flags.{lowered}.enabled",
        f"feature_flags.ai.{raw}",
        f"feature_flags.ai.{lowered}",
        f"feature_flags.ai.{raw}.enabled",
        f"feature_flags.ai.{lowered}.enabled",
    ]


def _resolve_config_manager(config_manager: Any | None = None) -> Any | None:
    if config_manager is not None:
        return config_manager
    return get_runtime_config_manager()


def _read_from_config(flag_name: str, *, default: bool, config_manager: Any | None) -> bool | None:
    manager = _resolve_config_manager(config_manager)
    if manager is None:
        return None
    has_method = getattr(manager, "has", None)
    get_bool_method = getattr(manager, "get_bool", None)
    if get_bool_method is None:
        return None
    for path in _candidate_paths(flag_name):
        try:
            if callable(has_method) and not has_method(path):
                continue
            return bool(get_bool_method(path, default))
        except Exception as error:  # noqa: BLE001 - feature flag reads must never raise
            logger.debug("feature_flag_check_error path=%s", path, exc_info=error)
            continue
    return None


def _read_from_env(flag_name: str) -> bool | None:
    raw = str(flag_name or "").strip()
    if not raw:
        return None
    env_keys = [raw, raw.upper(), f"FEATURE_{raw.upper()}", f"FF_{raw.upper()}"]
    for key in env_keys:
        value = os.getenv(key)
        if value is not None:
            return _to_bool(value, default=False)
    return None


def is_ai_feature_enabled(
    flag_name: str,
    *,
    default: bool | None = None,
    config_manager: Any | None = None,
    overrides: Mapping[str, Any] | None = None,
) -> bool:
    normalized_name = _normalize_flag_name(flag_name)
    fallback = AI_FEATURE_FLAG_DEFAULTS.get(normalized_name, True if default is None else bool(default))

    if overrides:
        for key in (normalized_name, normalized_name.lower()):
            if key in overrides:
                return _to_bool(overrides[key], fallback)

    config_value = _read_from_config(normalized_name, default=fallback, config_manager=config_manager)
    if config_value is not None:
        return config_value

    env_value = _read_from_env(normalized_name)
    if env_value is not None:
        return env_value

    return fallback


def resolve_ai_feature_flags(
    *,
    config_manager: Any | None = None,
    overrides: Mapping[str, Any] | None = None,
) -> dict[str, bool]:
    return {
        name: is_ai_feature_enabled(name, config_manager=config_manager, overrides=overrides)
        for name in AI_FEATURE_FLAG_DEFAULTS
    }
