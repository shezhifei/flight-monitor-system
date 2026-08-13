"""Architecture guards for retired sidecar business and AIP surfaces."""

from __future__ import annotations

import importlib
from pathlib import Path

SIDECAR_ROOT = Path(__file__).resolve().parents[2]
SRC_ROOT = SIDECAR_ROOT / "src"


RETIRED_PATHS = (
    SIDECAR_ROOT / "scripts" / "host" / "ai_sidecar_entrypoint.py",
    SRC_ROOT / "di" / "container.py",
    SRC_ROOT / "application" / "plugins" / "ai_plugin.py",
    SRC_ROOT / "application" / "api" / "routes" / "aip_ontology_routes.py",
    SRC_ROOT / "application" / "interfaces" / "service_contracts.py",
    SRC_ROOT / "application" / "services" / "aip_ontology_service.py",
    SRC_ROOT / "application" / "services" / "async_unit_of_work.py",
    SRC_ROOT / "application" / "services" / "async_todo_service.py",
    SRC_ROOT / "application" / "services" / "ai" / "todo_agent_service.py",
    SRC_ROOT / "application" / "services" / "ai" / "todo_chain_service.py",
    SRC_ROOT / "application" / "services" / "anomaly" / "anomaly_detection_service.py",
    SRC_ROOT / "application" / "services" / "anomaly" / "ports.py",
    SRC_ROOT / "application" / "services" / "dispatch" / "dispatch_conflict_service.py",
    SRC_ROOT / "application" / "services" / "dispatch" / "dispatch_command_service",
    SRC_ROOT / "application" / "services" / "dispatch" / "dispatch_query_service.py",
    SRC_ROOT / "application" / "services" / "dispatch" / "dispatch_rule_service.py",
    SRC_ROOT / "application" / "services" / "dispatch" / "dispatch_schedule_service.py",
    SRC_ROOT / "application" / "services" / "flight" / "flight_command_gateway.py",
    SRC_ROOT / "domain" / "models" / "aip_ontology_models.py",
    SRC_ROOT / "domain" / "aggregates" / "todo_aggregate.py",
    SRC_ROOT / "infrastructure" / "repositories" / "todo_agent_context_repository.py",
    SRC_ROOT / "infrastructure" / "repositories" / "aip_ontology_repository.py",
    SRC_ROOT / "infrastructure" / "ai" / "aip",
    SRC_ROOT / "infrastructure" / "ai" / "graph" / "aip_nodes.py",
    SRC_ROOT / "infrastructure" / "ai" / "ontology" / "data_loader.py",
    SRC_ROOT / "infrastructure" / "ai" / "ontology" / "schema.py",
    SRC_ROOT / "infrastructure" / "ai" / "ontology" / "objects",
    SRC_ROOT / "infrastructure" / "ai" / "ontology" / "query_engine.py",
    SRC_ROOT / "infrastructure" / "ai" / "ontology" / "security",
    SRC_ROOT / "infrastructure" / "ai" / "tools" / "dispatch_command_executor.py",
    SRC_ROOT / "infrastructure" / "ai" / "tools" / "dispatch_command_tools.py",
    SRC_ROOT / "infrastructure" / "ai" / "tools" / "business_case_tool_executor.py",
    SRC_ROOT / "infrastructure" / "ai" / "tools" / "todo_tool_executor.py",
    SRC_ROOT / "infrastructure" / "ai" / "tools" / "todo_tools.py",
    SRC_ROOT / "shared" / "uow_context.py",
)


def test_retired_sidecar_paths_are_absent() -> None:
    remaining = [
        str(path.relative_to(SIDECAR_ROOT))
        for path in RETIRED_PATHS
        if path.is_file() or (path.is_dir() and any(path.rglob("*.py")))
    ]
    assert remaining == [], f"retired sidecar paths remain: {remaining}"


def test_sidecar_entrypoint_only_mounts_current_internal_routers() -> None:
    entrypoint = (SIDECAR_ROOT.parent.parent / "scripts" / "host" / "ai_sidecar_entrypoint.py").read_text(
        encoding="utf-8"
    )
    assert "aip_ontology_routes" not in entrypoint
    assert "application.plugins.ai_plugin" not in entrypoint
    assert "src.di.container" not in entrypoint
    assert "app.include_router(api_routes)" in entrypoint
    assert "app.include_router(management_routes)" in entrypoint
    assert '"/api/v2/' not in entrypoint


def test_remaining_service_packages_import_without_legacy_reexports() -> None:
    ai_package = importlib.import_module("src.application.services.ai")
    dispatch_package = importlib.import_module("src.application.services.dispatch")
    flight_package = importlib.import_module("src.application.services.flight")
    tools_package = importlib.import_module("src.infrastructure.ai.tools")

    assert not hasattr(ai_package, "TodoAgentService")
    assert not hasattr(dispatch_package, "DispatchCommandApplicationService")
    assert not hasattr(flight_package, "AsyncFlightApplicationService")
    assert not hasattr(tools_package, "DispatchCommandExecutor")
    assert hasattr(tools_package, "DispatchQueryExecutor")
