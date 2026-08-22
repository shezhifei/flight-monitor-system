"""Structural + behavioral checks for the performance pre-commit hook."""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[2]
HOOK = ROOT / ".git_hooks" / "pre-commit-performance-check.sh"


def _lf(text: str | bytes) -> str:
    raw = text.encode("utf-8") if isinstance(text, str) else text
    return raw.replace(b"\r\n", b"\n").replace(b"\r", b"\n").decode("utf-8")


def test_hook_script_rejects_pool_size_over_64() -> None:
    text = HOOK.read_text(encoding="utf-8")
    assert "DB_POOL_SIZE" in text
    assert "64" in text
    assert "ACTIX_WORKERS" in text
    assert "nproc" in text or "_NPROCESSORS_ONLN" in text


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash is required to execute the hook")
def test_hook_fails_when_env_example_pool_exceeds_64(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / ".git_hooks").mkdir()
    script = _lf(HOOK.read_bytes())
    (repo / ".git_hooks" / "pre-commit-performance-check.sh").write_bytes(script.encode("utf-8"))
    (repo / ".env.example").write_bytes(b"DB_POOL_SIZE=80\nACTIX_WORKERS=2\n")
    subprocess.run(["git", "init"], cwd=repo, check=True, capture_output=True)
    monkeypatch.chdir(repo)
    result = subprocess.run(
        ["bash", "-s"],
        input=script.encode("utf-8"),
        check=False,
        capture_output=True,
        cwd=str(repo),
    )
    combined = result.stdout.decode("utf-8", errors="replace") + result.stderr.decode("utf-8", errors="replace")
    assert result.returncode != 0, combined
    assert "exceeds maximum recommended value of 64" in combined
