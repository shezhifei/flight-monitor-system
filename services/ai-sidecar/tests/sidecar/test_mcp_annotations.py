"""Tests for normalize_mcp_tool_annotations — single canonical annotation normalizer.

Task 13: MCP annotation interpretation must converge to a single helper.
- Fail-closed defaults: missing/ambiguous annotations → destructive=True, side_effect=True
- cacheable defaults to False
- Replaces divergent inline logic in tool_executor.py and capability_resolver.py
"""

from __future__ import annotations

from src.infrastructure.ai.mcp.annotations import (
    NormalizedMcpAnnotations,
    normalize_mcp_tool_annotations,
)


class TestNormalizeMcpToolAnnotations:
    """Fail-closed annotation normalization (Task 13)."""

    def test_returns_fail_closed_defaults_when_annotations_is_none(self):
        result = normalize_mcp_tool_annotations(None)
        assert result.destructive is True
        assert result.side_effect is True
        assert result.cacheable is False

    def test_returns_fail_closed_defaults_when_annotations_is_empty_dict(self):
        result = normalize_mcp_tool_annotations({})
        assert result.destructive is True
        assert result.side_effect is True
        assert result.cacheable is False

    def test_returns_fail_closed_defaults_when_annotations_is_invalid_json_string(self):
        result = normalize_mcp_tool_annotations("not valid json")
        assert result.destructive is True
        assert result.side_effect is True
        assert result.cacheable is False

    def test_parses_json_string_annotations(self):
        result = normalize_mcp_tool_annotations('{"destructive": false, "cacheable": true}')
        assert result.destructive is False
        assert result.side_effect is False
        assert result.cacheable is True

    def test_destructive_true_implies_side_effect_true(self):
        result = normalize_mcp_tool_annotations({"destructive": True})
        assert result.destructive is True
        assert result.side_effect is True

    def test_side_effect_true_implies_side_effect_true(self):
        result = normalize_mcp_tool_annotations({"side_effect": True})
        assert result.side_effect is True

    def test_destructive_false_and_side_effect_absent_implies_side_effect_false(self):
        result = normalize_mcp_tool_annotations({"destructive": False})
        assert result.destructive is False
        assert result.side_effect is False

    def test_side_effect_false_and_destructive_absent_implies_side_effect_false(self):
        result = normalize_mcp_tool_annotations({"side_effect": False})
        assert result.side_effect is False

    def test_destructive_true_takes_precedence_over_side_effect_false(self):
        result = normalize_mcp_tool_annotations({"destructive": True, "side_effect": False})
        assert result.side_effect is True

    def test_side_effect_true_takes_precedence_over_destructive_false(self):
        result = normalize_mcp_tool_annotations({"destructive": False, "side_effect": True})
        assert result.side_effect is True

    def test_both_false_implies_side_effect_false(self):
        result = normalize_mcp_tool_annotations({"destructive": False, "side_effect": False})
        assert result.side_effect is False

    def test_cacheable_defaults_to_false(self):
        result = normalize_mcp_tool_annotations({"cacheable": None})
        assert result.cacheable is False

    def test_cacheable_explicitly_true(self):
        result = normalize_mcp_tool_annotations({"cacheable": True})
        assert result.cacheable is True

    def test_returns_normalized_annotations_dataclass(self):
        result = normalize_mcp_tool_annotations({"destructive": False, "cacheable": True})
        assert isinstance(result, NormalizedMcpAnnotations)

    def test_non_dict_non_string_value_returns_fail_closed_defaults(self):
        result = normalize_mcp_tool_annotations(42)
        assert result.destructive is True
        assert result.side_effect is True
        assert result.cacheable is False

    def test_list_value_returns_fail_closed_defaults(self):
        result = normalize_mcp_tool_annotations([1, 2, 3])
        assert result.destructive is True
        assert result.side_effect is True
        assert result.cacheable is False
