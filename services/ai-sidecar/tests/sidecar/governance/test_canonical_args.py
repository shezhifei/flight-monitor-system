"""Cross-language canonical args hash tests.

The locked vector matches the Rust implementation in
``services/api-server/crates/domain/src/canonical_args.rs``.
"""

from __future__ import annotations

import hashlib
import json

from src.infrastructure.ai.governance import (
    canonical_args_hash,
    canonical_json_args,
    tool_call_idempotency_key,
)

LOCKED_VECTOR = "883af60772bce150610af6602572e27c45bd883acc01ee9bfc072901d6e972d3"
LOCKED_INPUT = {
    "flight_id": "CA1234",
    "metadata": {"airport": "PEK", "gate": "B12"},
    "status": "ON_TIME",
    "tags": ["priority", "vip"],
}


def test_canonical_json_args_matches_python_reference() -> None:
    expected = (
        '{"flight_id":"CA1234",'
        '"metadata":{"airport":"PEK","gate":"B12"},'
        '"status":"ON_TIME",'
        '"tags":["priority","vip"]}'
    )
    assert canonical_json_args(LOCKED_INPUT) == expected
    assert canonical_args_hash(LOCKED_INPUT) == LOCKED_VECTOR


def test_nested_object_keys_are_sorted_recursively() -> None:
    args = {
        "outer": {"z": 1, "a": {"y": 2, "b": 3}},
        "first": "value",
    }
    assert canonical_json_args(args) == (
        '{"first":"value","outer":{"a":{"b":3,"y":2},"z":1}}'
    )


def test_array_order_is_preserved() -> None:
    args = {"items": [3, 1, 2]}
    assert canonical_json_args(args) == '{"items":[3,1,2]}'


def test_null_values_are_distinct_from_missing_keys() -> None:
    explicit_null = canonical_json_args({"a": None})
    missing = canonical_json_args({})
    assert explicit_null == '{"a":null}'
    assert missing == "{}"
    assert explicit_null != missing


def test_non_ascii_characters_are_preserved() -> None:
    args = {"name": "航班CA1234", "city": "北京"}
    assert canonical_json_args(args) == '{"city":"北京","name":"航班CA1234"}'


def test_empty_object_and_empty_array_serialize() -> None:
    assert canonical_json_args({}) == "{}"
    assert canonical_json_args({"x": []}) == '{"x":[]}'


def test_hash_is_deterministic_across_key_orders() -> None:
    a = {"flight_id": "CA1234", "status": "ON_TIME"}
    b = {"status": "ON_TIME", "flight_id": "CA1234"}
    assert canonical_args_hash(a) == canonical_args_hash(b)


def test_idempotency_key_format_is_stable() -> None:
    key = tool_call_idempotency_key(
        run_id="run-1",
        round_index=0,
        tool_call_id="call-1",
        tool_name="flight_status_lookup",
        args={"flight_id": "CA1234"},
    )
    assert key.startswith("run-1:0:call-1:flight_status_lookup:")
    suffix = key.rsplit(":", 1)[-1]
    assert len(suffix) == 64
    assert suffix == hashlib.sha256(
        canonical_json_args({"flight_id": "CA1234"}).encode("utf-8")
    ).hexdigest()


def test_idempotency_key_is_stable_for_same_input() -> None:
    args = {"flight_id": "CA1234"}
    key_a = tool_call_idempotency_key("run-1", 0, "call-1", "flight_status_lookup", args)
    key_b = tool_call_idempotency_key("run-1", 0, "call-1", "flight_status_lookup", args)
    assert key_a == key_b


def test_idempotency_key_differs_for_different_args() -> None:
    base = {
        "run_id": "run-1",
        "round_index": 0,
        "tool_call_id": "call-1",
        "tool_name": "flight_status_lookup",
    }
    key_a = tool_call_idempotency_key(args={"flight_id": "CA1234"}, **base)
    key_b = tool_call_idempotency_key(args={"flight_id": "CA5678"}, **base)
    assert key_a != key_b


def test_idempotency_key_round_trip_against_rust_reference() -> None:
    args = LOCKED_INPUT
    key = tool_call_idempotency_key("run-42", 3, "call-99", "flight_status_lookup", args)
    expected_suffix = LOCKED_VECTOR
    assert key == f"run-42:3:call-99:flight_status_lookup:{expected_suffix}"


def test_canonical_hash_matches_manual_serde_python_form() -> None:
    args = {"flight_id": "CA1234", "metadata": {"airport": "PEK", "gate": "B12"}}
    expected = hashlib.sha256(
        json.dumps(args, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    ).hexdigest()
    assert canonical_args_hash(args) == expected
