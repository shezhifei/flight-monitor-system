"""Drive the shipped performance regression validator, including Criterion artifacts."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "validate_performance_regression.py"
CI_YML = ROOT / ".github" / "workflows" / "ci.yml"


def _write(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload), encoding="utf-8")


def _criterion_estimates(mean: float, std_dev: float = 0.0) -> dict:
    return {
        "mean": {"point_estimate": mean, "standard_error": 0.0},
        "median": {"point_estimate": mean, "standard_error": 0.0},
        "std_dev": {"point_estimate": std_dev, "standard_error": 0.0},
    }


def _run(baseline: Path, current: Path, output: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            sys.executable,
            str(SCRIPT),
            "--baseline",
            str(baseline),
            "--current",
            str(current),
            "--output",
            str(output),
        ],
        check=False,
        capture_output=True,
        text=True,
    )


def test_validator_blocks_when_p95_grows_more_than_five_percent(tmp_path: Path) -> None:
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    output = tmp_path / "report.json"
    _write(
        baseline,
        {"benchmarks": {"serde_json_to_string": {"p95": 100.0, "mean": 80.0, "throughput": 1000}}},
    )
    _write(
        current,
        {"benchmarks": {"serde_json_to_string": {"p95": 106.0, "mean": 81.0, "throughput": 990}}},
    )

    result = _run(baseline, current, output)
    assert result.returncode == 1, result.stdout + result.stderr
    report = json.loads(output.read_text(encoding="utf-8"))
    assert report["recommendation"] == "BLOCKED"
    assert report["metrics"]["serde_json_to_string"]["p95"]["change_percent"] == 6.0


def test_validator_approves_when_p95_stays_within_five_percent(tmp_path: Path) -> None:
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    output = tmp_path / "report.json"
    _write(
        baseline,
        {"benchmarks": {"serde_json_to_string": {"p95": 100.0, "throughput": 1000}}},
    )
    _write(
        current,
        {"benchmarks": {"serde_json_to_string": {"p95": 104.0, "throughput": 1010}}},
    )

    result = _run(baseline, current, output)
    assert result.returncode == 0, result.stdout + result.stderr
    report = json.loads(output.read_text(encoding="utf-8"))
    assert report["recommendation"] == "APPROVE"


def test_validator_parses_criterion_estimates_json_and_blocks_six_percent(tmp_path: Path) -> None:
    baseline = tmp_path / "baseline" / "serde_json_to_string" / "new" / "estimates.json"
    current = tmp_path / "current" / "serde_json_to_string" / "new" / "estimates.json"
    output = tmp_path / "report.json"
    _write(baseline, _criterion_estimates(100.0, 0.0))
    _write(current, _criterion_estimates(106.0, 0.0))

    result = _run(baseline.parent.parent.parent, current.parent.parent.parent, output)
    assert result.returncode == 1, result.stdout + result.stderr
    report = json.loads(output.read_text(encoding="utf-8"))
    assert report["recommendation"] == "BLOCKED"
    change = report["metrics"]["serde_json_to_string"]["p95"]["change_percent"]
    assert change == 6.0


def test_validator_rejects_same_path_without_compare_slots(tmp_path: Path) -> None:
    tree = tmp_path / "criterion"
    output = tmp_path / "report.json"
    _write(tree / "simd_json_to_string" / "base" / "estimates.json", _criterion_estimates(200.0, 0.0))
    _write(tree / "simd_json_to_string" / "new" / "estimates.json", _criterion_estimates(212.0, 0.0))

    result = _run(tree, tree, output)
    assert result.returncode == 1, result.stdout + result.stderr
    assert "same path" in (result.stdout + result.stderr)


def test_validator_compares_criterion_base_vs_new_with_compare_slots(tmp_path: Path) -> None:
    tree = tmp_path / "criterion"
    output = tmp_path / "report.json"
    _write(tree / "simd_json_to_string" / "base" / "estimates.json", _criterion_estimates(200.0, 0.0))
    _write(tree / "simd_json_to_string" / "new" / "estimates.json", _criterion_estimates(212.0, 0.0))

    result = subprocess.run(
        [
            sys.executable,
            str(SCRIPT),
            "--baseline",
            str(tree),
            "--current",
            str(tree),
            "--compare-slots",
            "--output",
            str(output),
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 1, result.stdout + result.stderr
    report = json.loads(output.read_text(encoding="utf-8"))
    assert report["recommendation"] == "BLOCKED"
    assert report["metrics"]["simd_json_to_string"]["p95"]["change_percent"] == 6.0


def test_validator_p95_from_criterion_sample_times(tmp_path: Path) -> None:
    import importlib.util

    estimates = tmp_path / "bench" / "new" / "estimates.json"
    sample = estimates.with_name("sample.json")
    _write(estimates, _criterion_estimates(10.0, 1.0))
    # 20 samples; p95 index is ceil(0.95*20)-1 = 18 → value 19.0 when per-iter is 1..20.
    times = [float(i + 1) for i in range(20)]
    iters = [1.0] * 20
    _write(sample, {"sampling_mode": "Linear", "iters": iters, "times": times})

    spec = importlib.util.spec_from_file_location("validate_performance_regression", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    loaded = module.load_benchmark_report(str(estimates))
    assert "bench" in loaded
    assert loaded["bench"]["p95"] == 19.0


def test_ci_workflow_uses_distinct_criterion_baseline_and_current() -> None:
    text = CI_YML.read_text(encoding="utf-8")
    assert "cargo bench -p fms-benches --bench simd_json_benchmarks" in text
    assert "--baseline base" in text
    assert "validate_performance_regression.py" in text
    assert "--baseline services/api-server/benches/criterion-baseline" in text
    assert "--current services/api-server/target/criterion" in text
    assert "services/api-server/benches/baseline.json" not in text
    baseline_line = next(
        line for line in text.splitlines() if "--baseline services/api-server/" in line
    )
    current_line = next(line for line in text.splitlines() if "--current services/api-server/" in line)
    assert "criterion-baseline" in baseline_line
    assert "target/criterion" in current_line
    assert baseline_line.strip() != current_line.strip()
