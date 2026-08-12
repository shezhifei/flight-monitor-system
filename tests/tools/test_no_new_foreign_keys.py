"""Guard against reintroducing database foreign keys.

Migration 120 removed ALL foreign key constraints; referential integrity is
now enforced at the application layer (see
docs/plans/2026-08-12-remove-foreign-keys-spec.md). This test fails if any
migration added after 120 defines new FOREIGN KEY / REFERENCES constraints.
"""

import re
from pathlib import Path


REPO_ROOT = Path(__file__).parent.parent.parent
MIGRATIONS_DIR = REPO_ROOT / "migrations"
FK_REMOVAL_MIGRATION = MIGRATIONS_DIR / "120_drop_all_foreign_keys.sql"

FK_PATTERN = re.compile(r"\b(FOREIGN\s+KEY|REFERENCES)\b", re.IGNORECASE)


def _migration_number(path: Path) -> int | None:
    match = re.match(r"^(\d+)_", path.name)
    return int(match.group(1)) if match else None


def _is_comment_line(line: str) -> bool:
    return line.strip().startswith("--")


def test_fk_removal_migration_exists_and_is_dynamic() -> None:
    """120_drop_all_foreign_keys.sql must exist and drop FKs dynamically."""
    assert FK_REMOVAL_MIGRATION.exists(), (
        "migrations/120_drop_all_foreign_keys.sql is missing; "
        "foreign key removal is part of the soft-delete migration stack"
    )
    content = FK_REMOVAL_MIGRATION.read_text(encoding="utf-8")
    assert "contype = 'f'" in content, (
        "migration 120 must dynamically enumerate pg_constraint "
        "(contype = 'f') instead of hardcoding constraint names"
    )


def test_no_new_foreign_keys_after_migration_120() -> None:
    """Migrations numbered > 120 must not define FOREIGN KEY / REFERENCES."""
    violations: list[str] = []
    for path in sorted(MIGRATIONS_DIR.glob("*.sql")):
        number = _migration_number(path)
        if number is None or number <= 120:
            continue
        for lineno, line in enumerate(
            path.read_text(encoding="utf-8").splitlines(), start=1
        ):
            if _is_comment_line(line):
                continue
            if FK_PATTERN.search(line):
                violations.append(f"{path.name}:{lineno}: {line.strip()}")

    assert not violations, (
        "New foreign key constraints are forbidden after migration 120 "
        "(referential integrity is application-layer enforced; see "
        "docs/plans/2026-08-12-remove-foreign-keys-spec.md):\n"
        + "\n".join(violations)
    )
