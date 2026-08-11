from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

ARCHITECTURE_DOCS = [
    "docs/architecture/DEPENDENCY_DIRECTION.md",
    "docs/architecture/TECH_DEBT_DASHBOARD.md",
    "docs/architecture/ARCHITECTURE_IMPROVEMENT_ROADMAP.md",
    "docs/plans/2026-06-29-tech-debt-sweep-master-plan.md",
]

REMOVED_PYTHON_AUDIT_ASSETS = [
    "audit_layer_dependencies.py",
    "layer_dependency_baseline.json",
]

RUST_GUARDRAILS = [
    "services/api-server/crates/api/tests/layer_boundary_guard.rs",
    "services/api-server/crates/application/tests/application_boundary_inventory.rs",
]


def test_architecture_docs_do_not_reference_removed_python_audit_assets():
    for doc in ARCHITECTURE_DOCS:
        text = (ROOT / doc).read_text(encoding="utf-8")

        for removed_asset in REMOVED_PYTHON_AUDIT_ASSETS:
            assert removed_asset not in text, f"{doc} still references removed audit asset {removed_asset}"


def test_architecture_docs_reference_existing_rust_guardrails():
    docs_by_path = {
        doc: (ROOT / doc).read_text(encoding="utf-8")
        for doc in ARCHITECTURE_DOCS
    }

    for guardrail in RUST_GUARDRAILS:
        assert (ROOT / guardrail).is_file(), f"documented Rust guardrail does not exist: {guardrail}"

    for doc, text in docs_by_path.items():
        for guardrail in RUST_GUARDRAILS:
            assert guardrail in text, f"{doc} should reference Rust guardrail {guardrail}"


def test_dashboard_and_master_plan_describe_api_dependency_guardrail():
    docs = [
        "docs/architecture/TECH_DEBT_DASHBOARD.md",
        "docs/plans/2026-06-29-tech-debt-sweep-master-plan.md",
    ]
    required_terms = [
        "fms-infrastructure",
        "sqlx",
        "redis",
        "layer_boundary_guard.rs",
    ]

    for doc in docs:
        text = (ROOT / doc).read_text(encoding="utf-8")
        for term in required_terms:
            assert term in text, f"{doc} should describe API dependency guardrail term {term}"


def test_route_snapshot_matches_root_redirect_location():
    web_rs = (ROOT / "services/api-server/crates/server/src/web.rs").read_text(encoding="utf-8")
    snapshot = (ROOT / "docs/API_ROUTE_SNAPSHOT.md").read_text(encoding="utf-8")
    root_row = next(
        (line for line in snapshot.splitlines() if line.startswith("| `/` |")),
        "",
    )

    if '("Location", "/frontend/login.html")' in web_rs:
        assert "/frontend/login.html" in root_row
    elif '("Location", "/frontend/html/login.html")' in web_rs:
        assert "/frontend/html/login.html" in root_row
    else:
        raise AssertionError("server root redirect Location header was not found in web.rs")
