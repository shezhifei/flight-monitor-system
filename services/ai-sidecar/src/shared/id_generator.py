"""ID generator utilities.

Supports both common ULID Python APIs:
- `ulid.new()` (ulid-py style)
- `ulid.ULID()` (python-ulid style)
"""

from __future__ import annotations

import ulid


def _new_ulid() -> str:
    """Create a ULID string compatible with installed ulid package."""
    new_fn = getattr(ulid, "new", None)
    if callable(new_fn):
        return str(new_fn())

    ulid_cls = getattr(ulid, "ULID", None)
    if ulid_cls is not None:
        return str(ulid_cls())

    raise RuntimeError("No supported ULID generator found in 'ulid' module")


def generate_id(prefix: str | None = None) -> str:
    """Generate an ID.

    - default: 26-char ULID
    - with prefix: `<prefix>_<ULID>`
    """
    value = _new_ulid()
    if prefix:
        normalized = str(prefix).strip()
        if normalized:
            return f"{normalized}_{value}"
    return value


def generate_short_id(length: int = 8) -> str:
    return _new_ulid()[:length]
