"""Load the repository's canonical AI sidecar application for contract tests."""

from __future__ import annotations

import importlib.util
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parents[4]
_ENTRYPOINT = _REPO_ROOT / "scripts" / "host" / "ai_sidecar_entrypoint.py"
_SPEC = importlib.util.spec_from_file_location("fms_canonical_ai_sidecar_entrypoint", _ENTRYPOINT)
if _SPEC is None or _SPEC.loader is None:  # pragma: no cover - importlib platform guard
    raise RuntimeError(f"cannot load canonical AI sidecar entrypoint: {_ENTRYPOINT}")

_MODULE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MODULE)
app = _MODULE.app
