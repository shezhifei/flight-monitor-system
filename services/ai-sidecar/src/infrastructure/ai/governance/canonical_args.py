"""Canonical args serialization and hashing.

Mirrors ``services/api-server/crates/domain/src/canonical_args.rs``. The
Python and Rust implementations must produce byte-identical canonical
JSON for the same logical input so that the cross-language idempotency
key is stable.

Rules:

* object keys sorted alphabetically (recursively);
* no whitespace between tokens;
* non-ASCII characters are preserved (``ensure_ascii=False``);
* explicit ``null`` and missing keys are distinct.
"""

from __future__ import annotations

import hashlib
import json
from typing import Any

ALGORITHM = "sha256"


def canonical_json_args(args: dict[str, Any]) -> str:
    """Serialize ``args`` to the canonical JSON form used for idempotency hashing."""
    return json.dumps(
        args,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    )


def canonical_args_hash(args: dict[str, Any]) -> str:
    """Return the SHA-256 hex digest of :func:`canonical_json_args`."""
    return hashlib.sha256(canonical_json_args(args).encode("utf-8")).hexdigest()


def tool_call_idempotency_key(
    run_id: str,
    round_index: int,
    tool_call_id: str,
    tool_name: str,
    args: dict[str, Any],
) -> str:
    """Build the canonical idempotency key for a single tool call.

    Shape: ``run_id:round_index:tool_call_id:tool_name:canonical_args_hash``.
    """
    return f"{run_id}:{round_index}:{tool_call_id}:{tool_name}:{canonical_args_hash(args)}"


__all__ = [
    "ALGORITHM",
    "canonical_args_hash",
    "canonical_json_args",
    "tool_call_idempotency_key",
]
