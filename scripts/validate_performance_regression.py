#!/usr/bin/env python3
"""Compare performance benchmarks and fail when P95 grows more than 5%.

Accepts:
  - wrapper JSON: {"benchmarks": {"name": {"p95": ..., "mean": ...}}}
  - a Criterion estimates.json file
  - a Criterion output directory (target/criterion), including base/ vs new/
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


P95_FAIL_PERCENT = 5.0
THROUGHPUT_WARN_PERCENT = 10.0
Z95 = 1.64485


def _as_float(value: Any) -> float | None:
    if value is None:
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _point_estimate(block: Any) -> float | None:
    if isinstance(block, (int, float)):
        return float(block)
    if isinstance(block, dict):
        return _as_float(block.get("point_estimate"))
    return None


def _p95_from_sample(sample_path: Path) -> float | None:
    try:
        sample = json.loads(sample_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    times = sample.get("times")
    iters = sample.get("iters")
    if not isinstance(times, list) or not isinstance(iters, list) or len(times) != len(iters):
        return None
    per_iter = sorted(
        float(time) / float(count) for time, count in zip(times, iters) if count
    )
    if not per_iter:
        return None
    index = min(len(per_iter) - 1, max(0, math.ceil(0.95 * len(per_iter)) - 1))
    return per_iter[index]


def _metrics_from_criterion_estimates(est_path: Path) -> dict[str, float]:
    data = json.loads(est_path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"Criterion estimates.json is not an object: {est_path}")

    out: dict[str, float] = {}
    mean = _point_estimate(data.get("mean"))
    median = _point_estimate(data.get("median"))
    std_dev = _point_estimate(data.get("std_dev"))
    if mean is not None:
        out["mean"] = mean
    if median is not None:
        out["median"] = median

    p95 = _p95_from_sample(est_path.with_name("sample.json"))
    if p95 is None and mean is not None and std_dev is not None:
        p95 = mean + Z95 * std_dev
    if p95 is None:
        p95 = mean
    if p95 is not None:
        out["p95"] = p95
    return out


def _metric_block(raw: dict[str, Any]) -> dict[str, float]:
    out: dict[str, float] = {}
    for key in ("p95", "p99", "mean", "median", "throughput"):
        parsed = _as_float(raw.get(key))
        if parsed is not None:
            out[key] = parsed
    nested = raw.get("benchmark")
    if isinstance(nested, dict):
        out.update(_metric_block(nested))
    mean = _point_estimate(raw.get("mean"))
    if mean is not None:
        out.setdefault("mean", mean)
    std_dev = _point_estimate(raw.get("std_dev"))
    if "p95" not in out and mean is not None and std_dev is not None:
        out["p95"] = mean + Z95 * std_dev
    return out


def _criterion_slot_dirs(root: Path) -> dict[str, list[Path]]:
    slots: dict[str, list[Path]] = {"new": [], "base": []}
    for estimates in root.rglob("estimates.json"):
        slot = estimates.parent.name
        if slot in slots:
            slots[slot].append(estimates)
    return slots


def _load_criterion_tree(root: Path, slot: str) -> dict[str, dict[str, float]]:
    benches: dict[str, dict[str, float]] = {}
    for estimates in sorted(root.rglob("estimates.json")):
        if estimates.parent.name != slot:
            continue
        name = estimates.parent.parent.name
        if name in {"report", "change"}:
            continue
        benches[name] = _metrics_from_criterion_estimates(estimates)
    if not benches:
        raise ValueError(f"No Criterion {slot}/estimates.json files under {root}")
    return benches


def load_benchmark_report(report_path: str, *, slot: str | None = None) -> dict[str, dict[str, float]]:
    path = Path(report_path)
    if path.is_dir():
        chosen = slot or "new"
        try:
            return _load_criterion_tree(path, chosen)
        except ValueError:
            if chosen == "new":
                return _load_criterion_tree(path, "base")
            raise

    if not path.is_file():
        raise FileNotFoundError(report_path)
    if path.suffix.lower() == ".html":
        raise ValueError("HTML Criterion reports are not accepted; pass estimates.json or a Criterion directory")

    if path.name == "estimates.json":
        parent = path.parent.name
        name = path.parent.parent.name if parent in {"new", "base"} else path.stem
        return {name: _metrics_from_criterion_estimates(path)}

    data = json.loads(path.read_text(encoding="utf-8"))
    benches: dict[str, dict[str, float]] = {}

    if isinstance(data, dict) and isinstance(data.get("benchmarks"), dict):
        for name, raw in data["benchmarks"].items():
            if isinstance(raw, dict):
                benches[name] = _metric_block(raw)
        if benches:
            return benches

    if isinstance(data, dict) and ("mean" in data or "std_dev" in data) and isinstance(data.get("mean"), dict):
        name = path.parent.parent.name if path.parent.name in {"new", "base"} else path.stem
        return {name: _metrics_from_criterion_estimates(path)}

    if isinstance(data, dict) and any(k in data for k in ("p95", "mean", "benchmark")):
        benches["default"] = _metric_block(data)
        return benches

    raise ValueError(f"Unsupported benchmark JSON shape in {report_path}")


def _same_criterion_tree(baseline: Path, current: Path) -> bool:
    try:
        left = baseline.resolve()
        right = current.resolve()
    except OSError:
        return False
    if left != right or not left.is_dir():
        return False
    slots = _criterion_slot_dirs(left)
    return bool(slots["base"] and slots["new"])


def compare_benchmarks(
    baseline: dict[str, dict[str, float]],
    current: dict[str, dict[str, float]],
    p95_fail_percent: float = P95_FAIL_PERCENT,
) -> dict[str, Any]:
    names = sorted(set(baseline) | set(current))
    metrics: dict[str, Any] = {}
    p95_blocked = False
    throughput_warn = False

    for name in names:
        left = baseline.get(name, {})
        right = current.get(name, {})
        row: dict[str, Any] = {}
        for key in ("p95", "p99", "mean", "median", "throughput"):
            old = left.get(key)
            new = right.get(key)
            if old is None or new is None or old == 0:
                continue
            change_pct = ((new - old) / old) * 100.0
            row[key] = {
                "baseline": old,
                "current": new,
                "change_percent": round(change_pct, 2),
            }
            if key == "p95" and change_pct > p95_fail_percent:
                p95_blocked = True
            if key == "throughput" and change_pct < -THROUGHPUT_WARN_PERCENT:
                throughput_warn = True
        if row:
            metrics[name] = row

    if p95_blocked:
        recommendation = "BLOCKED"
    elif throughput_warn:
        recommendation = "REVIEW_REQUIRED"
    else:
        recommendation = "APPROVE"

    return {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "metrics": metrics,
        "recommendation": recommendation,
        "p95_fail_percent": p95_fail_percent,
    }


def write_report(comparison: dict[str, Any], output_file: str | None) -> None:
    print(json.dumps(comparison, indent=2, ensure_ascii=False))
    if output_file:
        output_path = Path(output_file)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(comparison, indent=2), encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Validate benchmark regression")
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--current", required=True)
    parser.add_argument("--output", default="performance_validation.json")
    parser.add_argument("--p95-threshold", type=float, default=P95_FAIL_PERCENT)
    parser.add_argument(
        "--compare-slots",
        action="store_true",
        help="When --baseline and --current are the same Criterion tree, compare base/ vs new/",
    )
    args = parser.parse_args(argv)

    try:
        baseline_path = Path(args.baseline)
        current_path = Path(args.current)
        same_tree = False
        try:
            same_tree = baseline_path.resolve() == current_path.resolve()
        except OSError:
            same_tree = False
        if same_tree and not args.compare_slots:
            print(
                "error: --baseline and --current resolve to the same path; "
                "pass distinct Criterion trees (committed baseline vs target/criterion) "
                "or set --compare-slots to compare base/ vs new/",
                file=sys.stderr,
            )
            return 1
        if args.compare_slots and _same_criterion_tree(baseline_path, current_path):
            baseline = load_benchmark_report(args.baseline, slot="base")
            current = load_benchmark_report(args.current, slot="new")
        else:
            baseline = load_benchmark_report(args.baseline, slot="new")
            current = load_benchmark_report(args.current, slot="new")
    except (FileNotFoundError, ValueError, json.JSONDecodeError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    if not current:
        print("error: current benchmark report is empty", file=sys.stderr)
        return 1

    comparison = compare_benchmarks(baseline, current, args.p95_threshold)
    write_report(comparison, args.output)
    if comparison["recommendation"] == "BLOCKED":
        print("P95 latency grew more than the 5% gate; blocking merge.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
