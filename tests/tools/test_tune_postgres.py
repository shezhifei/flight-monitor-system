from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "perf" / "tune_postgres.py"


def load_module():
    spec = importlib.util.spec_from_file_location("tune_postgres", MODULE_PATH)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_three_gig_stack_stays_within_postgres_budget() -> None:
    mod = load_module()
    settings = mod.recommend(
        mod.TuneInput(
            stack_memory_mb=3072,
            cpus=8,
            disk="ssd",
            max_connections=64,
            low_latency_writes=False,
        )
    )
    assert settings.postgres_budget_mb <= 3072 - sum(mod.OTHER_PROCESS_MB.values()) + 0
    assert settings.estimated_rss_mb <= settings.postgres_budget_mb
    assert settings.max_connections == 64
    assert settings.random_page_cost == 1.1
    assert settings.synchronous_commit == "on"
    assert "pg_stat_statements" in settings.shared_preload_libraries


def test_low_latency_writes_disables_synchronous_commit() -> None:
    mod = load_module()
    settings = mod.recommend(
        mod.TuneInput(
            stack_memory_mb=3072,
            cpus=4,
            disk="ssd",
            max_connections=32,
            low_latency_writes=True,
        )
    )
    assert settings.synchronous_commit == "off"


def test_render_alter_system_quotes_values() -> None:
    mod = load_module()
    settings = mod.recommend(
        mod.TuneInput(
            stack_memory_mb=3072,
            cpus=8,
            disk="ssd",
            max_connections=64,
            low_latency_writes=False,
        )
    )
    statements = mod.render_alter_system(settings)
    assert any(item.startswith("ALTER SYSTEM SET shared_buffers = '") for item in statements)
    conf = mod.render_conf(settings)
    assert "shared_buffers =" in conf
    assert "estimated_rss_mb=" in conf


def test_hdd_profile_uses_higher_random_page_cost() -> None:
    mod = load_module()
    settings = mod.recommend(
        mod.TuneInput(
            stack_memory_mb=3072,
            cpus=4,
            disk="hdd",
            max_connections=40,
            low_latency_writes=False,
        )
    )
    assert settings.random_page_cost == 4.0
    assert settings.effective_io_concurrency == 2
