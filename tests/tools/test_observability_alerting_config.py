import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PROMETHEUS_CONFIG = ROOT / "deploy/docker/prometheus.yml"
PROMETHEUS_RULES = ROOT / "deploy/docker/prometheus-rules/fms-slo-alerts.yml"
OBSERVABILITY_COMPOSE = ROOT / "deploy/docker/docker-compose.observability.yml"
ENV_EXAMPLE = ROOT / ".env.example"
GRAFANA_DASHBOARDS_DIR = ROOT / "deploy/docker/grafana/dashboards"
AI_AGENT_DASHBOARD = GRAFANA_DASHBOARDS_DIR / "fms-ai-agent.json"


def test_prometheus_loads_the_versioned_slo_rule_file():
    config = PROMETHEUS_CONFIG.read_text(encoding="utf-8")
    compose = OBSERVABILITY_COMPOSE.read_text(encoding="utf-8")

    assert "rule_files:" in config
    assert "/etc/prometheus/rules/fms-slo-alerts.yml" in config
    assert "./prometheus-rules:/etc/prometheus/rules:ro" in compose


def test_slo_rules_cover_the_wave3_operational_alerts():
    assert PROMETHEUS_RULES.is_file(), "versioned Prometheus SLO rules are missing"
    rules = PROMETHEUS_RULES.read_text(encoding="utf-8")

    for alert_name in (
        "FmsApiAvailabilityLow",
        "FmsWriteLatencyHigh",
        "FmsOutboxBacklogHigh",
    ):
        assert f"alert: {alert_name}" in rules
        assert "runbook_url:" in rules


def test_slo_rules_cover_the_agent_alerts_task_j2():
    assert PROMETHEUS_RULES.is_file(), "versioned Prometheus SLO rules are missing"
    rules = PROMETHEUS_RULES.read_text(encoding="utf-8")

    for alert_name in (
        "FmsAiUngroundedSpike",
        "FmsAiSidecarDown",
        "FmsAiBudgetExhausted",
        "FmsAiFirstProgressSlow",
    ):
        assert f"alert: {alert_name}" in rules, f"missing agent alert {alert_name}"
    # Every alert must point at a runbook anchor in ALERT_RESPONSE.md.
    assert rules.count("runbook_url: docs/observability/ALERT_RESPONSE.md#") >= 7
    runbook = (ROOT / "docs/observability/ALERT_RESPONSE.md").read_text(encoding="utf-8")
    for anchor in (
        "## FmsAiUngroundedSpike",
        "## FmsAiSidecarDown",
        "## FmsAiBudgetExhausted",
        "## FmsAiFirstProgressSlow",
    ):
        assert anchor in runbook, f"runbook missing section {anchor}"


def test_agent_dashboard_is_provisioned_and_valid():
    # The observability compose mounts the whole dashboards directory, so a
    # valid JSON file inside it is auto-provisioned by the fms.yaml provider.
    compose = OBSERVABILITY_COMPOSE.read_text(encoding="utf-8")
    assert "./grafana/dashboards:/var/lib/grafana/dashboards:ro" in compose

    assert AI_AGENT_DASHBOARD.is_file(), "fms-ai-agent dashboard is missing"
    dashboard = json.loads(AI_AGENT_DASHBOARD.read_text(encoding="utf-8"))
    assert dashboard["uid"] == "fms-ai-agent"
    assert dashboard["panels"], "agent dashboard has no panels"

    panel_exprs = " ".join(
        target.get("expr", "")
        for panel in dashboard["panels"]
        for target in panel.get("targets", [])
    )
    # J4 exit criteria: sliceable failure rate and tokens per task type,
    # plus the cost counter surfaced for spend tracking.
    assert "task_type" in panel_exprs
    assert "fms_ai_llm_calls_total" in panel_exprs
    assert "fms_ai_tokens_total" in panel_exprs
    assert "fms_ai_run_cost_usd" in panel_exprs
    assert "fms_ai_run_stops_total" in panel_exprs
    assert "fms_ai_first_progress_seconds_bucket" in panel_exprs


def test_grafana_image_is_pinned_stable_semver_not_latest():
    compose = OBSERVABILITY_COMPOSE.read_text(encoding="utf-8")
    match = re.search(r"image:\s*grafana/grafana:(\S+)", compose)
    assert match, "grafana image pin missing from observability compose"
    tag = match.group(1)
    assert tag != "latest", "grafana must not use :latest"
    assert re.fullmatch(r"\d+\.\d+\.\d+", tag), (
        f"grafana image tag must be stable x.y.z semver, got {tag!r}"
    )
    # Prefer current LTS/stable pin used by the stack (not abandoned 11.x).
    major = int(tag.split(".", 1)[0])
    assert major >= 12, f"grafana major should be >=12, got {tag!r}"


def test_grafana_admin_password_requires_explicit_env_no_weak_default():
    compose = OBSERVABILITY_COMPOSE.read_text(encoding="utf-8")
    assert (
        "${GF_SECURITY_ADMIN_PASSWORD:?GF_SECURITY_ADMIN_PASSWORD is required}"
        in compose
    )
    assert "GF_SECURITY_ADMIN_PASSWORD:-admin" not in compose
    assert re.search(
        r"GF_SECURITY_ADMIN_PASSWORD:\s*\$\{GF_SECURITY_ADMIN_PASSWORD:-[^}]+\}",
        compose,
    ) is None

    env_example = ENV_EXAMPLE.read_text(encoding="utf-8")
    assert "GF_SECURITY_ADMIN_PASSWORD" in env_example
    assert re.search(
        r"^GF_SECURITY_ADMIN_PASSWORD\s*=\s*(?!__REPLACE_).+",
        env_example,
        re.MULTILINE,
    ) is None
