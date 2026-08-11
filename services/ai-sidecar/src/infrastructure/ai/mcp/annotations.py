"""Single canonical MCP tool annotation normalizer.

Task 13: Replaces divergent inline annotation logic in tool_executor.py
and capability_resolver.py with one fail-closed helper.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from src.infrastructure.common.runtime_utils import parse_json_field


@dataclass(frozen=True)
class NormalizedMcpAnnotations:
    """Normalized MCP tool annotations with fail-closed defaults.

    - ``destructive``: True when explicitly set or missing (fail-closed).
    - ``side_effect``: True if either ``destructive`` or ``side_effect`` is True;
      False only when both are explicitly False; True when missing (fail-closed).
    - ``cacheable``: False unless explicitly True.
    """

    destructive: bool
    side_effect: bool
    cacheable: bool


def normalize_mcp_tool_annotations(annotations: Any) -> NormalizedMcpAnnotations:
    """Normalize raw MCP tool annotations into a typed, fail-closed result.

    Accepts a dict, a JSON string, or None.  When annotations are missing,
    unparseable, or ambiguous, the result defaults to the safest assumption:
    destructive=True, side_effect=True, cacheable=False.
    """
    parsed = parse_json_field(annotations, default={})
    if not isinstance(parsed, dict):
        return NormalizedMcpAnnotations(destructive=True, side_effect=True, cacheable=False)

    destructive_raw = parsed.get("destructive")
    side_effect_raw = parsed.get("side_effect")

    # Fail-closed: missing/ambiguous → True
    destructive = destructive_raw if isinstance(destructive_raw, bool) else True

    if destructive_raw is True or side_effect_raw is True:
        side_effect = True
    elif destructive_raw is False or side_effect_raw is False:
        side_effect = False
    else:
        side_effect = True  # fail-closed

    cacheable_raw = parsed.get("cacheable")
    cacheable = cacheable_raw is True

    return NormalizedMcpAnnotations(
        destructive=destructive,
        side_effect=side_effect,
        cacheable=cacheable,
    )
