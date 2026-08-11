"""Prompt Cache utilities for OpenAI-compatible prompt caching.

Generates stable cache keys, parses cached token usage, and builds
cache parameters for ``chat_completion`` / ``responses_create`` calls.
"""

from __future__ import annotations

import hashlib
import json
from collections.abc import Mapping, Sequence
from typing import Any

from src.infrastructure.ai.feature_flags import is_ai_feature_enabled
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

try:
    import xxhash
except ModuleNotFoundError:  # pragma: no cover
    xxhash = None  # optional dependency

# ---------------------------------------------------------------------------
# Key generation
# ---------------------------------------------------------------------------


def _fingerprint(data: Any) -> str:
    """Return a short hex digest of *data*."""
    raw = json.dumps(data, sort_keys=True, default=str, ensure_ascii=False)
    encoded = raw.encode()
    if xxhash is not None:
        return xxhash.xxh3_64_hexdigest(encoded)[:16]
    return hashlib.sha256(encoded).hexdigest()[:16]


def generate_prompt_cache_key(
    *,
    namespace: str = "flight_monitor",
    surface: str = "default",
    entity_id: str = "default",
    api_format: str = "chat_completions",
    model: str = "",
    system_prompt: str | None = None,
    tools_schemas: Sequence[dict[str, Any]] | None = None,
    output_mode: str | None = None,
) -> str:
    """Build a stable routing-stickiness key.

    The key deliberately excludes dynamic fields (conversation_id,
    request_id, timestamps, user query text) so that requests sharing
    the same static prefix hit the same inference engine.
    """
    system_prompt_hash = _fingerprint(system_prompt or "")
    tools_hash = _fingerprint(tools_schemas or [])
    parts = [
        namespace,
        surface,
        entity_id,
        api_format,
        model,
        system_prompt_hash,
        tools_hash,
        output_mode or "",
    ]
    combined = ":".join(parts)
    encoded = combined.encode()
    if xxhash is not None:
        return xxhash.xxh3_128_hexdigest(encoded)
    return hashlib.sha256(encoded).hexdigest()


# ---------------------------------------------------------------------------
# Usage parsing
# ---------------------------------------------------------------------------


def parse_cached_tokens(usage: dict[str, Any] | None) -> int:
    """Extract the number of cached prompt tokens from an API response usage dict.

    Supports:
    * OpenAI: ``usage.prompt_tokens_details.cached_tokens``
    * Anthropic-style: ``usage.cache_read_input_tokens``
    """
    if not usage or not isinstance(usage, dict):
        return 0

    # OpenAI format
    details = usage.get("prompt_tokens_details")
    if isinstance(details, dict):
        cached = details.get("cached_tokens")
        if cached is not None:
            return int(cached)

    # Anthropic-style fallback
    cached_alt = usage.get("cache_read_input_tokens")
    if cached_alt is not None:
        return int(cached_alt)

    return 0


# ---------------------------------------------------------------------------
# Config-level helpers
# ---------------------------------------------------------------------------


def should_enable_prompt_cache(
    entity_config: Any,
    *,
    feature_overrides: Mapping[str, Any] | None = None,
    config_manager: Any | None = None,
) -> bool:
    """Determine whether prompt caching should be active.

    Both the feature flag ``AI_PROMPT_CACHE_V1`` **and** the entity-level
    ``enable_prompt_cache`` must be true.
    """
    flag_on = is_ai_feature_enabled(
        "AI_PROMPT_CACHE_V1",
        default=False,
        config_manager=config_manager,
        overrides=feature_overrides,
    )
    if not flag_on:
        return False

    entity_on = getattr(entity_config, "enable_prompt_cache", False)
    return bool(entity_on)


def build_prompt_cache_params(
    key: str | None,
    retention: str | None = None,
    *,
    enabled: bool = True,
) -> dict[str, Any]:
    """Return kwargs to inject into an AI gateway call.

    Returns an empty dict when disabled or when *key* is falsy.
    """
    if not enabled or not key:
        return {}

    params: dict[str, Any] = {"prompt_cache_key": key}
    if retention:
        params["prompt_cache_retention"] = retention
    return params
