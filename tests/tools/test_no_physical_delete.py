"""Guard against physical deletion of audited business data.

Audit requirements forbid physical deletion: product "delete" operations must
soft-delete (set deleted_at) instead. This test scans all production source
trees for DELETE FROM statements against audited tables.

Whitelist policy (spec §3.2.5, docs/plans/2026-08-12-remove-foreign-keys-spec.md):
- D-class transient runtime tables are exempt (queues, snapshots, derived
  projections — not business records). They are enumerated explicitly below;
  adding a new exemption requires editing this list, which is the review gate.
- File-level exemptions cover generic SQL builders (dynamic table names) and
  #[cfg(test)] cleanup blocks.
- legacy-backend/scripts/** (demo cleanup etc.) is outside the scan scope;
  those scripts are protected by dev-environment guards instead.
"""

import re
from pathlib import Path


REPO_ROOT = Path(__file__).parent.parent.parent

SCAN_ROOTS: tuple[tuple[Path, str], ...] = (
    (REPO_ROOT / "services" / "api-server" / "crates", "*.rs"),
    (REPO_ROOT / "services" / "ai-sidecar" / "src", "*.py"),
    (REPO_ROOT / "legacy-backend" / "src", "*.py"),
)

# Rust paths excluded from the scan (spec: tests/, benches/, test_support.rs).
RUST_EXCLUDED_PARTS = ("\\tests\\", "/tests/", "\\benches\\", "/benches/")
RUST_EXCLUDED_NAMES = {"test_support.rs"}

DELETE_PATTERN = re.compile(r"\bDELETE\s+FROM\s+([a-z_][a-z0-9_]*)", re.IGNORECASE)

# D-class transient runtime tables (spec §3.2.5) — not business records.
TRANSIENT_TABLE_WHITELIST = frozenset(
    {
        "ai_pending_actions",        # work queue (incl. full-queue purge)
        "domain_event_outbox",       # already-delivered event cleanup
        "ai_run_events",             # time-window retention cleanup
        "agent_shared_context",      # AI session transient context
        "schema_migrations",         # migration rollback bookkeeping
        "flight_sync_snapshots",     # sync snapshot replacement
        "flight_runtime_list_projection",  # derived projection view
        "operator_identity_contexts",      # per-session snapshot replacement
        "ai_action_proposals",       # smoke-test batch cleanup only
        "ai_conversations",          # transient AI conversation runtime
        # Terminal membership junctions: composition facts for Terminal.remove_*.
        # Directory rows (stands/gates/carousels) themselves are deactivated, not deleted.
        "terminal_stands",
        "terminal_gates",
        "terminal_carousels",
    }
)

# File-level exemptions: generic builders with dynamic table names, or
# #[cfg(test)] cleanup blocks that live inside src files. Paths are relative
# to the repo root, using forward slashes.
FILE_WHITELIST = frozenset(
    {
        "legacy-backend/src/infrastructure/database/query_builder.py",
        "legacy-backend/src/infrastructure/config/sources/database_source.py",
        "services/ai-sidecar/src/infrastructure/database/query_builder.py",
        "services/api-server/crates/infrastructure/src/db/query_builder.rs",
        "services/api-server/crates/infrastructure/src/repositories/pg_system_flags_repository.rs",
        # DELETE FROM statements below are inside #[cfg(test)] modules:
        "services/api-server/crates/infrastructure/src/repositories/pg_ai_ontology_repository.rs",
        "services/api-server/crates/infrastructure/src/repositories/pg_ai_object_policy_repository.rs",
        # DELETE FROM statements below are inside #[cfg(test)] modules:
        "services/api-server/crates/application/src/services/domain_action_executor/tests_terminal_equipment.rs",
    }
)

COMMENT_PREFIXES = ("#", "//", "--")


def _is_comment_line(line: str) -> bool:
    return line.strip().startswith(COMMENT_PREFIXES)


def _iter_source_files() -> list[Path]:
    files: list[Path] = []
    for root, pattern in SCAN_ROOTS:
        for path in root.rglob(pattern):
            if pattern == "*.rs" and (
                any(part in str(path) for part in RUST_EXCLUDED_PARTS)
                or path.name in RUST_EXCLUDED_NAMES
            ):
                continue
            files.append(path)
    return files


def test_no_physical_delete_of_audited_tables() -> None:
    """No DELETE FROM against audited tables in production source trees."""
    violations: list[str] = []
    for path in _iter_source_files():
        relative = path.relative_to(REPO_ROOT).as_posix()
        if relative in FILE_WHITELIST:
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        # Search over the whole text so multi-line SQL strings are caught.
        for match in DELETE_PATTERN.finditer(text):
            table = match.group(1).lower()
            if table in TRANSIENT_TABLE_WHITELIST:
                continue
            prefix = text[: match.start()]
            if _is_comment_line(prefix.splitlines()[-1] if prefix else ""):
                continue
            lineno = prefix.count("\n") + 1
            violations.append(f"{relative}:{lineno}: DELETE FROM {table}")

    assert not violations, (
        "Physical deletion of audited business data is forbidden; use "
        "soft-delete (UPDATE ... SET deleted_at = NOW()) instead. "
        "If this is a genuinely transient table, add it to "
        "TRANSIENT_TABLE_WHITELIST with justification (spec §3.2.5):\n"
        + "\n".join(violations)
    )
