#!/usr/bin/env python3
"""Guardrail for D-27: GitHub workflow files must be parseable YAML.

Why this exists
---------------
``ci.yml`` and ``nightly.yml`` were invalid YAML for months (commit
``e62c8d8``).  GitHub silently skipped the whole workflow, so every gate they
claim to run (clippy, fmt, cargo audit, cargo deny, cargo test, the layer
boundary guards, Playwright E2E, nightly mutation/chaos/perf) never executed.

Root cause of that incident: a ``run: |`` block scalar whose heredoc body was
written at column 0 (``TRUSTED_PROXY_CIDRS=127.0.0.1/32``).  A column-0 line
closes the block scalar early and leaves a bare ``NAME=value`` scalar at the
document root -- a scanner error, not a lint warning.

Two checks, therefore:

1. ``yaml.safe_load`` must succeed and yield a mapping for every
   ``.github/workflows/*.yml`` / ``*.yaml``.
2. No non-blank line at column 0 may look like a shell assignment
   (``NAME=value``).  That shape is never legal at the root of a workflow
   document, and it is the exact class of bug that broke the pipeline
   silently.  Checked both generically and specifically for lines that were
   sitting inside an open ``run: |`` / ``run: >`` block scalar.

Intentionally dependency-light: only ``PyYAML`` beyond the standard library.
Run with ``python scripts/ci/check_workflow_yaml.py`` from anywhere.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

try:
    import yaml
except ModuleNotFoundError:  # pragma: no cover - CI installs this explicitly
    print(
        "ERROR: PyYAML is required for the workflow YAML guard.\n"
        "       Install it with: python -m pip install pyyaml",
        file=sys.stderr,
    )
    raise SystemExit(2)

REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS_GLOB = ".github/workflows/*.yml"
WORKFLOWS_GLOB_YAML = ".github/workflows/*.yaml"

# A bare shell assignment anchored at column 0, e.g. `FOO=bar`, `FOO=a/b:32`,
# `FOO=`. Deliberately strict about the name (shell identifier) and tolerant
# about the value.
COLUMN_ZERO_ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*[?+]?=")

# `run: |`, `run: |-`, `run: >+`, `run: | # comment`, and the same for other
# multi-line string keys used inside actions.
BLOCK_SCALAR_KEY = re.compile(
    r"^(?P<indent>[ \t]*)(?:-\s+)?(?P<key>run|script|contents|command):"
    r"[ \t]*(?:[|>][+-]?(?:\d+)?)?[ \t]*(?:#.*)?$"
)


def workflow_files() -> list[Path]:
    found = sorted(REPO_ROOT.glob(WORKFLOWS_GLOB)) + sorted(
        REPO_ROOT.glob(WORKFLOWS_GLOB_YAML)
    )
    return found


def _indent_of(line: str) -> int:
    return len(line) - len(line.lstrip(" \t"))


def scan_column_zero_assignments(path: Path) -> list[tuple[int, str, bool]]:
    """Return (line_number, text, was_inside_block_scalar) violations."""
    violations: list[tuple[int, str, bool]] = []
    # Stack of open block scalars: (key_indent,). A line belongs to the block
    # while its indent is greater than the key indent, or it is blank.
    open_blocks: list[int] = []

    for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue

        indent = _indent_of(line)

        # A block scalar stays open only while lines are indented deeper than
        # its key. The first line that dedents closes it -- and that closing
        # line is what we want to inspect below.
        was_inside_block = bool(open_blocks) and indent <= open_blocks[-1]
        while open_blocks and indent <= open_blocks[-1]:
            open_blocks.pop()

        if indent == 0 and COLUMN_ZERO_ASSIGNMENT.match(line):
            violations.append((lineno, line.rstrip(), was_inside_block))
            continue

        match = BLOCK_SCALAR_KEY.match(line)
        if match:
            key_indent = len(match.group("indent"))
            # A block scalar must be indented deeper than its key. Track the
            # deepest open block only; sibling keys reset the stack.
            open_blocks = [indent for indent in open_blocks if indent < key_indent]
            open_blocks.append(key_indent)

    return violations


def check_parse(path: Path) -> str | None:
    """Return an error message when the file is not a valid workflow mapping."""
    text = path.read_text(encoding="utf-8")
    try:
        documents = list(yaml.safe_load_all(text))
    except yaml.YAMLError as exc:
        return f"{path.relative_to(REPO_ROOT).as_posix()}: YAML parse failed -> {exc}"
    if not documents:
        return f"{path.relative_to(REPO_ROOT).as_posix()}: file is empty"
    for index, document in enumerate(documents):
        if document is None:
            continue
        if not isinstance(document, dict):
            return (
                f"{path.relative_to(REPO_ROOT).as_posix()} (document {index}): "
                f"top level is {type(document).__name__}, expected a mapping. "
                "A stray column-0 line inside a `run: |` block scalar truncates "
                "the block and leaves a bare scalar at the document root."
            )
    return None


def main() -> int:
    files = workflow_files()
    if not files:
        print(
            "ERROR: no workflow files found under .github/workflows/ - the "
            "guard would silently pass on a typo'd glob.",
            file=sys.stderr,
        )
        return 1

    failures: list[str] = []
    for path in files:
        parse_error = check_parse(path)
        if parse_error:
            failures.append(parse_error)

        for lineno, text, inside_block in scan_column_zero_assignments(path):
            context = (
                "inside a `run: |` block scalar (this is exactly the D-27 bug "
                "shape: the column-0 line truncates the block scalar)"
                if inside_block
                else "at document root"
            )
            failures.append(
                f"{path.relative_to(REPO_ROOT).as_posix()}:{lineno}: "
                f"assignment-looking line at column 0 {context}: {text!r}. "
                "Indent it to match the surrounding `run: |` block content, "
                "or move it into the step's `env:` mapping."
            )

    print(f"workflow-yaml-guard: checked {len(files)} file(s)")
    for path in files:
        status = "FAIL" if any(
            f.startswith(path.relative_to(REPO_ROOT).as_posix()) for f in failures
        ) else "ok"
        print(f"  [{status}] {path.relative_to(REPO_ROOT).as_posix()}")

    if failures:
        print("\nWorkflow YAML guard violations:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        print(
            "\nGitHub does not run an unparsable workflow and does not surface a "
            "check for it.\nThis gate exists so that can never be silent again "
            "(see docs/architecture/TECH_DEBT_INVENTORY_2026-09-02.md D-27).",
            file=sys.stderr,
        )
        return 1

    print("workflow YAML guard passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
