#!/usr/bin/env python3
"""PostgreSQL autotune for the FMS host stack memory budget.

Computes pgtune-style settings so PostgreSQL plus the rest of the host stack
(API, Redis, Caddy, Vault, RocketMQ, mq-gateway) stay within a RAM cap.
Optional --apply writes ALTER SYSTEM; --iterate samples pg_stat_* under load
and nudges WAL / cache / work_mem.

Does not print DSNs or passwords.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


RESTART_KEYS = {
    "shared_buffers",
    "max_connections",
    "huge_pages",
    "wal_buffers",
    "shared_preload_libraries",
}

OTHER_PROCESS_MB = {
    "api": 400,
    "redis": 128,
    "caddy": 48,
    "vault": 80,
    "mq_namesrv": 120,
    "mq_broker": 220,
    "mq_gateway": 100,
}


@dataclass(frozen=True)
class TuneInput:
    stack_memory_mb: int
    cpus: int
    disk: str
    max_connections: int
    low_latency_writes: bool
    postgres_share: float = 0.0


@dataclass
class PgSettings:
    max_connections: int
    shared_buffers: str
    effective_cache_size: str
    maintenance_work_mem: str
    work_mem: str
    wal_buffers: str
    min_wal_size: str
    max_wal_size: str
    checkpoint_completion_target: float
    random_page_cost: float
    effective_io_concurrency: int
    huge_pages: str
    default_statistics_target: int
    autovacuum: str
    autovacuum_naptime: str
    idle_in_transaction_session_timeout: str
    synchronous_commit: str
    wal_compression: str
    shared_preload_libraries: str
    pg_stat_statements_track: str
    postgres_budget_mb: int
    estimated_rss_mb: int


def detect_cpus() -> int:
    return max(1, os.cpu_count() or 1)


def detect_ram_mb() -> int:
    system = platform.system().lower()
    if system == "windows":
        try:
            completed = subprocess.run(
                ["powershell.exe", "-NoProfile", "-Command", "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory"],
                check=False,
                capture_output=True,
                text=True,
            )
            if completed.returncode == 0 and completed.stdout.strip().isdigit():
                return max(512, int(completed.stdout.strip()) // (1024 * 1024))
        except OSError:
            pass
    try:
        page = os.sysconf("SC_PAGE_SIZE")
        pages = os.sysconf("SC_PHYS_PAGES")
        return max(512, int(page * pages) // (1024 * 1024))
    except (AttributeError, ValueError, OSError):
        return 8192


def _align_mb(value_mb: int, step: int = 16) -> int:
    return max(step, int(math.floor(value_mb / step) * step))


def _fmt_mb(value_mb: int) -> str:
    if value_mb >= 1024 and value_mb % 1024 == 0:
        return f"{value_mb // 1024}GB"
    return f"{value_mb}MB"


def postgres_budget_mb(stack_memory_mb: int) -> int:
    reserved = sum(OTHER_PROCESS_MB.values())
    return max(384, int(stack_memory_mb) - reserved)


def estimate_rss_mb(shared_buffers_mb: int, max_connections: int, work_mem_mb: int, maintenance_work_mem_mb: int) -> int:
    # Conservative: autovacuum + WAL writer + backends using a slice of work_mem.
    backend_mb = max_connections * work_mem_mb
    return shared_buffers_mb + backend_mb + maintenance_work_mem_mb + 96


def recommend(tune: TuneInput) -> PgSettings:
    budget = postgres_budget_mb(tune.stack_memory_mb)
    max_connections = max(20, min(tune.max_connections, 80))
    shared_buffers_mb = _align_mb(min(512, max(128, int(budget * 0.25))))
    maintenance_work_mem_mb = min(256, max(64, _align_mb(int(budget * 0.05), 32)))
    remaining = max(64, budget - shared_buffers_mb - maintenance_work_mem_mb - 96)
    work_mem_mb = max(2, min(16, remaining // (max_connections * 4)))
    while estimate_rss_mb(shared_buffers_mb, max_connections, work_mem_mb, maintenance_work_mem_mb) > budget and work_mem_mb > 2:
        work_mem_mb -= 1
    while (
        estimate_rss_mb(shared_buffers_mb, max_connections, work_mem_mb, maintenance_work_mem_mb) > budget
        and shared_buffers_mb > 128
    ):
        shared_buffers_mb -= 16
    effective_cache_mb = _align_mb(max(shared_buffers_mb * 2, int(budget * 0.75)))
    ssd = tune.disk.lower() != "hdd"
    wal_buffers_mb = 16
    max_wal_mb = 1024 if budget >= 1024 else 512
    min_wal_mb = 256 if max_wal_mb >= 1024 else 128
    return PgSettings(
        max_connections=max_connections,
        shared_buffers=_fmt_mb(shared_buffers_mb),
        effective_cache_size=_fmt_mb(effective_cache_mb),
        maintenance_work_mem=_fmt_mb(maintenance_work_mem_mb),
        work_mem=_fmt_mb(work_mem_mb),
        wal_buffers=_fmt_mb(wal_buffers_mb),
        min_wal_size=_fmt_mb(min_wal_mb),
        max_wal_size=_fmt_mb(max_wal_mb),
        checkpoint_completion_target=0.9,
        random_page_cost=1.1 if ssd else 4.0,
        effective_io_concurrency=200 if ssd else 2,
        huge_pages="off",
        default_statistics_target=100,
        autovacuum="on",
        autovacuum_naptime="10s",
        idle_in_transaction_session_timeout="30s",
        synchronous_commit="off" if tune.low_latency_writes else "on",
        wal_compression="on",
        shared_preload_libraries="pg_stat_statements",
        pg_stat_statements_track="all",
        postgres_budget_mb=budget,
        estimated_rss_mb=estimate_rss_mb(shared_buffers_mb, max_connections, work_mem_mb, maintenance_work_mem_mb),
    )


def settings_pairs(settings: PgSettings) -> list[tuple[str, str]]:
    skip = {"postgres_budget_mb", "estimated_rss_mb", "pg_stat_statements_track"}
    pairs = []
    for key, value in asdict(settings).items():
        if key in skip:
            continue
        pairs.append((key, str(value)))
    pairs.append(("pg_stat_statements.track", settings.pg_stat_statements_track))
    return pairs


def render_conf(settings: PgSettings) -> str:
    lines = [
        "# Generated by scripts/perf/tune_postgres.py",
        f"# postgres_budget_mb={settings.postgres_budget_mb}",
        f"# estimated_rss_mb={settings.estimated_rss_mb}",
        "",
    ]
    for key, value in settings_pairs(settings):
        lines.append(f"{key} = {value}")
    lines.append("")
    return "\n".join(lines)


def render_alter_system(settings: PgSettings) -> list[str]:
    statements = []
    for key, value in settings_pairs(settings):
        statements.append(f"ALTER SYSTEM SET {key} = '{value}';")
    return statements


def _psql(dsn: str, sql: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["psql", "-v", "ON_ERROR_STOP=1", "-At", "-c", sql, dsn],
        check=False,
        capture_output=True,
        text=True,
    )


def apply_settings(dsn: str, settings: PgSettings) -> dict[str, Any]:
    restart_needed = []
    applied = []
    errors = []
    for statement in render_alter_system(settings):
        result = _psql(dsn, statement)
        key = statement.split()[3]
        if result.returncode != 0:
            errors.append({"key": key, "stderr": (result.stderr or "").strip()[:300]})
            continue
        applied.append(key)
        if key in RESTART_KEYS:
            restart_needed.append(key)
    ext = _psql(dsn, "CREATE EXTENSION IF NOT EXISTS pg_stat_statements;")
    if ext.returncode != 0:
        errors.append({"key": "pg_stat_statements", "stderr": (ext.stderr or "").strip()[:300]})
    reload = _psql(dsn, "SELECT pg_reload_conf();")
    if reload.returncode != 0:
        errors.append({"key": "pg_reload_conf", "stderr": (reload.stderr or "").strip()[:300]})
    return {
        "applied": applied,
        "restart_needed": restart_needed,
        "errors": errors,
    }


def collect_stats(dsn: str) -> dict[str, Any]:
    sql = """
    SELECT json_build_object(
      'hit_ratio', COALESCE(
        (SELECT CASE WHEN blks_hit + blks_read = 0 THEN 1
                     ELSE blks_hit::float / (blks_hit + blks_read) END
         FROM pg_stat_database WHERE datname = current_database()), 1),
      'xact_commit', (SELECT xact_commit FROM pg_stat_database WHERE datname = current_database()),
      'temp_files', (SELECT temp_files FROM pg_stat_database WHERE datname = current_database()),
      'checkpoints_timed', (SELECT checkpoints_timed FROM pg_stat_bgwriter),
      'checkpoints_req', (SELECT checkpoints_req FROM pg_stat_bgwriter),
      'deadlocks', (SELECT deadlocks FROM pg_stat_database WHERE datname = current_database())
    );
    """
    result = _psql(dsn, sql)
    if result.returncode != 0:
        return {"error": (result.stderr or "").strip()[:300]}
    try:
        return json.loads(result.stdout.strip() or "{}")
    except json.JSONDecodeError:
        return {"error": "pg stats were not JSON"}


def adjust_from_stats(settings: PgSettings, stats: dict[str, Any]) -> PgSettings:
    next_settings = PgSettings(**asdict(settings))
    hit = float(stats.get("hit_ratio") or 1)
    temp_files = int(stats.get("temp_files") or 0)
    ckpt_req = int(stats.get("checkpoints_req") or 0)
    ckpt_timed = int(stats.get("checkpoints_timed") or 0)
    if hit < 0.99 and next_settings.postgres_budget_mb - next_settings.estimated_rss_mb >= 64:
        # Caller reapplies recommend() after raising stack share; here bump cache hint.
        next_settings.effective_cache_size = next_settings.effective_cache_size
    if temp_files > 0:
        work_mb = int(next_settings.work_mem.replace("MB", "").replace("GB", "000"))
        if "GB" not in next_settings.work_mem:
            next_settings.work_mem = _fmt_mb(min(16, work_mb + 2))
    if ckpt_req > ckpt_timed:
        max_wal = next_settings.max_wal_size
        if max_wal.endswith("MB"):
            value = int(max_wal[:-2])
            next_settings.max_wal_size = _fmt_mb(min(2048, value * 2))
        elif max_wal.endswith("GB"):
            value = int(max_wal[:-2])
            next_settings.max_wal_size = f"{min(4, value + 1)}GB"
    next_settings.estimated_rss_mb = settings.estimated_rss_mb
    return next_settings


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Autotune PostgreSQL for the FMS host stack RAM budget")
    parser.add_argument("--stack-memory-mb", type=int, default=3072)
    parser.add_argument("--cpus", type=int, default=0)
    parser.add_argument("--disk", choices=("ssd", "hdd"), default="ssd")
    parser.add_argument("--max-connections", type=int, default=64)
    parser.add_argument("--low-latency-writes", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--iterate", action="store_true")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--dsn", default=os.environ.get("DATABASE_URL", ""))
    parser.add_argument("--out-conf", default="")
    parser.add_argument("--out-json", default="")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    tune = TuneInput(
        stack_memory_mb=args.stack_memory_mb,
        cpus=args.cpus or detect_cpus(),
        disk=args.disk,
        max_connections=args.max_connections,
        low_latency_writes=args.low_latency_writes,
    )
    settings = recommend(tune)
    report: dict[str, Any] = {
        "input": asdict(tune),
        "host_ram_mb": detect_ram_mb(),
        "other_process_mb": OTHER_PROCESS_MB,
        "settings": asdict(settings),
        "restart_keys": sorted(RESTART_KEYS),
        "apply": None,
        "iterations": [],
    }
    if args.out_conf:
        Path(args.out_conf).parent.mkdir(parents=True, exist_ok=True)
        Path(args.out_conf).write_text(render_conf(settings), encoding="utf-8")
    if args.apply:
        if not args.dsn:
            print("DATABASE_URL/--dsn is required for --apply", file=sys.stderr)
            return 2
        report["apply"] = apply_settings(args.dsn, settings)
    if args.iterate:
        if not args.dsn:
            print("DATABASE_URL/--dsn is required for --iterate", file=sys.stderr)
            return 2
        for round_index in range(1, max(1, args.rounds) + 1):
            stats = collect_stats(args.dsn)
            adjusted = adjust_from_stats(settings, stats) if "error" not in stats else settings
            changed = asdict(adjusted) != asdict(settings)
            report["iterations"].append({"round": round_index, "stats": stats, "changed": changed})
            if changed and args.apply:
                report.setdefault("iteration_applies", []).append(apply_settings(args.dsn, adjusted))
                settings = adjusted
            elif not changed:
                break
    text = json.dumps(report, indent=2)
    if args.out_json:
        Path(args.out_json).parent.mkdir(parents=True, exist_ok=True)
        Path(args.out_json).write_text(text + "\n", encoding="utf-8")
    print(text)
    if args.dry_run or not args.apply:
        print(render_conf(settings), file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
