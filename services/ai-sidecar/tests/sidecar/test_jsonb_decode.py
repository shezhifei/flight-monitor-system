"""Tests for JSONB decode helpers — fail-closed for security fields.

Task 13: Security fields (allowed_tools, denied_tools, allowed_resources)
must raise on parse failure instead of silently defaulting to empty,
which would weaken ACL enforcement.
"""

from __future__ import annotations

import json

import pytest

from src.infrastructure.ai.mcp_repository import _decode_row_jsonb
from src.infrastructure.common.runtime_utils import (
    decode_jsonb_or_raise,
    parse_json_field,
)


class TestDecodeJsonbOrRaise:
    """Security field JSONB decoding must raise, not silently default."""

    def test_decodes_json_string_to_dict(self):
        result = decode_jsonb_or_raise('{"key": "value"}', "allowed_tools")
        assert result == {"key": "value"}

    def test_decodes_json_string_to_list(self):
        result = decode_jsonb_or_raise('["tool_a", "tool_b"]', "allowed_tools")
        assert result == ["tool_a", "tool_b"]

    def test_passes_through_dict(self):
        original = {"key": "value"}
        result = decode_jsonb_or_raise(original, "allowed_tools")
        assert result is original

    def test_passes_through_list(self):
        original = [1, 2, 3]
        result = decode_jsonb_or_raise(original, "denied_tools")
        assert result is original

    def test_returns_none_for_none(self):
        result = decode_jsonb_or_raise(None, "allowed_tools")
        assert result is None

    def test_raises_on_invalid_json_string(self):
        with pytest.raises(ValueError, match="allowed_tools"):
            decode_jsonb_or_raise("not valid json", "allowed_tools")

    def test_raises_on_empty_string(self):
        with pytest.raises(ValueError, match="allowed_resources"):
            decode_jsonb_or_raise("", "allowed_resources")

    def test_raises_on_non_string_non_dict_value(self):
        with pytest.raises(ValueError, match="denied_tools"):
            decode_jsonb_or_raise(42, "denied_tools")

    def test_raises_on_bytes_with_invalid_json(self):
        with pytest.raises(ValueError, match="allowed_tools"):
            decode_jsonb_or_raise(b"not json", "allowed_tools")

    def test_decodes_bytes_with_valid_json(self):
        result = decode_jsonb_or_raise(b'{"a": 1}', "allowed_tools")
        assert result == {"a": 1}

    def test_error_message_includes_field_name(self):
        with pytest.raises(ValueError, match="allowed_tools"):
            decode_jsonb_or_raise("broken", "allowed_tools")

    def test_error_message_includes_original_error(self):
        with pytest.raises(ValueError, match="JSONDecodeError|Expecting value"):
            decode_jsonb_or_raise("broken", "allowed_tools")


class TestParseJsonFieldStillSafe:
    """parse_json_field (non-security) still returns default on failure."""

    def test_returns_default_on_invalid_json(self):
        result = parse_json_field("not json", default=[])
        assert result == []

    def test_returns_default_on_none(self):
        result = parse_json_field(None, default={})
        assert result == {}

    def test_passes_through_dict(self):
        result = parse_json_field({"a": 1}, default={})
        assert result == {"a": 1}


class TestDecodeRowJsonb:
    """Repository-boundary JSONB decoding (Task 13)."""

    def test_decodes_security_fields_from_json_string(self):
        row = {
            "allowed_tools": '["tool_a"]',
            "denied_tools": '["tool_b"]',
            "allowed_resources": '["res1"]',
        }
        result = _decode_row_jsonb(row)
        assert result["allowed_tools"] == ["tool_a"]
        assert result["denied_tools"] == ["tool_b"]
        assert result["allowed_resources"] == ["res1"]

    def test_raises_on_corrupt_allowed_tools(self):
        row = {"allowed_tools": "broken json", "denied_tools": "[]", "allowed_resources": "[]"}
        with pytest.raises(ValueError, match="allowed_tools"):
            _decode_row_jsonb(row)

    def test_raises_on_corrupt_denied_tools(self):
        row = {"allowed_tools": "[]", "denied_tools": "broken", "allowed_resources": "[]"}
        with pytest.raises(ValueError, match="denied_tools"):
            _decode_row_jsonb(row)

    def test_raises_on_corrupt_allowed_resources(self):
        row = {"allowed_tools": "[]", "denied_tools": "[]", "allowed_resources": "broken"}
        with pytest.raises(ValueError, match="allowed_resources"):
            _decode_row_jsonb(row)

    def test_non_security_fields_default_on_parse_failure(self):
        row = {"tools": "broken", "resources": "also broken", "prompts": "bad"}
        result = _decode_row_jsonb(row)
        assert result["tools"] == []
        assert result["resources"] == []
        assert result["prompts"] == []

    def test_non_security_dict_fields_default_on_parse_failure(self):
        row = {"risk_policy": "broken", "tool_defaults": "bad"}
        result = _decode_row_jsonb(row)
        assert result["risk_policy"] == {}
        assert result["tool_defaults"] == {}

    def test_already_decoded_fields_pass_through(self):
        row = {
            "allowed_tools": ["tool_a"],
            "tools": [{"name": "tool1"}],
            "risk_policy": {"level": "high"},
        }
        result = _decode_row_jsonb(row)
        assert result["allowed_tools"] == ["tool_a"]
        assert result["tools"] == [{"name": "tool1"}]
        assert result["risk_policy"] == {"level": "high"}

    def test_none_security_fields_stay_none(self):
        row = {"allowed_tools": None, "denied_tools": None, "allowed_resources": None}
        result = _decode_row_jsonb(row)
        assert result["allowed_tools"] is None
        assert result["denied_tools"] is None
        assert result["allowed_resources"] is None

    def test_non_jsonb_fields_untouched(self):
        row = {"server_id": "srv1", "enabled": True, "binding_id": "b1"}
        result = _decode_row_jsonb(row)
        assert result["server_id"] == "srv1"
        assert result["enabled"] is True
        assert result["binding_id"] == "b1"
