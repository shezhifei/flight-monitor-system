"""Live integration check for the MCP resource-read endpoint against REAL
Postgres + REAL Redis.

Seeds an ``ai_mcp_servers`` row and an ``ai_entity_mcp_bindings`` row via the real
``PostgresMcpRepository`` (production write path), pre-seeds the MCP resource cache
in real Redis via the real ``AiCacheManager``, then drives the real
``read_mcp_resource`` handler through a FastAPI TestClient. Only ``_resolve_repos``
and ``require_service_identity`` are patched (DI + edge-auth seams); the repo, the
DB, the cache, and the handler logic are all real.

Validates the three boundary outcomes that short-circuit BEFORE any MCP subprocess
is spawned — exactly the behaviors that require live DB + Redis:
  * enabled-binding lookup against real DB rows (403 when missing),
  * allowed_resources whitelist enforcement (403),
  * cache-first hit served from real Redis (200, cached, no subprocess).

Run: ./.venv/Scripts/python.exe scripts/tools/live_check_mcp_resource.py
Requires: Postgres (DATABASE_URL / DB_*) and Redis (REDIS_URL) reachable.
"""

from __future__ import annotations

import asyncio
import os
import sys
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SIDE = ROOT / "services" / "ai-sidecar"
sys.path.insert(0, str(SIDE))

ENTITY = "todo_graph_pilot"  # an existing ai_entities.id (binding.entity_id FK target)
SRV = "live-srv"
BIND = "live-bind"
URI_OK = "file://live-ok"
URI_FORBIDDEN = "file://live-forbidden"


def _load_env():
    env = {}
    for line in open(ROOT / ".env", encoding="utf-8-sig"):
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        env[k.strip()] = v.strip()
    return env


async def main() -> int:
    env = _load_env()
    dsn = os.environ.get("DATABASE_URL") or (
        f"postgresql://{env['DB_USER']}:{env['DB_PASSWORD']}@"
        f"{env.get('DB_HOST', 'localhost')}:{env.get('DB_PORT', '5432')}/{env['DB_NAME']}"
    )
    redis_url = os.environ.get("REDIS_URL", "redis://localhost:6379/0")

    import json as _json

    import asyncpg
    import redis.asyncio as redis

    from src.infrastructure.ai.cache_manager import AiCacheManager
    from src.infrastructure.ai.mcp_repository import PostgresMcpRepository
    from src.infrastructure.ai.management_routes import read_mcp_resource
    from src.infrastructure.ai.mcp import command_allowlist

    # The command-allowlist check now precedes the cache read; allowlist npx so
    # the happy-path cache hit is reachable. Source of truth stays the env var.
    os.environ["AI_MCP_COMMAND_ALLOWLIST_JSON"] = _json.dumps(
        {"npx": {"executable": "npx", "args_prefix": []}}
    )
    command_allowlist.reset_cache()

    class _Req:
        """Minimal Starlette-Request stand-in (require_service_identity is patched)."""
        def __init__(self, body):
            self._body = body

        async def json(self):
            return self._body

    async def _call(entity, srv, uri):
        resp = await read_mcp_resource(_Req({"resource_uri": uri}), entity, srv)
        return resp.status_code, _json.loads(resp.body)

    pool = await asyncpg.create_pool(dsn=dsn, min_size=1, max_size=2)
    client = redis.from_url(redis_url, decode_responses=True)
    print(f"[infra] Postgres OK; Redis ping -> {await client.ping()}")

    repo = PostgresMcpRepository(pool)
    cache = AiCacheManager(redis_client=client)
    failures = []

    try:
        # --- Seed via the real production write path -------------------------
        await repo.upsert_server(SRV, {
            "display_name": "Live Test MCP",
            "transport": "stdio",
            "command_ref": "npx",
            "status": "active",
        })
        await repo.upsert_binding(BIND, {
            "entity_id": ENTITY,
            "server_id": SRV,
            "enabled": True,
            "allowed_resources": [URI_OK],
        })
        # Confirm the rows are really readable back from Postgres.
        srv_row = await repo.find_server_by_id(SRV)
        binds = await repo.find_bindings_by_entity(ENTITY)
        print(f"[db] server status={srv_row['status']} transport={srv_row['transport']}; "
              f"bindings={len(binds)} enabled={binds[0]['enabled'] if binds else None}")
        # Pre-seed the resource cache in real Redis.
        await cache.set_mcp_resource(SRV, URI_OK, "LIVE-CACHED-CONTENT", ttl_seconds=300)

        repos = {
            "capability_resolver": None, "mcp_repo": repo, "mcp_client_manager": None,
            "skill_repo": None, "cache_metrics_repo": None, "cache_manager": cache,
            "model_catalog_repo": None,
        }

        with patch("src.infrastructure.ai.management_routes._resolve_repos", return_value=repos), \
             patch("src.infrastructure.ai.management_routes.require_service_identity", return_value=None):

            # 1) Cache hit (real DB binding + real Redis) -> 200, cached, no subprocess.
            status, body = await _call(ENTITY, SRV, URI_OK)
            print(f"[case cache-hit] {status} {body.get('data')}")
            if not (status == 200 and body.get("data", {}).get("cached") is True
                    and body["data"]["content"] == "LIVE-CACHED-CONTENT"):
                failures.append(f"cache-hit expected 200/cached/content, got {status} {body}")

            # 2) Resource not in allowed_resources -> 403 MCP_RESOURCE_NOT_ALLOWED.
            status, body = await _call(ENTITY, SRV, URI_FORBIDDEN)
            print(f"[case forbidden-uri] {status} {body.get('code')}")
            if not (status == 403 and body.get("code") == "MCP_RESOURCE_NOT_ALLOWED"):
                failures.append(f"forbidden-uri expected 403/MCP_RESOURCE_NOT_ALLOWED, got {status} {body}")

            # 3) Entity with no binding -> 403 MCP_BINDING_NOT_ENABLED (real DB query).
            status, body = await _call("no-such-ent", SRV, URI_OK)
            print(f"[case no-binding] {status} {body.get('code')}")
            if not (status == 403 and body.get("code") == "MCP_BINDING_NOT_ENABLED"):
                failures.append(f"no-binding expected 403/MCP_BINDING_NOT_ENABLED, got {status} {body}")

            # 4) Cache hit must NOT bypass the command allowlist (P0). Drop npx
            #    from the allowlist; the warm cache key is still present, yet the
            #    read must be refused before the cache is consulted.
            os.environ["AI_MCP_COMMAND_ALLOWLIST_JSON"] = "{}"
            command_allowlist.reset_cache()
            try:
                status, body = await _call(ENTITY, SRV, URI_OK)
            finally:
                os.environ["AI_MCP_COMMAND_ALLOWLIST_JSON"] = _json.dumps(
                    {"npx": {"executable": "npx", "args_prefix": []}}
                )
                command_allowlist.reset_cache()
            print(f"[case cache-hit + command not allowlisted] {status} {body.get('code')}")
            if not (status == 403 and body.get("code") == "MCP_COMMAND_NOT_ALLOWLISTED"):
                failures.append(
                    f"command-bypass expected 403/MCP_COMMAND_NOT_ALLOWLISTED, got {status} {body}"
                )

            # 5) Cache hit must NOT bypass the server-status check (P0). Flip the
            #    server to draft; the cached content must stay unreadable.
            await repo.upsert_server(SRV, {
                "display_name": "Live Test MCP",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "draft",
            })
            status, body = await _call(ENTITY, SRV, URI_OK)
            print(f"[case cache-hit + draft server] {status} {body.get('code')}")
            if not (status == 403 and body.get("code") == "MCP_SERVER_NOT_ACTIVE"):
                failures.append(
                    f"status-bypass expected 403/MCP_SERVER_NOT_ACTIVE, got {status} {body}"
                )

    finally:
        # --- Cleanup: never leave live test rows / keys behind ---------------
        try:
            await repo.delete_binding(BIND)
            await repo.delete_server(SRV)
            await client.delete(cache._build_mcp_resource_key(SRV, URI_OK))
            print("[cleanup] removed seeded rows + cache key")
        except Exception as exc:
            print(f"[cleanup] WARNING: {exc}")
        await client.aclose()
        await pool.close()

    if failures:
        print("\nLIVE CHECK 2 FAILED:")
        for f in failures:
            print("  - " + f)
        return 1
    print("\nLIVE CHECK 2 PASSED: MCP resource-read boundaries verified against real Postgres + Redis.")
    return 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
