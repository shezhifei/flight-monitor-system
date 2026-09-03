from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any, Dict


OUTPUT_FILES = (
    "dispatch_replan_solver.js",
    "dispatch_replan_solver.wasm",
    "LICENSES.tar.gz",
)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _write_json(path: Path, payload: Dict[str, Any]) -> None:
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def build_release_manifest(
    *,
    output_dir: Path,
    upstream_manifest_path: Path,
) -> Dict[str, Any]:
    upstream_manifest = json.loads(upstream_manifest_path.read_text(encoding="utf-8"))
    manifest = {
        "schema_version": 1,
        "solver_version": upstream_manifest["solver_version"],
        "artifact_version": upstream_manifest["artifact_version"],
        "release_tag": upstream_manifest["release_tag"],
        "upstream": upstream_manifest["upstream"],
        "toolchain": upstream_manifest["toolchain"],
        "files": {},
    }
    for file_name in OUTPUT_FILES:
        file_path = output_dir / file_name
        manifest["files"][file_name] = {
            "sha256": _sha256(file_path),
        }
    manifest_path = output_dir / "manifest.json"
    _write_json(manifest_path, manifest)

    checksum_lines = []
    for file_name in (
        "dispatch_replan_solver.js",
        "dispatch_replan_solver.wasm",
        "LICENSES.tar.gz",
    ):
        checksum_lines.append(f"{manifest['files'][file_name]['sha256']}  {file_name}")
    checksum_lines.append(f"{_sha256(manifest_path)}  manifest.json")
    checksums_path = output_dir / "SHA256SUMS"
    checksums_path.write_text(
        "\n".join(checksum_lines) + "\n", encoding="utf-8", newline="\n"
    )
    _write_json(manifest_path, manifest)
    return manifest


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate release manifest for OR-Tools wasm artifacts")
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--upstream-manifest", required=True)
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    manifest = build_release_manifest(
        output_dir=Path(args.output_dir).resolve(),
        upstream_manifest_path=Path(args.upstream_manifest).resolve(),
    )
    print(json.dumps(manifest, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
