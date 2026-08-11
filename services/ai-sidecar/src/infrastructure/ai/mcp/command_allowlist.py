"""Centralized MCP command_ref allowlist resolver.

Security source: AI_MCP_COMMAND_ALLOWLIST_JSON environment variable only.
No dynamic bypass allowed — command_ref must be present in the allowlist
before any subprocess is spawned.

Format (env JSON):
{
  "npx": {
    "executable": "npx",
    "args_prefix": ["-y", "@scope/server"],
    "working_dir": null,
    "allowed_args": ["--verbose", "--timeout=30"]
  }
}

``allowed_args`` (optional list of strings):
  If present, caller-supplied ``args`` must be a subset of this set.
  If absent, no caller args are accepted (safe default — only the
  admin-controlled ``args_prefix`` is forwarded to the subprocess).
"""

from __future__ import annotations

import json
import os
from typing import Any

_ENV_VAR = "AI_MCP_COMMAND_ALLOWLIST_JSON"

_allowlist_cache: dict[str, Any] | None = None


def _load_allowlist() -> dict[str, Any]:
    raw = os.environ.get(_ENV_VAR, "").strip()
    if not raw:
        return {}
    try:
        parsed = json.loads(raw)
        if not isinstance(parsed, dict):
            return {}
        return parsed
    except (json.JSONDecodeError, TypeError):
        return {}


def get_command_allowlist() -> dict[str, Any]:
    """Return the full allowlist dict. Cached after first load."""
    global _allowlist_cache
    if _allowlist_cache is None:
        _allowlist_cache = _load_allowlist()
    return _allowlist_cache


def is_command_allowed(command_ref: str) -> bool:
    """Check if command_ref is in the allowlist."""
    allowlist = get_command_allowlist()
    return command_ref in allowlist


def get_allowlist_entry(command_ref: str) -> dict[str, Any] | None:
    """Return allowlist entry for command_ref, or None if not allowed."""
    allowlist = get_command_allowlist()
    return allowlist.get(command_ref)


def build_client_allowlist(command_ref: str) -> dict[str, Any]:
    """Build a single-entry allowlist dict for McpClientManager.

    Returns empty dict if command_ref is not allowed — caller must check
    is_command_allowed() first.
    """
    entry = get_allowlist_entry(command_ref)
    if entry is None:
        return {}
    return {command_ref: entry}


def reset_cache() -> None:
    """Reset cached allowlist (for testing only)."""
    global _allowlist_cache
    _allowlist_cache = None
