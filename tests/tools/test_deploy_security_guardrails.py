"""Static security guardrails for deploy edge configs and bootstrap SQL."""

from pathlib import Path
import re


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

def test_android_token_storage_fails_closed_no_plaintext_fallback():
    """TokenStorage must never fall back to plaintext SharedPreferences."""
    path = ROOT / "legacy/android-kotlin/app/src/main/java/com/flightmonitor/mobile/session/TokenStorage.kt"
    text = path.read_text(encoding="utf-8")
    assert "EncryptedSharedPreferences" in text
    assert "SecureTokenStorageException" in text
    assert "wipeLegacyPlaintextTokens" in text
    # Fail-closed: throw on crypto failure, never open MODE_PRIVATE for active storage.
    assert "refusing plaintext fallback" in text or "refuse plaintext" in text.lower()
    assert "using private prefs" not in text
    # Must not assign getSharedPreferences(...MODE_PRIVATE) as the live store return path
    # (legacy wipe may still open it solely to clear).
    assert "throw SecureTokenStorageException" in text
    assert "PREFS_NAME_LEGACY_PLAINTEXT" in text or "mobile_auth_tokens" in text

def test_android_token_storage_wipe_checks_commit_and_fails_closed():
    path = ROOT / "legacy/android-kotlin/app/src/main/java/com/flightmonitor/mobile/session/TokenStorage.kt"
    text = path.read_text(encoding="utf-8")
    assert "wipeLegacyPlaintextTokens" in text
    assert ".commit()" in text
    assert "hadSecrets" in text or "hadData" in text or "all.isNotEmpty()" in text
    assert "SecureTokenStorageException" in text
    # Must not swallow clear failure with only Log.w and continue
    assert "Failed clearing legacy plaintext prefs content" not in text
    assert "throw SecureTokenStorageException" in text

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

def test_android_targets_api_35_or_higher():
    """Play requires ordinary apps to target Android 15 / API 35+ (2026-07)."""
    gradle = (ROOT / "legacy/android-kotlin/app/build.gradle.kts").read_text(encoding="utf-8")
    m_compile = re.search(r"compileSdk\s*=\s*(\d+)", gradle)
    m_target = re.search(r"targetSdk\s*=\s*(\d+)", gradle)
    assert m_compile and m_target, "compileSdk/targetSdk missing from app build.gradle.kts"
    assert int(m_compile.group(1)) >= 35
    assert int(m_target.group(1)) >= 35

def test_android_readme_matches_api_35_no_stale_sdk_docs():
    """Archived Kotlin app: build.gradle.kts stays truthful; README must declare the archive.

    The Kotlin client was archived to legacy/android-kotlin (bdb0832); its README
    no longer documents SDK versions, but it must still identify the app as
    archived and point at the active client so nobody ships the legacy app.
    """
    readme = (ROOT / "legacy/android-kotlin/README.md").read_text(encoding="utf-8")
    gradle = (ROOT / "legacy/android-kotlin/app/build.gradle.kts").read_text(encoding="utf-8")
    m_compile = re.search(r"compileSdk\s*=\s*(\d+)", gradle)
    m_target = re.search(r"targetSdk\s*=\s*(\d+)", gradle)
    m_min = re.search(r"minSdk\s*=\s*(\d+)", gradle)
    assert m_compile and m_target and m_min
    assert int(m_compile.group(1)) >= 35
    assert int(m_target.group(1)) >= 35
    assert int(m_min.group(1)) >= 23
    lowered = readme.lower()
    assert "archived" in lowered, "legacy android README must declare the archive"
    assert "mobile/flutter-app" in readme or "mobile/core" in readme, (
        "legacy android README must point at the active client"
    )


def test_removed_aip_module_stays_gone():
    """K1: the AIP parallel stack was deleted (W2-2); guardrails must not resurrect it."""
    aip_dir = ROOT / "services/ai-sidecar/src/infrastructure/ai/aip"
    assert not any(aip_dir.glob("*.py")), "aip module must remain deleted (no .py sources)"
    for rel in (
        "services/ai-sidecar/src/infrastructure/ai/aip/action_handlers.py",
        "services/ai-sidecar/src/infrastructure/ai/aip/app.py",
    ):
        assert not (ROOT / rel).exists(), f"{rel} must not be reintroduced"

