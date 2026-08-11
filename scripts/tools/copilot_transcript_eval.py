#!/usr/bin/env python3
"""Run AI Copilot transcript samples against the draft diagnostic endpoint."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

# Error stages that indicate model/schema/parser/validation failures.
_MODEL_SCHEMA_STAGES = frozenset({
    "schema",
    "model",
    "parser",
    "validation",
    "json_parse",
    "llm_output",
    "response",
})
SUMMARY_SCHEMA_VERSION = 2
_REGRESSION_COUNTERS = frozenset({
    "errors",
    "transport_errors",
    "model_schema_errors",
    "expectation_mismatches",
    "input_errors",
})


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Evaluate Copilot transcript extraction diagnostics.")
    parser.add_argument("--base-url", default=os.environ.get("FMS_API_BASE_URL", "http://127.0.0.1:8000/api/v2"))
    parser.add_argument("--token", default=os.environ.get("FMS_AUTH_TOKEN"), help="Bearer token; defaults to FMS_AUTH_TOKEN")
    parser.add_argument("--input", required=True, help="JSONL file with transcript samples")
    parser.add_argument(
        "--output",
        required=True,
        help=(
            "Output JSONL file for legacy mode, or a run archive root directory when the path is an "
            "existing directory or has no file suffix"
        ),
    )
    parser.add_argument("--entity-id", default="flight-monitor-copilot", help="Default AI entity id")
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--fail-on-mismatch", action="store_true", help="Fail with exit code 1 if any expectation fails to match.")
    parser.add_argument("--run-id", help="Optional run id used for archive directory names.")
    parser.add_argument(
        "--transcript-preview-chars",
        type=int,
        default=160,
        help="Maximum transcript preview characters stored in outputs; set to 0 to disable previews.",
    )
    parser.add_argument(
        "--include-transcripts",
        action="store_true",
        help="Explicitly store full transcript text in outputs for local debugging. Do not use for long-lived archives.",
    )
    parser.add_argument(
        "--baseline-summary",
        help="Optional previous summary.json used to calculate totals deltas and regression counters.",
    )
    return parser.parse_args()


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_no, line in enumerate(handle, 1):
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            try:
                item = json.loads(stripped)
            except json.JSONDecodeError as exc:
                raise SystemExit(f"{path}:{line_no}: invalid JSON: {exc}") from exc
            if not isinstance(item, dict):
                raise SystemExit(f"{path}:{line_no}: sample must be a JSON object")
            items.append(item)
    return items


def post_json(url: str, token: str | None, payload: dict[str, Any], timeout: float) -> tuple[int, dict[str, Any]]:
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, data=body, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            data = json.loads(response.read().decode("utf-8"))
            return response.status, data
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            data = {"message": raw}
        return exc.code, data


def expected_list(expected: dict[str, Any], key: str) -> list[str]:
    value = expected.get(key)
    if isinstance(value, list):
        return [str(item).strip() for item in value if str(item).strip()]
    if isinstance(value, str) and value.strip():
        return [value.strip()]
    return []


def field_values(actions: list[dict[str, Any]], field_name: str) -> list[str]:
    values: list[str] = []
    for action in actions:
        fields = action.get("fields")
        if isinstance(fields, dict) and fields.get(field_name) is not None:
            values.append(str(fields[field_name]).strip())
    return values


def field_values_by_name(actions: list[dict[str, Any]]) -> dict[str, list[str]]:
    values: dict[str, list[str]] = {}
    for action in actions:
        fields = action.get("fields")
        if not isinstance(fields, dict):
            continue
        for field_name, field_value in fields.items():
            if field_value is None:
                continue
            values.setdefault(str(field_name), []).append(str(field_value).strip())
    return values


def score_result(diagnostic: dict[str, Any], expected: dict[str, Any]) -> dict[str, Any]:
    parsed = diagnostic.get("parsed_payload")
    actions = parsed.get("actions", []) if isinstance(parsed, dict) else []
    if not isinstance(actions, list):
        actions = []

    expected_case_types = expected_list(expected, "case_types")
    expected_flight_numbers = expected_list(expected, "flight_numbers")
    expected_fields = expected.get("fields") if isinstance(expected.get("fields"), dict) else {}
    expected_action_count = expected.get("action_count")
    if not isinstance(expected_action_count, int) or expected_action_count < 0:
        expected_action_count = None

    actual_case_types = [str(action.get("case_type", "")).strip() for action in actions if isinstance(action, dict)]
    actual_flight_numbers = [
        str(action.get("flight_number_raw", "")).strip() for action in actions if isinstance(action, dict)
    ]
    actual_fields = field_values_by_name([a for a in actions if isinstance(a, dict)])

    missing_case_types = [item for item in expected_case_types if item not in actual_case_types]
    missing_flight_numbers = [item for item in expected_flight_numbers if item not in actual_flight_numbers]
    action_count_mismatch = (
        {"expected": expected_action_count, "actual": len(actions)}
        if expected_action_count is not None and len(actions) != expected_action_count
        else None
    )
    missing_fields: dict[str, list[str]] = {}
    for field_name, expected_value in expected_fields.items():
        expected_values = expected_value if isinstance(expected_value, list) else [expected_value]
        actual_values = field_values([a for a in actions if isinstance(a, dict)], str(field_name))
        missing = [str(item).strip() for item in expected_values if str(item).strip() not in actual_values]
        if missing:
            missing_fields[str(field_name)] = missing

    return {
        "action_count": len(actions),
        "actual_case_types": actual_case_types,
        "actual_flight_numbers": actual_flight_numbers,
        "actual_fields": actual_fields,
        "missing_case_types": missing_case_types,
        "missing_flight_numbers": missing_flight_numbers,
        "action_count_mismatch": action_count_mismatch,
        "missing_fields": missing_fields,
        "expectations_passed": not missing_case_types
        and not missing_flight_numbers
        and action_count_mismatch is None
        and not missing_fields,
    }


def is_model_schema_error(result: dict[str, Any]) -> bool:
    """Return True when the failure indicates a model/schema/parser issue."""
    if result.get("ok"):
        return False
    stage = str(result.get("error_stage") or "").lower()
    return stage in _MODEL_SCHEMA_STAGES


def utc_now() -> dt.datetime:
    return dt.datetime.now(dt.UTC).replace(microsecond=0)


def format_timestamp(value: dt.datetime) -> str:
    return value.isoformat().replace("+00:00", "Z")


def safe_name(value: str) -> str:
    sanitized = re.sub(r"[^A-Za-z0-9._-]+", "-", value.strip())
    return sanitized.strip("-._") or "sample"


def transcript_metadata(transcript: str, preview_chars: int, include_transcript: bool) -> dict[str, Any]:
    normalized_preview = " ".join(transcript.split())
    metadata: dict[str, Any] = {
        "transcript_sha256": hashlib.sha256(transcript.encode("utf-8")).hexdigest(),
        "transcript_chars": len(transcript),
    }
    if preview_chars > 0:
        metadata["transcript_preview"] = normalized_preview[:preview_chars]
    if include_transcript:
        metadata["transcript"] = transcript
    return metadata


def wants_archive_output(output_path: Path) -> bool:
    if output_path.exists():
        return output_path.is_dir()
    return output_path.suffix == ""


def build_run_id(value: str | None) -> str:
    if value:
        return safe_name(value)
    return utc_now().strftime("%Y%m%dT%H%M%SZ")


def classify_result(result: dict[str, Any]) -> str | None:
    if result.get("transport_error"):
        return "transport_errors"
    if result.get("input_error"):
        return "input_errors"
    if is_model_schema_error(result):
        return "model_schema_errors"
    if result.get("ok") and not result.get("expectations_passed"):
        return "expectation_mismatches"
    if not result.get("ok"):
        status = result.get("http_status")
        if isinstance(status, int) and status >= 400:
            return "transport_errors"
        return "model_schema_errors"
    return None


def diagnostic_entry(result: dict[str, Any]) -> dict[str, Any]:
    entry: dict[str, Any] = {
        "id": result.get("id"),
        "category": classify_result(result),
        "ok": bool(result.get("ok")),
    }
    for key in (
        "http_status",
        "error_stage",
        "error_message",
        "missing_case_types",
        "missing_flight_numbers",
        "action_count_mismatch",
        "missing_fields",
        "candidate_case_types",
        "transcript_sha256",
        "transcript_chars",
    ):
        if key in result:
            entry[key] = result[key]
    return entry


class OutputArchive:
    def __init__(self, output_arg: str, run_id: str | None) -> None:
        requested_path = Path(output_arg)
        self.archive_mode = wants_archive_output(requested_path)
        self.run_id = build_run_id(run_id)
        self.root_path = requested_path
        self.run_path = requested_path
        if self.archive_mode:
            self.root_path.mkdir(parents=True, exist_ok=True)
            self.run_path = self.root_path / self.run_id
            if self.run_path.exists():
                suffix = utc_now().strftime("%H%M%S")
                self.run_path = self.root_path / f"{self.run_id}-{suffix}"
            self.run_path.mkdir(parents=True, exist_ok=False)
            self.failures_path = self.run_path / "failures"
            self.failures_path.mkdir(parents=True, exist_ok=True)
            self.results_path = self.run_path / "results.jsonl"
            self.summary_path = self.run_path / "summary.json"
        else:
            requested_path.parent.mkdir(parents=True, exist_ok=True)
            self.results_path = requested_path
            self.summary_path = None
            self.failures_path = None

    def write_failure(self, result: dict[str, Any], index: int) -> None:
        if self.failures_path is None:
            return
        category = classify_result(result)
        if category is None:
            return
        sample_id = str(result.get("id") or f"sample-{index}")
        failure_path = self.failures_path / f"{index:04d}-{safe_name(sample_id)}-{category}.json"
        with failure_path.open("w", encoding="utf-8") as handle:
            json.dump(result, handle, ensure_ascii=False, indent=2, sort_keys=True)
            handle.write("\n")

    def write_summary(self, summary: dict[str, Any]) -> None:
        if self.summary_path is None:
            return
        with self.summary_path.open("w", encoding="utf-8") as handle:
            json.dump(summary, handle, ensure_ascii=False, indent=2, sort_keys=True)
            handle.write("\n")


def new_totals() -> dict[str, int]:
    return {
        "samples": 0,
        "ok": 0,
        "expectations_passed": 0,
        "errors": 0,
        "transport_errors": 0,
        "model_schema_errors": 0,
        "expectation_mismatches": 0,
        "input_errors": 0,
    }


def read_baseline_summary(path: Path) -> dict[str, Any]:
    try:
        with path.open("r", encoding="utf-8") as handle:
            summary = json.load(handle)
    except FileNotFoundError as exc:
        raise SystemExit(f"baseline summary not found: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"{path}: invalid baseline summary JSON: {exc}") from exc
    if not isinstance(summary, dict):
        raise SystemExit(f"{path}: baseline summary must be a JSON object")
    if not isinstance(summary.get("totals"), dict):
        raise SystemExit(f"{path}: baseline summary must contain a totals object")
    return summary


def int_counter(value: Any) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    return 0


def build_baseline_compare(current_totals: dict[str, int], baseline_path: Path) -> dict[str, Any]:
    baseline_summary = read_baseline_summary(baseline_path)
    baseline_totals_raw = baseline_summary["totals"]
    baseline_totals = {
        str(key): int_counter(value)
        for key, value in baseline_totals_raw.items()
        if isinstance(key, str)
    }
    counter_names = sorted(set(new_totals()) | set(baseline_totals) | set(current_totals))
    deltas = {
        key: int_counter(current_totals.get(key)) - int_counter(baseline_totals.get(key))
        for key in counter_names
    }
    regressions = {
        key: delta
        for key, delta in deltas.items()
        if key in _REGRESSION_COUNTERS and delta > 0
    }
    return {
        "baseline_summary_path": str(baseline_path),
        "baseline_run_id": baseline_summary.get("run_id"),
        "baseline_schema_version": baseline_summary.get("schema_version"),
        "baseline_totals": {key: int_counter(baseline_totals.get(key)) for key in counter_names},
        "current_totals": {key: int_counter(current_totals.get(key)) for key in counter_names},
        "deltas": deltas,
        "regressions": regressions,
    }


def update_totals(totals: dict[str, int], result: dict[str, Any]) -> None:
    category = classify_result(result)
    totals["samples"] += 1
    totals["ok"] += int(bool(result.get("ok")))
    expectations_ok = bool(result.get("expectations_passed"))
    totals["expectations_passed"] += int(expectations_ok)
    if category is None:
        return
    if category != "expectation_mismatches":
        totals["errors"] += 1
    totals[category] += 1


def main() -> int:
    args = parse_args()
    samples = read_jsonl(Path(args.input))
    archive = OutputArchive(args.output, args.run_id)
    started_at = utc_now()
    url = args.base_url.rstrip("/") + "/ai/copilot/business-case-draft-diagnostics"

    totals = new_totals()
    diagnostics: dict[str, list[dict[str, Any]]] = {
        "transport_errors": [],
        "model_schema_errors": [],
        "expectation_mismatches": [],
        "input_errors": [],
    }
    with archive.results_path.open("w", encoding="utf-8") as out:
        for index, sample in enumerate(samples, 1):
            sample_id = str(sample.get("id") or f"sample-{index}")
            transcript = str(sample.get("transcript") or "").strip()
            transcript_info = transcript_metadata(
                transcript,
                max(args.transcript_preview_chars, 0),
                bool(args.include_transcripts),
            )
            if not transcript:
                result = {
                    "id": sample_id,
                    "ok": False,
                    "input_error": True,
                    "error_stage": "input",
                    "error_message": "missing transcript",
                    **transcript_info,
                }
            else:
                started = time.perf_counter()
                try:
                    status, response = post_json(
                        url,
                        args.token,
                        {
                            "entity_id": str(sample.get("entity_id") or args.entity_id),
                            "transcript": transcript,
                            "source_page": str(sample.get("source_page") or "flight_monitor"),
                            "context": sample.get("context") if isinstance(sample.get("context"), dict) else {},
                        },
                        args.timeout,
                    )
                except (urllib.error.URLError, OSError, TimeoutError) as exc:
                    result = {
                        "id": sample_id,
                        "ok": False,
                        "transport_error": True,
                        "error_stage": "transport",
                        "error_message": str(exc),
                        **transcript_info,
                    }
                    update_totals(totals, result)
                    category = classify_result(result)
                    if category is not None:
                        diagnostics[category].append(diagnostic_entry(result))
                    archive.write_failure(result, index)
                    out.write(json.dumps(result, ensure_ascii=False, sort_keys=True) + "\n")
                    continue

                elapsed_ms = round((time.perf_counter() - started) * 1000, 2)
                data = response.get("data") if isinstance(response.get("data"), dict) else response
                result = {
                    "id": sample_id,
                    "http_status": status,
                    "elapsed_ms": elapsed_ms,
                    "ok": bool(data.get("ok")) if isinstance(data, dict) else False,
                    "error_stage": data.get("error_stage") if isinstance(data, dict) else "response",
                    "error_message": data.get("error_message") if isinstance(data, dict) else "invalid response",
                    "candidate_case_types": data.get("candidate_case_types", []) if isinstance(data, dict) else [],
                    **transcript_info,
                }
                if isinstance(data, dict):
                    result.update(score_result(data, sample.get("expected") if isinstance(sample.get("expected"), dict) else {}))

            update_totals(totals, result)
            category = classify_result(result)
            if category is not None:
                diagnostics[category].append(diagnostic_entry(result))
            archive.write_failure(result, index)
            out.write(json.dumps(result, ensure_ascii=False, sort_keys=True) + "\n")

    finished_at = utc_now()
    baseline_compare = build_baseline_compare(totals, Path(args.baseline_summary)) if args.baseline_summary else None
    summary: dict[str, Any] = {
        "schema_version": SUMMARY_SCHEMA_VERSION,
        "run_id": archive.run_id,
        "started_at": format_timestamp(started_at),
        "finished_at": format_timestamp(finished_at),
        "input": str(Path(args.input)),
        "base_url": args.base_url,
        "results_path": str(archive.results_path),
        "failures_path": str(archive.failures_path) if archive.failures_path else None,
        "totals": totals,
        "diagnostics": diagnostics,
        "privacy": {
            "include_transcripts": bool(args.include_transcripts),
            "transcript_preview_chars": max(args.transcript_preview_chars, 0),
        },
    }
    if baseline_compare is not None:
        summary["baseline_compare"] = baseline_compare
    archive.write_summary(summary)

    stdout_payload: dict[str, Any] = summary if archive.archive_mode else totals
    if baseline_compare is not None and not archive.archive_mode:
        stdout_payload = {"totals": totals, "baseline_compare": baseline_compare}
    print(json.dumps(stdout_payload, ensure_ascii=False, sort_keys=True))

    print("\n================ EVALUATION SUMMARY ================")
    print(f"  Run ID:               {archive.run_id}")
    print(f"  Samples:              {totals['samples']}")
    print(f"  Passed:               {totals['ok']}")
    print(f"  Failed:               {totals['errors']}")
    print(f"  Transport errors:     {totals['transport_errors']}")
    print(f"  Model/schema errors:  {totals['model_schema_errors']}")
    print(f"  Input errors:         {totals['input_errors']}")
    print(f"  Expectation mismatches: {totals['expectation_mismatches']}")
    if baseline_compare is not None:
        print(f"  Baseline summary:      {baseline_compare['baseline_summary_path']}")
        if baseline_compare["regressions"]:
            print("  Regression deltas:")
            for key, delta in baseline_compare["regressions"].items():
                print(f"    {key}: +{delta}")
        else:
            print("  Regression deltas:    none")
    print(f"  Results:              {archive.results_path}")
    if archive.summary_path:
        print(f"  Summary:              {archive.summary_path}")
        print(f"  Failures:             {archive.failures_path}")
    print("====================================================")

    if totals["transport_errors"] > 0:
        return 1
    if totals["input_errors"] > 0:
        return 1
    if totals["model_schema_errors"] > 0:
        return 1
    if args.fail_on_mismatch and totals["expectation_mismatches"] > 0:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
