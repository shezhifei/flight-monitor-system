"""Utility helpers for approval diff generation.

Uses deepdiff for robust, RFC 6902-style JSON diff computation.
"""

from __future__ import annotations

from typing import Any

from deepdiff import DeepDiff


def build_json_patch(
    before_snapshot: Any,
    after_snapshot: Any,
) -> tuple[list[dict[str, Any]], dict[str, int]]:
    """Build a JSON-patch-like diff and summary counters.

    The return format is intentionally simple and stable:
    - ``json_patch``: list of dict operations (``add``, ``remove``, ``replace``)
    - ``summary``: ``{adds, updates, deletes}``
    """
    before = before_snapshot if before_snapshot is not None else {}
    after = after_snapshot if after_snapshot is not None else {}

    diff = DeepDiff(before, after, verbose_level=2)

    patches: list[dict[str, Any]] = []
    summary = {"adds": 0, "updates": 0, "deletes": 0}

    # --- added items ---
    for category in ("dictionary_item_added", "iterable_item_added"):
        for path, value in diff.get(category, {}).items():
            patches.append({"op": "add", "path": _normalize_path(path), "value": value})
            summary["adds"] += 1

    # --- removed items ---
    for category in ("dictionary_item_removed", "iterable_item_removed"):
        for path, value in diff.get(category, {}).items():
            patches.append({"op": "remove", "path": _normalize_path(path), "old_value": value})
            summary["deletes"] += 1

    # --- changed values ---
    for path, change in diff.get("values_changed", {}).items():
        patches.append(
            {
                "op": "replace",
                "path": _normalize_path(path),
                "old_value": change.get("old_value"),
                "value": change.get("new_value"),
            }
        )
        summary["updates"] += 1

    # --- type changes (treated as replace) ---
    for path, change in diff.get("type_changes", {}).items():
        patches.append(
            {
                "op": "replace",
                "path": _normalize_path(path),
                "old_value": change.get("old_value"),
                "value": change.get("new_value"),
            }
        )
        summary["updates"] += 1

    return patches, summary


def _normalize_path(deepdiff_path: str) -> str:
    """Convert a DeepDiff path like ``root['key'][0]`` to a JSON Pointer-like ``/key/0``.

    This is intentionally simple and only handles dict-key and list-index
    access patterns produced by DeepDiff.
    """
    import re

    # Strip the leading "root"
    path = deepdiff_path
    if path.startswith("root"):
        path = path[4:]

    # Replace ['key'] and [0] with /key and /0
    segments: list[str] = []
    for match in re.finditer(r"\[(\d+)\]|\['([^']+)'\]|\[\"([^\"]+)\"\]", path):
        index, single_key, double_key = match.groups()
        segment = index if index is not None else (single_key or double_key or "")
        # JSON Pointer escaping
        segment = segment.replace("~", "~0").replace("/", "~1")
        segments.append(segment)

    if not segments:
        return "/"
    return "/" + "/".join(segments)
