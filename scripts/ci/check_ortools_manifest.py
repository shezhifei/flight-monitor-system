#!/usr/bin/env python3
"""Guardrail for D-35: the OR-Tools wasm release chain must not drift.

Two manifests describe the same artifact:

* ``tools/ortools_wasm/upstream-manifest.json``
      The pinned build input. ``artifact_version`` here is the version the
      next ``build_release.sh`` run will produce and tag.
* ``frontend/vendor/ortools/active-manifest.json``
      The only manifest tracked by git in the vendor directory, and the first
      file the browser fetches
      (``frontend/vue-app/src/workers/dispatchReplanWorker.ts`` reads it, then
      follows ``runtime_manifest_path``).  It records what was actually
      published to a GitHub Release.

The local ``runtime-manifest.json`` is git-ignored and written by the install
script, so a fresh clone only ever sees ``active-manifest.json``.

When the two ``artifact_version`` values disagree, a release was built locally
but never published -- the exact failure that already happened once
(``v9.14-bridge.4`` built, ``v9.14-bridge.2`` still active).  No CI caught it,
which is why this assertion exists.

Exit codes: 0 = consistent, 1 = drift (or self-inconsistent manifest),
2 = manifest missing / unreadable.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

UPSTREAM_MANIFEST = REPO_ROOT / "tools/ortools_wasm/upstream-manifest.json"
ACTIVE_MANIFEST = REPO_ROOT / "frontend/vendor/ortools/active-manifest.json"

# Fields that must agree between the pinned build input and the published
# artifact the app actually loads.
SHARED_FIELDS = ("solver_version", "artifact_version")


def _load(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        print(f"ERROR: expected manifest not found: {path}", file=sys.stderr)
        raise SystemExit(2)
    except json.JSONDecodeError as exc:
        print(f"ERROR: {path} is not valid JSON: {exc}", file=sys.stderr)
        raise SystemExit(2)
    if not isinstance(payload, dict):
        print(f"ERROR: {path} must contain a JSON object", file=sys.stderr)
        raise SystemExit(2)
    return payload


def _rel(path: Path) -> str:
    return path.relative_to(REPO_ROOT).as_posix()


def main() -> int:
    upstream = _load(UPSTREAM_MANIFEST)
    active = _load(ACTIVE_MANIFEST)

    failures: list[str] = []

    for field in SHARED_FIELDS:
        upstream_value = upstream.get(field)
        active_value = active.get(field)
        if upstream_value is None or active_value is None:
            failures.append(
                f"{field}: missing -- "
                f"{_rel(UPSTREAM_MANIFEST)}={upstream_value!r}, "
                f"{_rel(ACTIVE_MANIFEST)}={active_value!r}"
            )
        elif upstream_value != active_value:
            failures.append(f"{field}: {_rel(ACTIVE_MANIFEST)}={active_value!r} "
                            f"!= {_rel(UPSTREAM_MANIFEST)}={upstream_value!r}")

    # The active manifest must be internally consistent: its release tag and
    # every download URL must point at its own artifact_version, otherwise the
    # app fetches assets from a release that is not the one it declares.
    artifact_version = active.get("artifact_version")
    if isinstance(artifact_version, str) and artifact_version:
        release_tag = (active.get("release") or {}).get("tag")
        if isinstance(release_tag, str) and artifact_version not in release_tag:
            failures.append(
                "release.tag: "
                f"{_rel(ACTIVE_MANIFEST)} declares artifact_version="
                f"{artifact_version!r} but release.tag={release_tag!r} does not "
                "contain it"
            )
        assets = active.get("assets") or {}
        if isinstance(assets, dict):
            for name, entry in assets.items():
                url = (entry or {}).get("url") if isinstance(entry, dict) else None
                if isinstance(url, str) and artifact_version not in url:
                    failures.append(
                        f"assets.{name}.url: does not contain "
                        f"artifact_version {artifact_version!r}: {url}"
                    )

    print(
        "ortools-manifest-guard: "
        f"{_rel(UPSTREAM_MANIFEST)}={upstream.get('artifact_version')!r} vs "
        f"{_rel(ACTIVE_MANIFEST)}={active.get('artifact_version')!r}"
    )

    if not failures:
        print("OR-Tools manifest consistency passed.")
        return 0

    print("\nOR-Tools wasm release chain drift detected:", file=sys.stderr)
    for failure in failures:
        print(f"  - {failure}", file=sys.stderr)
    print(
        "\nThis is a real release gap, not a formatting problem: the pinned\n"
        "build input and the manifest the browser actually load disagree, so\n"
        "a fresh clone ships a different solver than the one that was built\n"
        "and fixture-tested.\n"
        "\nResolve it one of two ways -- do NOT edit a manifest just to make\n"
        "this check green:\n"
        "\n"
        "  A. Publish the pinned version (normal case, upstream > active):\n"
        "       tools/ortools_wasm/build_release.sh \\\n"
        "         --output-dir dist/ortools \\\n"
        "         --artifact-version <upstream artifact_version>\n"
        "       # fixture tests run inside build_release.sh; then upload the\n"
        "       # per-file assets plus the bundle to the immutable tag\n"
        "       gh release create <release_tag> dist/ortools/<version>/* \\\n"
        "         --title <release_tag>\n"
        "       # and update frontend/vendor/ortools/active-manifest.json to\n"
        "       # that tag with the published SHA256 digests, per\n"
        "       # tools/ortools_wasm/README.md ('Release flow' steps 3-4).\n"
        "\n"
        "  B. Withdraw the pin (if <upstream artifact_version> was never\n"
        "     meant to ship): revert tools/ortools_wasm/upstream-manifest.json\n"
        "     to the published version and rebuild from that tag.\n",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
