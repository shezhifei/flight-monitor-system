#!/usr/bin/env python
"""Export the retired Python canonical route manifest for Rust parity tests.

The Python HTTP backend was retired, so this exporter intentionally no longer
imports the old FastAPI application factory. It emits the checked-in subset of
canonical `/api/v2/*` routes that Rust still promises to serve. Retired routes
are removed from the fixture and covered by explicit 404 tests in the Rust API.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


FIXTURE_NAME = "python_canonical_route_manifest.json"


def load_manifest() -> list[dict[str, Any]]:
    fixture_path = Path(__file__).with_name(FIXTURE_NAME)
    raw = fixture_path.read_text(encoding="utf-8")
    payload = json.loads(raw)
    if not isinstance(payload, list):
        raise ValueError(f"{FIXTURE_NAME} must contain a JSON array")
    return payload


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Export the retired Python canonical route manifest as JSON.",
    )
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="Path to write the route manifest JSON file.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(load_manifest(), ensure_ascii=False, indent=2, sort_keys=True),
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
