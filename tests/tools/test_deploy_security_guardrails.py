"""Static security guardrails for deploy edge configs and bootstrap SQL."""

from pathlib import Path
import re
import subprocess


ROOT = Path(__file__).resolve().parents[2]

NGINX_EDGE_CONFIGS = (
    ROOT / "deploy/docker/nginx/edge.conf",
    ROOT / "deploy/docker/nginx/default.conf",
    ROOT / "deploy/nginx/flight-monitor-distributed.conf.example",
)

SETUP_SQL = ROOT / "scripts/database/setup_postgresql.sql"


def test_edge_nginx_blocks_public_metrics_exact_locations():
    """Public edge must deny exact /metrics and /api/v2/metrics.

    Prometheus continues to scrape rust-api:8080/metrics on the internal network
    (see deploy/docker/prometheus.yml) — not via the public edge.
    """
    for path in NGINX_EDGE_CONFIGS:
        text = path.read_text(encoding="utf-8")
        assert "location = /metrics" in text, f"{path} missing exact /metrics block"
        assert "location = /api/v2/metrics" in text, (
            f"{path} missing exact /api/v2/metrics block"
        )

        for location in ("location = /metrics", "location = /api/v2/metrics"):
            start = text.index(location)
            # Look only at the block that follows this location directive.
            block = text[start : start + 120]
            assert "return 404" in block or "deny all" in block, (
                f"{path} {location} must deny public access"
            )


def test_prometheus_still_scrapes_internal_rust_api_metrics():
    prometheus = (ROOT / "deploy/docker/prometheus.yml").read_text(encoding="utf-8")
    assert "rust-api:8080" in prometheus
    assert "metrics_path: /metrics" in prometheus


def test_setup_postgresql_ai_query_ro_has_no_password_literal():
    setup_sql = SETUP_SQL.read_text(encoding="utf-8")
    # Broader than schema test: entire file must not reintroduce the old default.
    assert "ai_query_ro_dev_change_me" not in setup_sql
    assert "PASSWORD 'ai_query_ro" not in setup_sql

def test_archived_android_kotlin_client_stays_gone():
    """D-17: the archived Kotlin client was deleted from the repository.

    The Flutter app (mobile/flutter-app) is the only supported mobile client;
    the legacy Kotlin sources must not be reintroduced.
    """
    legacy_dir = ROOT / "legacy/android-kotlin"
    assert not legacy_dir.exists(), "legacy/android-kotlin must remain deleted"
    tracked = subprocess.run(
        ["git", "ls-files", "legacy/android-kotlin"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    assert not tracked, f"legacy/android-kotlin files reintroduced: {tracked[:200]}"

PRODUCTION_COMPOSE_WITH_RUST_API = (
    ROOT / "deploy/docker/docker-compose.distributed.yml",
    ROOT / "deploy/docker/docker-compose.edge.yml",
)


def test_production_compose_requires_explicit_trusted_proxy_cidrs():
    """rust-api must fail-fast without TRUSTED_PROXY_CIDRS (no silent empty trust)."""
    for path in PRODUCTION_COMPOSE_WITH_RUST_API:
        text = path.read_text(encoding="utf-8")
        assert "TRUSTED_PROXY_CIDRS" in text, f"{path} missing TRUSTED_PROXY_CIDRS"
        assert (
            "${TRUSTED_PROXY_CIDRS:?TRUSTED_PROXY_CIDRS is required}" in text
        ), f"{path} must fail-fast when TRUSTED_PROXY_CIDRS is unset"
        # No broad default via :- syntax
        assert re.search(
            r"TRUSTED_PROXY_CIDRS:\s*\$\{TRUSTED_PROXY_CIDRS:-",
            text,
        ) is None, f"{path} must not default TRUSTED_PROXY_CIDRS"


def test_env_example_discourages_entire_rfc1918_as_trusted_proxy():
    text = (ROOT / ".env.example").read_text(encoding="utf-8")
    assert "TRUSTED_PROXY_CIDRS" in text
    lowered = text.lower()
    assert "never trust entire rfc1918" in lowered or "do not use 10.0.0.0/8" in lowered
    assert "127.0.0.1/32" in text
    assert "DO NOT use 10.0.0.0/8" in text or "do not use 10.0.0.0/8" in lowered



def test_management_routes_do_not_echo_exceptions_on_500():
    """Client 500 boundaries must not format exception objects into responses."""
    path = ROOT / "services/ai-sidecar/src/infrastructure/ai/management_routes.py"
    text = path.read_text(encoding="utf-8")
    # Disallow return _err(f"...{exc}...", 500) style
    bad = re.findall(
        r"return _err\(f?[\"'].*\{(?:exc|e)\}.*[\"'].*500",
        text,
    )
    assert not bad, f"management_routes 500 paths still embed exceptions: {bad[:5]}"
    for m in re.finditer(r"return _err\(str\((?:exc|e)\)", text):
        # str(exc) only acceptable if not 500 — check nearby
        start = max(0, m.start() - 80)
        chunk = text[start : m.end() + 40]
        assert "500" not in chunk, f"str(exc) near 500: {chunk!r}"

def test_removed_aip_module_stays_gone():
    """K1: the AIP parallel stack was deleted (W2-2); guardrails must not resurrect it."""
    aip_dir = ROOT / "services/ai-sidecar/src/infrastructure/ai/aip"
    assert not any(aip_dir.glob("*.py")), "aip module must remain deleted (no .py sources)"
    for rel in (
        "services/ai-sidecar/src/infrastructure/ai/aip/action_handlers.py",
        "services/ai-sidecar/src/infrastructure/ai/aip/app.py",
    ):
        assert not (ROOT / rel).exists(), f"{rel} must not be reintroduced"

