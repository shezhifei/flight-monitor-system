"""Verify that wildcard re-export compat modules have been removed."""

from pathlib import Path

SIDECAR_ROOT = Path(__file__).resolve().parents[2] / "src/application/services/ai"


def test_nl_query_service_facade_removed():
    path = SIDECAR_ROOT / "nl_query_service.py"
    assert not path.is_file(), f"{path} should not exist"


def test_llm_eval_service_facade_removed():
    path = SIDECAR_ROOT / "llm_eval_service.py"
    assert not path.is_file(), f"{path} should not exist"
