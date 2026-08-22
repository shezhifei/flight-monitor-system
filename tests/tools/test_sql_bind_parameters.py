"""Identifier/value filters must stay on $N binds, not string-interpolated ids."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TARGETS = [
    ROOT / "services/api-server/crates/infrastructure/src/db/query_builder.rs",
    ROOT / "services/api-server/crates/infrastructure/src/repositories/pg_business_case_repository.rs",
]


def test_query_builder_and_business_case_sql_do_not_interpolate_ids() -> None:
    forbidden = (
        "WHERE id = '{}'",
        'WHERE id = "{}"',
        "WHERE id = format!",
        'format!("SELECT * FROM business_cases WHERE id = \'{}\'"',
    )
    for path in TARGETS:
        text = path.read_text(encoding="utf-8")
        for token in forbidden:
            assert token not in text, f"{path} still interpolates identifiers: {token}"
        assert "$1" in text or "${" in text
