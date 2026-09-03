#!/usr/bin/env python3
"""Guardrail for D-03: every env key the Rust API reads must be documented.

The Rust workspace reads configuration with literal ``std::env::var("KEY")`` /
``env::var_os("KEY")`` / ``option_env!("KEY")`` calls.  ``.env.example`` is the
only inventory operators work from, and it had drifted badly: dozens of keys
were read by the binary but appeared nowhere in the sample file, including
fail-closed switches such as ``APP_ENVIRONMENT``.

This check asserts the literal key set read under ``services/api-server`` is a
subset of the keys documented in ``.env.example``.

Known blind spot (deliberate, and printed in the report):
``crates/application/src/services/system_flags_service.rs`` seeds a config
snapshot from *all* of ``env::vars()`` at runtime
(``load_env_source()`` -> ``load_env_source_from_iter(env::vars())``).  That
source is unbounded -- it reflects whatever the process happens to have in its
environment -- so there is no literal key set to extract and nothing to
subset-check.  A documented subset guarantee therefore only covers statically
readable keys.

Usage:
    python scripts/ci/check_env_documentation.py            # verify subset
    python scripts/ci/check_env_documentation.py --list     # print key set
    python scripts/ci/check_env_documentation.py --format json
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
RUST_ROOT = REPO_ROOT / "services" / "api-server"
ENV_EXAMPLE = REPO_ROOT / ".env.example"

# Unbounded dynamic reader -- consumes the whole environment, no literal keys.
DYNAMIC_ENV_CONSUMER = (
    "services/api-server/crates/application/src/services/system_flags_service.rs"
)

# Directory names never scanned (build output, vendored deps, scratch).
SKIP_DIR_NAMES = {"target", ".tmp", "node_modules", ".cargo", ".git", "dist"}

# Keys that belong to the OS / toolchain rather than to this application.
# They must not be pushed into .env.example.
NON_APPLICATION_KEYS = {
    "SystemRoot",
    "PATH",
    "HOME",
    "USERPROFILE",
    "TMPDIR",
    "TEMP",
    "TMP",
    "COMSPEC",
    "LANG",
    "LC_ALL",
    "TZ",
    "PWD",
    "OLDPWD",
    "SHELL",
    "HOSTNAME",
}

LITERAL_ENV_READ = re.compile(
    r"""
    (?:std::)?env::(?:var|var_os)\(\s*
      "(?P<key1>[A-Za-z_][A-Za-z0-9_]*)"\s*\)
  | option_env!\(\s*"(?P<key2>[A-Za-z_][A-Za-z0-9_]*)"\s*\)
    """,
    re.VERBOSE,
)

# `KEY=value` in .env.example, optionally commented out (`# KEY=value`,
# `#KEY=value`). A prose mention of a token does NOT count as documentation --
# only a key with a default (or an explicit placeholder) is actionable.
DOCUMENTED_ASSIGNMENT = re.compile(r"^\s*#?\s*(?P<key>[A-Za-z_][A-Za-z0-9_]*)\s*=")

# `const SOME_NAME: &str = "REAL_ENV_KEY";` -- a few call sites read env keys
# through such a constant instead of a literal (e.g.
# `crates/api/src/middleware/jwt.rs:36` -> `SSE_QUERY_TOKEN_AUTH_ENABLED`).
# Resolving them keeps the guard from having a hole where a key is reachable
# only through a named constant.
CONST_STRING = re.compile(
    r"\b(?:pub\s+)?const\s+(?P<name>[A-Z][A-Z0-9_]*)\s*:\s*&'?\w*\s*str\s*=\s*"
    r'"(?P<value>[A-Za-z_][A-Za-z0-9_]*)"'
)

# env::var(NAME) / env::var_os(NAME) where NAME is a bare identifier.
INDIRECT_ENV_READ = re.compile(
    r"(?:std::)?env::(?:var|var_os)\(\s*(?P<name>[A-Z][A-Z0-9_]*)\s*\)"
)


def _rust_sources() -> list[tuple[Path, str]]:
    sources: list[tuple[Path, str]] = []
    for path in sorted(RUST_ROOT.rglob("*.rs")):
        if SKIP_DIR_NAMES.intersection(path.parts):
            continue
        try:
            sources.append((path, path.read_text(encoding="utf-8")))
        except (OSError, UnicodeDecodeError):
            continue
    return sources


def scan_rust_keys() -> dict[str, list[str]]:
    """Map every statically-readable env key to its `path:line` occurrences.

    Two passes: the first collects `const NAME: &str = "ENV_KEY"` so the second
    can resolve `env::var(NAME)` down to the real key.
    """
    sources = _rust_sources()

    consts: dict[str, str] = {}
    for _, text in sources:
        for match in CONST_STRING.finditer(text):
            consts[match.group("name")] = match.group("value")

    occurrences: dict[str, list[str]] = {}

    def record(key: str, path: Path, text: str, offset: int) -> None:
        line = text.count("\n", 0, offset) + 1
        rel = path.relative_to(REPO_ROOT).as_posix()
        occurrences.setdefault(key, []).append(f"{rel}:{line}")

    for path, text in sources:
        for match in LITERAL_ENV_READ.finditer(text):
            key = match.group("key1") or match.group("key2")
            if key:
                record(key, path, text, match.start())
        for match in INDIRECT_ENV_READ.finditer(text):
            name = match.group("name")
            key = consts.get(name)
            if key:
                record(key, path, text, match.start())

    return occurrences


def scan_documented_keys() -> set[str]:
    text = ENV_EXAMPLE.read_text(encoding="utf-8")
    return {
        match.group("key")
        for match in (DOCUMENTED_ASSIGNMENT.match(line) for line in text.splitlines())
        if match
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--list",
        action="store_true",
        help="print the literal key set read by Rust and exit",
    )
    parser.add_argument(
        "--format",
        choices=("text", "json"),
        default="text",
        help="output format for --list / the violation report",
    )
    parser.add_argument(
        "--allow-dynamic",
        action="store_true",
        help="do not fail when a key is undocumented (report only)",
    )
    parser.add_argument(
        "--env-example",
        type=Path,
        default=None,
        help="check against an alternate .env.example (for exercising the guard)",
    )
    args = parser.parse_args()

    global ENV_EXAMPLE
    if args.env_example is not None:
        ENV_EXAMPLE = args.env_example.resolve()

    if not RUST_ROOT.is_dir():
        print(f"ERROR: Rust workspace not found at {RUST_ROOT}", file=sys.stderr)
        return 2
    if not ENV_EXAMPLE.is_file():
        print(f"ERROR: {ENV_EXAMPLE} not found", file=sys.stderr)
        return 2

    occurrences = scan_rust_keys()
    documented = scan_documented_keys()

    application_keys = {k for k in occurrences if k not in NON_APPLICATION_KEYS}
    undocumented = sorted(application_keys - documented)

    if args.list:
        payload = {
            key: {"sites": occurrences[key]}
            for key in sorted(application_keys)
        }
        if args.format == "json":
            print(json.dumps(payload, indent=2, sort_keys=True))
        else:
            for key in sorted(payload):
                print(f"{key}\t{', '.join(payload[key]['sites'])}")
        return 0

    summary = {
        "literal_keys_read_by_rust": len(application_keys),
        "ignored_os_keys": sorted(set(occurrences) - application_keys),
        "documented_keys_in_env_example": len(documented),
        "undocumented_keys": undocumented,
        "dynamic_env_consumer": DYNAMIC_ENV_CONSUMER,
    }

    print(
        f"env-documentation guard: {len(application_keys)} literal env keys read "
        f"under services/api-server, {len(documented)} keys documented in "
        ".env.example"
    )
    if summary["ignored_os_keys"]:
        print(
            "  (OS/toolchain variables excluded from the subset check: "
            + ", ".join(summary["ignored_os_keys"])
            + ")"
        )
    print(
        "  NOTE unbounded source: "
        f"{DYNAMIC_ENV_CONSUMER} reads the whole environment via env::vars(), "
        "so its keys cannot be enumerated statically and are not covered by "
        "this subset check."
    )

    if not undocumented:
        print("Every statically readable env key is documented in .env.example.")
        return 0

    if args.format == "json":
        print(json.dumps(summary, indent=2, sort_keys=True))

    print(
        f"\n{len(undocumented)} env key(s) are read by Rust but missing from "
        ".env.example:",
        file=sys.stderr,
    )
    for key in undocumented:
        sites = ", ".join(occurrences[key][:3])
        more = "" if len(occurrences[key]) <= 3 else f" (+{len(occurrences[key]) - 3} more)"
        print(f"  - {key}\n      read at: {sites}{more}", file=sys.stderr)

    print(
        "\nAdd each key to .env.example with a safe default or a commented\n"
        "placeholder (never a real secret), or -- if the key is an OS/toolchain\n"
        "variable rather than application config -- add it to NON_APPLICATION_KEYS\n"
        "in scripts/ci/check_env_documentation.py with a justification.\n"
        "See docs/architecture/TECH_DEBT_INVENTORY_2026-09-02.md D-03.",
        file=sys.stderr,
    )
    return 0 if args.allow_dynamic else 1


if __name__ == "__main__":
    raise SystemExit(main())
