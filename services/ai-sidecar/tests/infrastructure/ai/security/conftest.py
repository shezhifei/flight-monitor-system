"""pytest configuration for infrastructure tests.

Adds services/ai-sidecar to sys.path so that `from src.infrastructure...`
imports resolve correctly.
"""

from __future__ import annotations

import sys
from pathlib import Path

_SIDECAR_ROOT = str(Path(__file__).resolve().parents[4])
if _SIDECAR_ROOT not in sys.path:
    sys.path.insert(0, _SIDECAR_ROOT)
