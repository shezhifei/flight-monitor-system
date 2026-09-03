from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CI = ROOT / ".github/workflows/ci.yml"
NIGHTLY = ROOT / ".github/workflows/nightly.yml"


def _section(lines: list[str], start: str, stop_prefixes: tuple[str, ...]) -> str:
    start_index = next(i for i, line in enumerate(lines) if line == start)
    for index in range(start_index + 1, len(lines)):
        if any(lines[index].startswith(prefix) for prefix in stop_prefixes):
            return "\n".join(lines[start_index:index])
    return "\n".join(lines[start_index:])


def _ci_lines() -> list[str]:
    return CI.read_text(encoding="utf-8").splitlines()


def _nightly_lines() -> list[str]:
    return NIGHTLY.read_text(encoding="utf-8").splitlines()


def test_ci_targets_repository_base_branch():
    triggers = _section(_ci_lines(), "on:", ("env:", "jobs:"))

    assert triggers.count("branches: [master]") == 2
    assert "branches: [main]" not in triggers


def test_ci_runs_architecture_boundary_guards():
    rust_job = _section(
        _ci_lines(),
        "  rust-api:",
        ("  mq-gateway:", "  python-sidecar:", "  repository-meta-tests:", "  vue-frontend:"),
    )

    assert "working-directory: services/api-server" in rust_job
    assert "- name: Architecture boundary guard" in rust_job
    assert "cargo test -p fms-api --test layer_boundary_guard" in rust_job
    assert "cargo test -p fms-application --test application_boundary_inventory" in rust_job


def test_ci_runs_every_repository_meta_test():
    """D-24: collect tests/tools wholesale, never an enumerated subset."""

    meta_job = _section(
        _ci_lines(),
        "  repository-meta-tests:",
        ("  vue-frontend:", "  docker-compose-validate:"),
    )

    assert "actions/setup-python@v5" in meta_job
    assert "python-version: \"3.12\"" in meta_job
    assert "- name: Install dependencies" in meta_job
    assert "python -m pip install pytest" in meta_job
    assert "python -m pytest tests/tools -q" in meta_job

    # Drift guard: an enumerated subset is how 10 of 20 meta-tests silently
    # stopped running. Individual module paths must never reappear in ci.yml.
    ci_text = CI.read_text(encoding="utf-8")
    for path in sorted((ROOT / "tests" / "tools").glob("test_*.py")):
        assert f"tests/tools/{path.name}" not in ci_text, (
            f"ci.yml enumerates {path.name} again; the job must collect "
            "tests/tools as a directory"
        )


def test_ci_blocks_before_docker_on_workflow_yaml_and_config_drift():
    """D-27 / D-03 / D-35 / D-09 guards must exist and stay Docker-free."""

    lines = _ci_lines()
    workflow_lint = _section(lines, "  workflow-lint:", ("  env-documentation:",))
    assert "python scripts/ci/check_workflow_yaml.py" in workflow_lint
    assert "docker" not in workflow_lint.lower()

    env_docs = _section(lines, "  env-documentation:", ("  ortools-release-consistency:",))
    assert "python scripts/ci/check_env_documentation.py" in env_docs

    ortools = _section(lines, "  ortools-release-consistency:", ("  migrations-clean-install:",))
    assert "python scripts/ci/check_ortools_manifest.py" in ortools

    migrations = _section(lines, "  migrations-clean-install:", ("  rust-api:",))
    assert "sqlx migrate run" in migrations
    assert "image: postgres:16" in migrations


def test_cargo_deny_uses_the_root_policy_explicitly():
    """D-10: deny.toml exists only at the repo root, and the deny step runs
    with working-directory services/api-server."""

    rust_job = _section(_ci_lines(), "  rust-api:", ("  mq-gateway:",))
    assert "cargo deny --config ../../deny.toml check" in rust_job
    assert "run: cargo deny check" not in rust_job


def test_ci_has_no_soft_failed_jobs():
    """D-26: an ungated `continue-on-error` at job level makes the whole
    pipeline decorative -- failures report green. Nightly best-effort jobs are
    allowed to soft-fail; the merge-blocking CI is not."""

    ci_text = CI.read_text(encoding="utf-8")
    offenders = [
        line
        for line in ci_text.splitlines()
        if line.strip().startswith("continue-on-error")
    ]
    assert not offenders, f"ci.yml must not soft-fail any job: {offenders}"


def test_compose_validation_supplies_required_interpolation_environment():
    compose_job = _section(_ci_lines(), "  docker-compose-validate:", ("  integration-test:",))

    assert "FMS_RUNTIME_ENV_FILE: /dev/null" in compose_job
    assert "VAULT_RENDERED_ENV_FILE: /dev/null" in compose_job
    assert (
        "DB_REPLICATION_PASSWORD: ci_explicit_replication_password_not_for_prod"
        in compose_job
    )


def test_integration_compose_binds_loopback_and_requires_passwords():
    integration = (ROOT / "deploy/docker/docker-compose.integration.yml").read_text(
        encoding="utf-8"
    )

    assert "POSTGRES_PASSWORD: ${DB_PASSWORD:?DB_PASSWORD is required}" in integration
    assert "REDIS_PASSWORD: ${REDIS_PASSWORD:?REDIS_PASSWORD is required}" in integration
    assert "${DB_PASSWORD:-" not in integration
    assert "${REDIS_PASSWORD:-" not in integration

    for mapping in (
        "127.0.0.1:5432:5432",
        "127.0.0.1:6379:6379",
        "127.0.0.1:9876:9876",
        "127.0.0.1:10911:10911",
        "127.0.0.1:8097:8097",
        "127.0.0.1:18443:8080",
    ):
        assert mapping in integration, f"integration host port must be loopback: {mapping}"

    # Unbound host ports like "5432:5432" are forbidden.
    for line in integration.splitlines():
        stripped = line.strip()
        if not stripped.startswith("- "):
            continue
        if ":" not in stripped or "127.0.0.1:" in stripped:
            continue
        port_part = stripped.lstrip("- ").strip().strip('"').strip("'")
        if port_part.count(":") == 1 and port_part.replace(":", "").isdigit():
            raise AssertionError(f"integration host port must bind 127.0.0.1: {stripped}")


def test_integration_stack_preserves_diagnostics_and_teardown_environment():
    integration_job = _section(_ci_lines(), "  integration-test:", ("  e2e-integration:",))

    assert "- name: Capture integration stack diagnostics" in integration_job
    assert "docker compose -f deploy/docker/docker-compose.distributed.yml" in integration_job
    assert "ps -a" in integration_job
    for service in ("rocketmq-namesrv", "rocketmq-namesrv-2", "rocketmq-broker", "mq-gateway"):
        assert service in integration_job
    assert integration_job.count("FMS_RUNTIME_ENV_FILE: /dev/null") == 3
    assert integration_job.count("VAULT_RENDERED_ENV_FILE: /dev/null") == 3
    # Explicit secrets required after removing weak compose defaults.
    assert integration_job.count("DB_PASSWORD: ci_explicit_db_password_not_for_prod") == 3
    assert integration_job.count("DB_PASSWORD: postgres") == 0
    assert "DB_PASSWORD: password" not in integration_job
    assert integration_job.count("REDIS_PASSWORD: redis_ci_password") == 3
    assert integration_job.count("TRUSTED_PROXY_CIDRS: 127.0.0.1/32") >= 1


def test_e2e_compose_runs_from_repository_root_with_matching_environment():
    e2e_job = _section(_ci_lines(), "  e2e-integration:", ())

    assert e2e_job.count("working-directory: ${{ github.workspace }}") == 2
    assert e2e_job.count("FMS_RUNTIME_ENV_FILE: ${{ github.workspace }}/ci_runtime.env") == 2
    assert e2e_job.count("VAULT_RENDERED_ENV_FILE: ${{ github.workspace }}/ci_runtime.env") == 2


def test_nightly_installs_mutation_tool_and_supplies_compose_environment():
    nightly_lines = _nightly_lines()
    mutation_job = _section(nightly_lines, "  mutation-test:", ("  performance-baseline:",))
    performance_job = _section(nightly_lines, "  performance-baseline:", ("  chaos-test:",))
    chaos_job = _section(nightly_lines, "  chaos-test:", ())

    assert mutation_job.index("- name: Install cargo-mutants") < mutation_job.index("- name: Run mutation pilot")
    assert "cargo install cargo-mutants --locked" in mutation_job
    for job in (performance_job, chaos_job):
        assert "- name: Generate minimal runtime env for stack" in job
        # At least the compose bring-up and tear-down steps must receive the
        # rendered runtime env; extra consumers (e.g. the referential-integrity
        # patrol) are allowed.
        assert job.count("FMS_RUNTIME_ENV_FILE: ${{ github.workspace }}/ci_runtime.env") >= 2
        assert job.count("VAULT_RENDERED_ENV_FILE: ${{ github.workspace }}/ci_runtime.env") >= 2


def test_frontend_audit_remains_blocking_at_high_severity():
    frontend_job = _section(_ci_lines(), "  vue-frontend:", ("  docker-compose-validate:",))

    assert "run: npm audit --audit-level=high" in frontend_job
    assert "continue-on-error" not in frontend_job


def test_architecture_contract_documents_are_tracked_inputs():
    assert (ROOT / "docs/API_ROUTE_SNAPSHOT.md").is_file()
    assert (ROOT / "docs/plans/2026-06-29-tech-debt-sweep-master-plan.md").is_file()
