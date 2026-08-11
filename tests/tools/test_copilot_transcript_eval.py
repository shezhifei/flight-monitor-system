from __future__ import annotations

import json
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


PROJECT_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = PROJECT_ROOT / "scripts" / "tools" / "copilot_transcript_eval.py"


class _DiagnosticHandler(BaseHTTPRequestHandler):
    def do_POST(self) -> None:
        content_length = int(self.headers.get("Content-Length", "0"))
        payload = json.loads(self.rfile.read(content_length).decode("utf-8"))
        transcript = str(payload.get("transcript") or "")

        if self.path != "/api/v2/ai/copilot/business-case-draft-diagnostics":
            self.send_error(404)
            return

        action_count = 1
        if "two-actions" in transcript:
            action_count = 2
        actions = [
            {
                "case_type": "gate_baggage_check",
                "flight_number_raw": f"77{index}",
                "fields": {"seat_no": f"{index}A"},
            }
            for index in range(1, action_count + 1)
        ]
        body = {
            "data": {
                "ok": True,
                "candidate_case_types": ["gate_baggage_check"],
                "parsed_payload": {"actions": actions},
            }
        }
        encoded = json.dumps(body).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, format: str, *args: Any) -> None:
        return


class _TestServer:
    def __enter__(self) -> str:
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), _DiagnosticHandler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        host, port = self.server.server_address
        return f"http://{host}:{port}/api/v2"

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        self.server.shutdown()
        self.thread.join(timeout=5)
        self.server.server_close()


def _write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.write_text(
        "".join(json.dumps(row, ensure_ascii=False) + "\n" for row in rows),
        encoding="utf-8",
    )


def _run_eval(tmp_path: Path, rows: list[dict[str, Any]], *extra_args: str) -> subprocess.CompletedProcess[str]:
    input_path = tmp_path / "samples.jsonl"
    _write_jsonl(input_path, rows)
    with _TestServer() as base_url:
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT_PATH),
                "--base-url",
                base_url,
                "--input",
                str(input_path),
                *extra_args,
            ],
            cwd=PROJECT_ROOT,
            text=True,
            capture_output=True,
            check=False,
        )


def _read_jsonl(path: Path) -> list[dict[str, Any]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def test_archive_mode_writes_summary_results_failures_and_keeps_transcripts_private_by_default(tmp_path: Path) -> None:
    output_root = tmp_path / "archive"
    rows = [
        {
            "id": "pass-001",
            "transcript": "two-actions transcript",
            "expected": {"action_count": 2, "case_types": ["gate_baggage_check"]},
        },
        {
            "id": "mismatch-001",
            "transcript": "one-action transcript",
            "expected": {"action_count": 2, "case_types": ["gate_baggage_check"]},
        },
    ]

    completed = _run_eval(
        tmp_path,
        rows,
        "--output",
        str(output_root),
        "--run-id",
        "privacy-default",
        "--transcript-preview-chars",
        "8",
    )

    assert completed.returncode == 0, completed.stderr + completed.stdout
    run_path = output_root / "privacy-default"
    summary_path = run_path / "summary.json"
    results_path = run_path / "results.jsonl"
    failures_path = run_path / "failures"
    assert summary_path.is_file()
    assert results_path.is_file()
    assert failures_path.is_dir()

    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    assert summary["totals"]["samples"] == 2
    assert summary["totals"]["expectation_mismatches"] == 1
    assert summary["totals"]["errors"] == 0
    assert summary["privacy"] == {"include_transcripts": False, "transcript_preview_chars": 8}
    mismatch_diagnostics = summary["diagnostics"]["expectation_mismatches"]
    assert mismatch_diagnostics[0]["id"] == "mismatch-001"
    assert mismatch_diagnostics[0]["action_count_mismatch"] == {"expected": 2, "actual": 1}

    results = _read_jsonl(results_path)
    assert all("transcript" not in result for result in results)
    assert all("transcript_sha256" in result for result in results)
    assert all("transcript_chars" in result for result in results)
    assert results[0]["transcript_preview"] == "two-acti"

    failure_files = list(failures_path.glob("*.json"))
    assert len(failure_files) == 1
    assert failure_files[0].name.endswith("-mismatch-001-expectation_mismatches.json")
    failure = json.loads(failure_files[0].read_text(encoding="utf-8"))
    assert "transcript" not in failure
    assert failure["action_count_mismatch"] == {"expected": 2, "actual": 1}


def test_include_transcripts_explicitly_writes_full_transcript(tmp_path: Path) -> None:
    output_root = tmp_path / "archive"
    transcript = "full transcript only for local debugging"
    rows = [
        {
            "id": "include-001",
            "transcript": transcript,
            "expected": {"action_count": 1, "case_types": ["gate_baggage_check"]},
        }
    ]

    completed = _run_eval(
        tmp_path,
        rows,
        "--output",
        str(output_root),
        "--run-id",
        "include-transcripts",
        "--include-transcripts",
    )

    assert completed.returncode == 0, completed.stderr + completed.stdout
    result = _read_jsonl(output_root / "include-transcripts" / "results.jsonl")[0]
    assert result["transcript"] == transcript
    summary = json.loads((output_root / "include-transcripts" / "summary.json").read_text(encoding="utf-8"))
    assert summary["privacy"]["include_transcripts"] is True


def test_baseline_compare_reports_deltas_and_regressions(tmp_path: Path) -> None:
    output_root = tmp_path / "archive"
    baseline_path = tmp_path / "baseline-summary.json"
    baseline_path.write_text(
        json.dumps(
            {
                "schema_version": 2,
                "run_id": "baseline",
                "totals": {
                    "samples": 1,
                    "ok": 1,
                    "expectations_passed": 1,
                    "errors": 0,
                    "transport_errors": 0,
                    "model_schema_errors": 0,
                    "expectation_mismatches": 0,
                    "input_errors": 0,
                },
            }
        ),
        encoding="utf-8",
    )
    rows = [
        {
            "id": "regression-001",
            "transcript": "one-action transcript",
            "expected": {"action_count": 2, "case_types": ["gate_baggage_check"]},
        }
    ]

    completed = _run_eval(
        tmp_path,
        rows,
        "--output",
        str(output_root),
        "--run-id",
        "baseline-current",
        "--baseline-summary",
        str(baseline_path),
    )

    assert completed.returncode == 0, completed.stderr + completed.stdout
    summary = json.loads((output_root / "baseline-current" / "summary.json").read_text(encoding="utf-8"))
    compare = summary["baseline_compare"]
    assert compare["baseline_run_id"] == "baseline"
    assert compare["deltas"]["expectation_mismatches"] == 1
    assert compare["deltas"]["expectations_passed"] == -1
    assert compare["regressions"] == {"expectation_mismatches": 1}
