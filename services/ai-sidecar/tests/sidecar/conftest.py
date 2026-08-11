"""pytest configuration for ai-sidecar tests.

Adds services/ai-sidecar to sys.path so that `from src.infrastructure...`
imports resolve correctly regardless of the working directory (repo root,
services/ai-sidecar, or any other directory).
"""

from __future__ import annotations

import sys
from pathlib import Path

# services/ai-sidecar is two levels up from this conftest.py
_SIDECAR_ROOT = str(Path(__file__).resolve().parents[2])
if _SIDECAR_ROOT not in sys.path:
    sys.path.insert(0, _SIDECAR_ROOT)

# The sidecar imports the shared ``config`` package from the repo root.
_REPO_ROOT = str(Path(__file__).resolve().parents[4])
if _REPO_ROOT not in sys.path:
    sys.path.insert(0, _REPO_ROOT)

# Also add the test directory itself so test-to-test imports resolve
_TEST_DIR = str(Path(__file__).resolve().parent)
if _TEST_DIR not in sys.path:
    sys.path.insert(0, _TEST_DIR)
