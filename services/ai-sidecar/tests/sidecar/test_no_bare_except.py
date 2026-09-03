"""确保代码库中不存在无 noqa 注解的裸 `except Exception:`。"""

import json
import subprocess
import sys
from pathlib import Path


def test_no_bare_except_exception_without_noqa():
    src_dir = str(Path(__file__).resolve().parents[2] / "src")
    result = subprocess.run(
        [sys.executable, "-m", "ruff", "check", src_dir, "--select", "BLE001", "--output-format", "json"],
        capture_output=True,
        text=True,
    )
    issues = json.loads(result.stdout) if result.stdout.strip() else []
    bare_issues = [i for i in issues if "noqa" not in i.get("message", "").lower()]
    assert len(bare_issues) == 0, f"Found {len(bare_issues)} `except Exception` without noqa annotation."
