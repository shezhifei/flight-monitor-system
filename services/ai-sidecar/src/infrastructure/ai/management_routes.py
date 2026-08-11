"""AI Configuration Management API - Internal endpoints for V2 config management.

Provides CRUD for MCP servers/bindings, Agent Skills, Cache metrics,
and Capability resolution/validation.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import uuid
from pathlib import Path
from typing import Any

from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse

from src.infrastructure.ai.mcp.client_manager import McpClientManager
from src.infrastructure.ai.service_identity import require_service_identity
from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.common.runtime_utils import decode_jsonb_or_raise, parse_json_field
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

router = APIRouter(prefix="/internal/ai/v1", tags=["AI Config Management"])


def _ok(data: Any, message: str | None = None) -> JSONResponse:
    payload: dict[str, Any] = {"success": True, "data": data}
    if message:
        payload["message"] = message
    return JSONResponse(payload)


def _err(message: str, status: int = 400, code: str | None = None) -> JSONResponse:
    payload: dict[str, Any] = {"success": False, "error": message}
    if code:
        payload["code"] = code
    return JSONResponse(payload, status_code=status)


async def _authorize_mcp_resource_read(
    mcp_repo: Any,
    entity_id: str,
    server_id: str,
    resource_uri: str,
) -> JSONResponse | None:
    bindings = await mcp_repo.find_bindings_by_entity(entity_id)
    binding = next(
        (b for b in bindings if b.get("server_id") == server_id and b.get("enabled")),
        None,
    )
    if not binding:
        return _err(
            f"MCP_BINDING_NOT_ENABLED: No enabled MCP binding for entity '{entity_id}' and server '{server_id}'",
            403,
            "MCP_BINDING_NOT_ENABLED",
        )

    try:
        allowed_resources = (
            decode_jsonb_or_raise(
                binding.get("allowed_resources"),
                "allowed_resources",
            )
            or []
        )
    except ValueError as exc:
        logger.error("mcp_resource_acl_invalid", exc_info=exc)
        return _err("MCP resource ACL invalid", 403, "MCP_RESOURCE_ACL_INVALID")

    if allowed_resources and resource_uri not in allowed_resources:
        return _err(
            "MCP resource not allowed",
            403,
            "MCP_RESOURCE_NOT_ALLOWED",
        )

    return None


_SKILL_SLUG_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")


def _is_safe_skill_slug(skill_slug: str) -> bool:
    return bool(_SKILL_SLUG_RE.fullmatch(skill_slug)) and ".." not in skill_slug


def _resolve_local_skill_file(skill_slug: str) -> Path | None:
    if not _is_safe_skill_slug(skill_slug):
        return None
    skill_root = (Path(__file__).resolve().parents[3] / "agent_skills").resolve()
    skill_file = (skill_root / skill_slug / "SKILL.md").resolve()
    try:
        skill_file.relative_to(skill_root)
    except ValueError:
        return None
    return skill_file if skill_file.is_file() else None


def _resolve_repos() -> dict[str, Any]:
    """Resolve repositories from the global container."""
    from src.infrastructure.ai.ai_container import (
        resolve_cache_manager,
        resolve_cache_metrics_repo,
        resolve_capability_resolver,
        resolve_mcp_client_manager,
        resolve_mcp_repo,
        resolve_model_catalog_repo,
        resolve_skill_repo,
    )

    return {
        "capability_resolver": resolve_capability_resolver(),
        "mcp_repo": resolve_mcp_repo(),
        "mcp_client_manager": resolve_mcp_client_manager(),
        "skill_repo": resolve_skill_repo(),
        "cache_metrics_repo": resolve_cache_metrics_repo(),
        "cache_manager": resolve_cache_manager(),
        "model_catalog_repo": resolve_model_catalog_repo(),
    }


# ---------------------------------------------------------------------------
# Capabilities
# ---------------------------------------------------------------------------


@router.get("/entities/{entity_id}/capabilities")
async def get_entity_capabilities(request: Request, entity_id: str) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    resolver = repos["capability_resolver"]
    if not resolver:
        return _err("CapabilityResolver not configured", 503, "SERVICE_NOT_CONFIGURED")

    try:
        snapshot = await resolver.resolve(entity_id=entity_id, model_purpose="chat")
    except ValueError as exc:
        logger.error("capability_validation_failed", exc_info=exc)
        return _err("Capability validation failed", 422, "CAPABILITY_VALIDATION_FAILED")
    except Exception as exc:
        logger.error("capability_resolution_failed", exc_info=exc)
        return _err("Capability resolution failed", 500, "CAPABILITY_RESOLUTION_FAILED")

    data = _snapshot_to_dict(snapshot)
    data = await _enrich_snapshot(data, entity_id, repos)
    return _ok(data)


@router.post("/entities/{entity_id}/capabilities/validate")
async def validate_entity_capabilities(request: Request, entity_id: str) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    resolver = repos["capability_resolver"]
    if not resolver:
        return _ok(
            {
                "valid": False,
                "errors": [
                    {
                        "code": "SERVICE_NOT_CONFIGURED",
                        "message": "CapabilityResolver not configured",
                        "severity": "error",
                    },
                ],
            }
        )

    errors: list[dict[str, str]] = []
    try:
        snapshot = await resolver.resolve(entity_id=entity_id, model_purpose="chat")
        if not snapshot.model_id:
            errors.append({"code": "NO_MODEL_RESOLVED", "message": "No model_id resolved", "severity": "error"})
        if snapshot.model and not snapshot.model.input_modalities:
            errors.append(
                {"code": "NO_INPUT_MODALITIES", "message": "No input modalities configured", "severity": "error"}
            )
        if snapshot.api_format not in ("chat_completions", "responses"):
            errors.append(
                {
                    "code": "INVALID_API_FORMAT",
                    "message": f"Invalid api_format: {snapshot.api_format}",
                    "severity": "error",
                }
            )
        if snapshot.mcp.enabled and snapshot.mcp.server_count == 0:
            errors.append(
                {
                    "code": "MCP_ENABLED_NO_SERVERS",
                    "message": "MCP enabled but no active server bindings",
                    "severity": "warning",
                }
            )
        if snapshot.mcp.enabled and snapshot.mcp.tool_count == 0 and snapshot.mcp.server_count > 0:
            errors.append(
                {
                    "code": "MCP_CAPABILITIES_NOT_DISCOVERED",
                    "message": "MCP servers bound but capabilities not discovered; run probe first",
                    "severity": "warning",
                }
            )
        if snapshot.skills.enabled and snapshot.skills.skill_count == 0:
            errors.append(
                {
                    "code": "SKILLS_ENABLED_NO_BINDINGS",
                    "message": "Skills enabled but no active skill bindings",
                    "severity": "warning",
                }
            )
        if snapshot.subagents.enabled and not snapshot.subagents.allowed_entity_ids:
            errors.append(
                {
                    "code": "SUBAGENT_ENABLED_EMPTY_ALLOWLIST",
                    "message": "Subagents enabled but allowed_entity_ids is empty; no delegation will be allowed",
                    "severity": "warning",
                }
            )
    except ValueError as exc:
        error_msg = str(exc)
        logger.error("capability_validate_value_error", exc_info=exc)
        if "AI_MODALITY_NOT_SUPPORTED" in error_msg:
            errors.append(
                {
                    "code": "AI_MODALITY_NOT_SUPPORTED",
                    "message": "AI modality not supported",
                    "severity": "error",
                }
            )
        else:
            errors.append(
                {
                    "code": "CAPABILITY_VALIDATION_FAILED",
                    "message": "Capability validation failed",
                    "severity": "error",
                }
            )
    except Exception as exc:
        logger.error("capability_validation_resolution_failed", exc_info=exc)
        errors.append(
            {"code": "CAPABILITY_RESOLUTION_FAILED", "message": "Resolution error", "severity": "error"}
        )

    return _ok({"valid": len(errors) == 0 or all(e.get("severity") == "warning" for e in errors), "errors": errors})


def _snapshot_to_dict(snapshot: Any) -> dict[str, Any]:
    """Convert ResolvedAiRuntimeConfig to a JSON-serializable dict."""
    from dataclasses import fields as dc_fields

    def _dc_to_dict(obj: Any) -> Any:
        if obj is None or isinstance(obj, (str, int, float, bool)):
            return obj
        if isinstance(obj, list):
            return [_dc_to_dict(item) for item in obj]
        if isinstance(obj, dict):
            return {k: _dc_to_dict(v) for k, v in obj.items()}
        if hasattr(obj, "__dataclass_fields__"):
            return {f.name: _dc_to_dict(getattr(obj, f.name)) for f in dc_fields(obj)}
        return obj

    result = _dc_to_dict(snapshot)
    # Flatten for frontend compatibility
    if isinstance(result, dict):
        model = result.get("model", {})
        if isinstance(model, dict):
            result["provider_model"] = model.get("provider_model", result.get("model_id", ""))
            result["input_modalities"] = model.get("input_modalities", ["text"])
            result["output_modalities"] = model.get("output_modalities", ["text"])
        mcp = result.get("mcp", {})
        if isinstance(mcp, dict):
            result["mcp_enabled"] = mcp.get("enabled", False)
        skills = result.get("skills", {})
        if isinstance(skills, dict):
            result["skills_enabled"] = skills.get("enabled", False)
            result["skill_instruction_count"] = skills.get("skill_count", 0)
        subagents = result.get("subagents", {})
        if isinstance(subagents, dict):
            result["subagents_enabled"] = subagents.get("enabled", False)
            result["subagent_tool_enabled"] = subagents.get("enabled", False)
        cache = result.get("cache_policy", {})
        if isinstance(cache, dict):
            result["cache_enabled"] = cache.get("enabled", False)
            result["cache_backends"] = {
                "provider_prompt_cache": cache.get("provider_prompt_cache_enabled", False),
                "provider_prompt_cache_retention": cache.get("provider_prompt_cache_retention"),
                "provider_prompt_cache_namespace": cache.get("provider_prompt_cache_namespace"),
                "tool_result_cache": cache.get("tool_result_cache_enabled", False),
                "tool_result_cache_ttl": cache.get("tool_result_cache_ttl", 0),
                "tool_result_cacheable_tools": cache.get("tool_result_cacheable_tools", []),
                "mcp_resource_cache": cache.get("mcp_resource_cache_enabled", False),
                "mcp_resource_cache_note": "runtime_integrated"
                if cache.get("mcp_resource_cache_enabled", False)
                else "disabled",
                "context_cache": cache.get("context_cache_enabled", False),
                "context_cache_note": "runtime_integrated" if cache.get("context_cache_enabled", False) else "disabled",
                "context_cache_ttl": cache.get("context_cache_ttl", 0),
            }
        tools = result.get("tools", [])
        tool_count = len(tools) if isinstance(tools, list) else 0
        result["tool_count"] = tool_count
        builtin_count = sum(1 for t in tools if isinstance(t, dict) and t.get("source") == "builtin")
        mcp_count = sum(1 for t in tools if isinstance(t, dict) and t.get("source") == "mcp")
        result["builtin_tool_count"] = builtin_count
        result["mcp_tool_count"] = mcp_count
    return result


async def _enrich_snapshot(
    data: dict[str, Any],
    entity_id: str,
    repos: dict[str, Any],
) -> dict[str, Any]:
    """Enrich snapshot dict with per-server MCP status, skill details, subagent risk indicators."""
    mcp_repo = repos.get("mcp_repo")
    skill_repo = repos.get("skill_repo")

    # --- MCP per-server details ---
    mcp_servers: list[dict[str, Any]] = []
    if mcp_repo and data.get("mcp_enabled", False):
        try:
            bindings = await mcp_repo.find_bindings_by_entity(entity_id)
            for binding in bindings:
                if not binding.get("enabled", False):
                    continue
                server_id = binding.get("server_id", "")
                server = await mcp_repo.find_server_by_id(server_id)
                display_name = (server or {}).get("display_name", server_id)
                transport = (server or {}).get("transport", "stdio")
                command_ref = (server or {}).get("command_ref")
                server_status = (server or {}).get("status", "draft")
                caps = await mcp_repo.get_capabilities(server_id)

                if transport != "stdio":
                    discovery_status = "unsupported_transport"
                elif not caps:
                    discovery_status = "not_discovered"
                else:
                    caps_tools = caps.get("tools", [])
                    if isinstance(caps_tools, str):
                        try:
                            caps_tools = json.loads(caps_tools)
                        except (json.JSONDecodeError, TypeError, ValueError):
                            caps_tools = []
                    if caps_tools:
                        discovery_status = "discovered"
                    else:
                        discovery_status = "not_discovered"

                # Check allowlist status
                command_allowlisted = True
                allowlist_configured = False
                if command_ref and transport == "stdio":
                    try:
                        from src.infrastructure.ai.mcp.command_allowlist import is_command_allowed

                        allowlist_env = os.environ.get("AI_MCP_COMMAND_ALLOWLIST_JSON", "")
                        allowlist_configured = bool(allowlist_env)
                        command_allowlisted = is_command_allowed(command_ref)
                    except (ImportError, OSError, ValueError, RuntimeError) as e:
                        logger.warning("mcp_command_allowlist_check_failed", exc_info=e)

                if not command_allowlisted:
                    discovery_status = "command_not_allowlisted"

                mcp_servers.append(
                    {
                        "server_id": server_id,
                        "display_name": display_name,
                        "enabled": True,
                        "transport": transport,
                        "command_ref_present": bool(command_ref),
                        "command_allowlisted": command_allowlisted if command_ref else None,
                        "allowlist_configured": allowlist_configured,
                        "server_status": server_status,
                        "discovery_status": discovery_status,
                    }
                )
        except POSTGRES_EXCEPTIONS as e:
            logger.warning("mcp_server_list_construction_failed", exc_info=e)
    data["mcp_servers"] = mcp_servers
    data["mcp_server_count"] = len(mcp_servers)
    data["mcp_binding_count"] = len(mcp_servers)

    # --- Skill binding details ---
    skill_bindings: list[dict[str, Any]] = []
    if skill_repo and data.get("skills_enabled", False):
        try:
            raw_bindings = await skill_repo.find_bindings_by_entity(entity_id)
            for b in raw_bindings:
                if not b.get("enabled", False):
                    continue
                skill_bindings.append(
                    {
                        "binding_id": b.get("binding_id", ""),
                        "skill_slug": b.get("skill_slug", ""),
                        "version": b.get("version", "0.0.0"),
                        "enabled": True,
                        "activation_policy": b.get("activation_policy", "always"),
                        "priority": b.get("priority", 0),
                        "max_instruction_tokens": b.get("max_instruction_tokens", 4096),
                    }
                )
        except POSTGRES_EXCEPTIONS as e:
            logger.warning("skill_binding_list_failed", exc_info=e)
    data["skill_bindings"] = skill_bindings

    # --- Subagent risk indicators ---
    subagents = data.get("subagents", {})
    if isinstance(subagents, dict) and subagents.get("enabled", False):
        allowed_ids = subagents.get("allowed_entity_ids", [])
        max_depth = subagents.get("max_depth", 1)
        data["subagent_risk"] = {
            "enabled_no_allowlist": bool(not allowed_ids),
            "high_depth": max_depth > 1,
        }
    else:
        data["subagent_risk"] = {
            "enabled_no_allowlist": False,
            "high_depth": False,
        }

    # --- MCP command allowlist env status ---
    try:
        allowlist_env = os.environ.get("AI_MCP_COMMAND_ALLOWLIST_JSON", "")
        data["mcp_allowlist_configured"] = bool(allowlist_env)
    except OSError as error:
        logger.warning("mcp_allowlist_env_read_failed", exc_info=error)
        data["mcp_allowlist_configured"] = False

    return data


# ---------------------------------------------------------------------------
# MCP Servers
# ---------------------------------------------------------------------------


@router.get("/entities/{entity_id}/mcp/servers")
async def list_mcp_servers(request: Request, entity_id: str) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    mcp_repo = repos["mcp_repo"]
    if not mcp_repo:
        return _ok([])

    try:
        servers = await mcp_repo.find_all_servers()
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("mcp_list_servers_failed", exc_info=exc)
        return _err("Failed to list MCP servers", 500, "MCP_SERVERS_LIST_FAILED")

    return _ok([_server_to_frontend(s) for s in servers])


@router.post("/entities/{entity_id}/mcp/servers")
async def create_mcp_server(request: Request, entity_id: str) -> JSONResponse:
    require_service_identity(request)
    body = await request.json()

    repos = _resolve_repos()
    mcp_repo = repos["mcp_repo"]
    if not mcp_repo:
        return _err("MCP repository not configured", 503)

    server_id = body.get("id") or f"mcp-{uuid.uuid4().hex[:8]}"
    body["id"] = server_id

    try:
        saved = await mcp_repo.upsert_server(server_id, _server_from_frontend(body))
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("mcp_create_server_failed", exc_info=exc)
        return _err("Failed to create MCP server", 500, "MCP_SERVER_CREATE_FAILED")

    return _ok(_server_to_frontend(saved))


@router.put("/entities/{entity_id}/mcp/servers/{server_id}")
async def update_mcp_server(request: Request, entity_id: str, server_id: str) -> JSONResponse:
    require_service_identity(request)
    body = await request.json()

    repos = _resolve_repos()
    mcp_repo = repos["mcp_repo"]
    if not mcp_repo:
        return _err("MCP repository not configured", 503)

    existing = await mcp_repo.find_server_by_id(server_id)
    if not existing:
        return _err(f"MCP server not found: {server_id}", 404)

    body["id"] = server_id
    try:
        saved = await mcp_repo.upsert_server(server_id, _server_from_frontend(body))
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("mcp_update_server_failed", exc_info=exc)
        return _err("Failed to update MCP server", 500, "MCP_SERVER_UPDATE_FAILED")

    return _ok(_server_to_frontend(saved))


@router.delete("/entities/{entity_id}/mcp/servers/{server_id}")
async def delete_mcp_server(request: Request, entity_id: str, server_id: str) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    mcp_repo = repos["mcp_repo"]
    if not mcp_repo:
        return _err("MCP repository not configured", 503)

    existing = await mcp_repo.find_server_by_id(server_id)
    if not existing:
        return _err(f"MCP server not found: {server_id}", 404)

    try:
        deleted = await mcp_repo.delete_server(server_id)
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("mcp_delete_server_failed", exc_info=exc)
        return _err("Failed to delete MCP server", 500, "MCP_SERVER_DELETE_FAILED")

    return _ok(deleted)


@router.post("/entities/{entity_id}/mcp/servers/{server_id}/probe")
async def probe_mcp_server(request: Request, entity_id: str, server_id: str) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    mcp_repo = repos["mcp_repo"]
    if not mcp_repo:
        return _err("MCP repository not configured", 503)

    server = await mcp_repo.find_server_by_id(server_id)
    if not server:
        return _err(f"MCP server not found: {server_id}", 404)

    # Only probe active/enabled servers
    server_status = server.get("status", "draft")
    if server_status not in ("active", "enabled"):
        return _err(
            f"MCP server {server_id} status is '{server_status}'; only active/enabled servers can be probed",
            400,
        )

    # Helper to build capability response from a capabilities row
    def _caps_to_response(caps: dict[str, Any]) -> JSONResponse:
        tools_raw = caps.get("tools", [])
        if isinstance(tools_raw, str):
            try:
                tools_raw = json.loads(tools_raw)
            except (json.JSONDecodeError, TypeError, ValueError):
                tools_raw = []
        resources_raw = caps.get("resources", [])
        if isinstance(resources_raw, str):
            try:
                resources_raw = json.loads(resources_raw)
            except (json.JSONDecodeError, TypeError, ValueError):
                resources_raw = []
        prompts_raw = caps.get("prompts", [])
        if isinstance(prompts_raw, str):
            try:
                prompts_raw = json.loads(prompts_raw)
            except (json.JSONDecodeError, TypeError, ValueError):
                prompts_raw = []
        return _ok(
            {
                "status": "discovered",
                "capabilities": {
                    "server_id": server_id,
                    "protocol_version": caps.get("protocol_version"),
                    "tools": tools_raw,
                    "resources": resources_raw,
                    "prompts": prompts_raw,
                    "schema_hash": caps.get("schema_hash", ""),
                    "discovered_at": str(caps.get("discovered_at", "")),
                },
            }
        )

    transport = server.get("transport", "stdio")

    if transport != "stdio":
        return _ok(
            {
                "status": "unsupported_transport",
                "capabilities": None,
                "error": f"Transport '{transport}' is not yet supported for discovery",
            }
        )

    command_ref = server.get("command_ref")
    if not command_ref:
        # No command_ref means we cannot do real discovery; return cached or not_found
        caps = await mcp_repo.get_capabilities(server_id)
        if caps:
            return _caps_to_response(caps)
        return _ok(
            {
                "status": "not_discovered",
                "capabilities": None,
            }
        )

    timeout = server.get("timeout_seconds", 30)
    startup_timeout = server.get("startup_timeout_seconds", 10)

    # Resolve allowlist from secure config — NO dynamic bypass
    from src.infrastructure.ai.mcp.command_allowlist import (
        build_client_allowlist,
        is_command_allowed,
    )

    if not is_command_allowed(command_ref):
        return _ok(
            {
                "status": "not_discovered",
                "capabilities": None,
                "error": f"MCP_COMMAND_NOT_ALLOWLISTED: command_ref '{command_ref}' is not in the configured allowlist",
            }
        )

    allowlist = build_client_allowlist(command_ref)

    args = server.get("args", [])
    if isinstance(args, str):
        try:
            args = json.loads(args)
        except (json.JSONDecodeError, TypeError, ValueError):
            args = []

    try:
        client_mgr = McpClientManager(
            mcp_repo=mcp_repo,
            command_allowlist=allowlist,
        )

        server_config = dict(server)
        server_config["command_ref"] = command_ref
        if args:
            server_config["args"] = args

        await client_mgr.connect_server(
            server_id,
            server_config,
            timeout=timeout,
            startup_timeout=startup_timeout,
        )

        discovery = await client_mgr.discover_all(
            server_id,
            timeout=timeout,
        )

        await client_mgr.disconnect_server(server_id)

        # Store capabilities in repository
        tools_raw = [
            {
                "name": t.name,
                "description": t.description,
                "inputSchema": t.parameters,
                "annotations": {
                    "cacheable": t.cacheable,
                    "destructive": t.side_effect,
                },
            }
            for t in discovery["tools"]
        ]
        resources_raw = [
            {
                "uri": r.uri,
                "name": r.name,
                "description": r.description,
                "mimeType": r.mime_type,
            }
            for r in discovery["resources"]
        ]
        prompts_raw = discovery["prompts"]

        await mcp_repo.upsert_capabilities(
            server_id,
            {
                "protocol_version": "2024-11-05",
                "tools": tools_raw,
                "resources": resources_raw,
                "prompts": prompts_raw,
                "schema_hash": discovery["schema_hash"],
                "expires_at": None,
            },
        )

        return _ok(
            {
                "status": "discovered",
                "capabilities": {
                    "server_id": server_id,
                    "protocol_version": "2024-11-05",
                    "tools": tools_raw,
                    "resources": resources_raw,
                    "prompts": prompts_raw,
                    "schema_hash": discovery["schema_hash"],
                },
            }
        )

    except ValueError as exc:
        error_msg = str(exc)
        if "not in allowlist" in error_msg:
            logger.error("mcp_command_not_allowlisted", exc_info=exc)
            return _ok(
                {
                    "status": "not_discovered",
                    "capabilities": None,
                    "error": "MCP_COMMAND_NOT_ALLOWLISTED",
                }
            )
        logger.error("mcp_discovery_value_error", exc_info=exc)
        return _ok(
            {
                "status": "not_discovered",
                "capabilities": None,
                "error": "internal_error",
            }
        )
    except RuntimeError as exc:
        logger.error("mcp_discovery_runtime_error", exc_info=exc)
        return _ok(
            {
                "status": "discovery_failed",
                "capabilities": None,
                "error": "internal_error",
            }
        )
    except TimeoutError as exc:
        logger.error("mcp_discovery_timeout", exc_info=exc)
        return _ok(
            {
                "status": "discovery_failed",
                "capabilities": None,
                "error": "MCP_DISCOVERY_TIMEOUT",
            }
        )
    except OSError as exc:
        logger.error("mcp_discovery_os_error", exc_info=exc)
        return _ok(
            {
                "status": "discovery_failed",
                "capabilities": None,
                "error": "MCP discovery failed",
            }
        )
    except Exception as exc:
        logger.error("mcp_discovery_failed", exc_info=exc)
        return _ok(
            {
                "status": "discovery_failed",
                "capabilities": None,
                "error": "internal_error",
            }
        )


@router.post("/entities/{entity_id}/mcp/servers/{server_id}/resource")
async def read_mcp_resource(request: Request, entity_id: str, server_id: str) -> JSONResponse:
    """Read an MCP resource's content through the cache-integrated client.

    Security boundaries:
      * server must exist, be active/enabled, and use a supported (stdio) transport;
      * its ``command_ref`` must be present in the env-sourced allowlist (no bypass);
      * entity binding + allowed_resources ACL is enforced INSIDE ``McpClientManager.read_resource``
        (the single authority), which also performs the cache read AFTER authorization.
    """
    require_service_identity(request)
    body = await request.json()
    resource_uri = (body or {}).get("resource_uri", "")
    if not resource_uri:
        return _err("resource_uri is required", 422, "RESOURCE_URI_REQUIRED")

    repos = _resolve_repos()
    mcp_repo = repos["mcp_repo"]
    cache_manager = repos["cache_manager"]
    if not mcp_repo:
        return _err("MCP repository not configured", 503)

    server = await mcp_repo.find_server_by_id(server_id)
    if not server:
        return _err(f"MCP server not found: {server_id}", 404)

    # Server-config security boundaries (status / transport / command allowlist).
    # Entity binding + allowed_resources ACL is enforced inside read_resource() itself.
    server_status = server.get("status", "draft")
    if server_status not in ("active", "enabled"):
        return _err(
            f"MCP server {server_id} status is '{server_status}'; only active/enabled servers can read resources",
            403,
            "MCP_SERVER_NOT_ACTIVE",
        )

    transport = server.get("transport", "stdio")
    if transport != "stdio":
        return _err(
            f"Transport '{transport}' is not yet supported for resource read",
            400,
            "MCP_UNSUPPORTED_TRANSPORT",
        )

    acl_error = await _authorize_mcp_resource_read(mcp_repo, entity_id, server_id, resource_uri)
    if acl_error is not None:
        return acl_error

    command_ref = server.get("command_ref")
    from src.infrastructure.ai.mcp.command_allowlist import (
        build_client_allowlist,
        is_command_allowed,
    )

    if not command_ref or not is_command_allowed(command_ref):
        return _err(
            f"MCP_COMMAND_NOT_ALLOWLISTED: command_ref '{command_ref}' is not allowlisted",
            403,
            "MCP_COMMAND_NOT_ALLOWLISTED",
        )

    ttl = int(server.get("timeout_seconds", 30))
    resource_ttl = 300
    cache_cfg = server.get("cache", {})
    if isinstance(cache_cfg, dict):
        resource_ttl = int(cache_cfg.get("resources_ttl_seconds", resource_ttl))

    if cache_manager is not None:
        cached = await cache_manager.get_mcp_resource(
            server_id, resource_uri, ttl_seconds=resource_ttl, entity_id=entity_id
        )
        if cached is not None:
            return _ok(
                {
                    "content": cached,
                    "cached": True,
                    "server_id": server_id,
                    "resource_uri": resource_uri,
                }
            )

    args = server.get("args", [])
    if isinstance(args, str):
        try:
            args = json.loads(args)
        except (json.JSONDecodeError, TypeError, ValueError):
            args = []

    client_mgr = McpClientManager(
        mcp_repo=mcp_repo,
        command_allowlist=build_client_allowlist(command_ref),
        cache_manager=cache_manager,
    )
    server_config = dict(server)
    server_config["command_ref"] = command_ref
    if args:
        server_config["args"] = args

    try:
        await client_mgr.connect_server(
            server_id,
            server_config,
            timeout=ttl,
            startup_timeout=int(server.get("startup_timeout_seconds", 10)),
        )
        result = await client_mgr.read_resource(
            server_id, resource_uri, ttl_seconds=resource_ttl, timeout=ttl, entity_id=entity_id
        )
        return _ok(result)
    except ValueError as exc:
        logger.error("mcp_resource_invalid_request", exc_info=exc)
        return _err("Invalid resource request", 400, "MCP_RESOURCE_INVALID_REQUEST")
    except PermissionError as exc:
        msg = str(exc)
        logger.error("mcp_resource_permission_denied", exc_info=exc)
        if "MCP_BINDING_NOT_ENABLED" in msg:
            return _err("MCP binding not enabled", 403, "MCP_BINDING_NOT_ENABLED")
        if "MCP_RESOURCE_NOT_ALLOWED" in msg:
            return _err("MCP resource not allowed", 403, "MCP_RESOURCE_NOT_ALLOWED")
        if "MCP_RESOURCE_ACL_INVALID" in msg:
            return _err("MCP resource ACL invalid", 403, "MCP_RESOURCE_ACL_INVALID")
        return _err("MCP access denied", 403, "MCP_ACCESS_DENIED")
    except Exception as exc:
        logger.error("mcp_resource_read_failed", exc_info=exc)
        return _err("MCP resource read failed", 502, "MCP_RESOURCE_READ_FAILED")
    finally:
        try:
            await client_mgr.disconnect_server(server_id)
        except Exception as e:  # noqa: BLE001 - cleanup block in finally must not raise
            logger.debug("mcp_disconnect_failed server_id=%s", server_id, exc_info=e)


# ---------------------------------------------------------------------------
# MCP Bindings
# ---------------------------------------------------------------------------


@router.get("/entities/{entity_id}/mcp/bindings")
async def list_mcp_bindings(request: Request, entity_id: str) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    mcp_repo = repos["mcp_repo"]
    if not mcp_repo:
        return _ok([])

    try:
        bindings = await mcp_repo.find_bindings_by_entity(entity_id)
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("mcp_list_bindings_failed", exc_info=exc)
        return _err("Failed to list MCP bindings", 500, "MCP_BINDINGS_LIST_FAILED")

    return _ok([_binding_to_frontend(b) for b in bindings])


@router.post("/entities/{entity_id}/mcp/bindings")
async def save_mcp_binding(request: Request, entity_id: str) -> JSONResponse:
    require_service_identity(request)
    body = await request.json()

    repos = _resolve_repos()
    mcp_repo = repos["mcp_repo"]
    if not mcp_repo:
        return _err("MCP repository not configured", 503)

    binding_id = body.get("binding_id") or f"mcp-bind-{entity_id}-{uuid.uuid4().hex[:8]}"
    body["entity_id"] = entity_id
    body["binding_id"] = binding_id

    try:
        saved = await mcp_repo.upsert_binding(binding_id, body)
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("mcp_save_binding_failed", exc_info=exc)
        return _err("Failed to save MCP binding", 500, "MCP_BINDING_SAVE_FAILED")

    return _ok(_binding_to_frontend(saved))


# ---------------------------------------------------------------------------
# Skills
# ---------------------------------------------------------------------------


@router.get("/skills")
async def list_skill_registry(request: Request) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    skill_repo = repos["skill_repo"]
    if not skill_repo:
        return _ok([])

    try:
        skills = await skill_repo.find_all_skills()
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("skills_list_failed", exc_info=exc)
        return _err("Failed to list skills", 500, "SKILLS_LIST_FAILED")

    return _ok([_skill_to_frontend(s) for s in skills])


@router.get("/entities/{entity_id}/skills")
async def list_entity_skills(request: Request, entity_id: str) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    skill_repo = repos["skill_repo"]
    if not skill_repo:
        return _ok([])

    try:
        bindings = await skill_repo.find_bindings_by_entity(entity_id)
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("entity_skills_list_failed", exc_info=exc)
        return _err("Failed to list entity skills", 500, "ENTITY_SKILLS_LIST_FAILED")

    return _ok([_skill_binding_to_frontend(b) for b in bindings])


@router.post("/entities/{entity_id}/skills/{skill_slug}/probe")
async def probe_skill(request: Request, entity_id: str, skill_slug: str) -> JSONResponse:
    require_service_identity(request)
    if not _is_safe_skill_slug(skill_slug):
        return _err("Invalid skill slug", 400, "INVALID_SKILL_SLUG")

    repos = _resolve_repos()
    skill_repo = repos["skill_repo"]
    if not skill_repo:
        return _ok({"status": "not_found", "content_hash": None})

    skill = await skill_repo.find_skill(skill_slug)
    if skill:
        return _ok(
            {
                "status": "ok",
                "content_hash": skill.get("content_hash", ""),
            }
        )

    # Check local skill files as fallback, constrained to the sidecar skill root.
    skill_file = _resolve_local_skill_file(skill_slug)
    if skill_file is not None:
        content = skill_file.read_bytes()
        content_hash = hashlib.sha256(content).hexdigest()[:16]
        return _ok({"status": "ok", "content_hash": content_hash})

    return _ok({"status": "not_found", "content_hash": None})


@router.post("/entities/{entity_id}/skills/bindings")
async def save_skill_binding(request: Request, entity_id: str) -> JSONResponse:
    require_service_identity(request)
    body = await request.json()

    repos = _resolve_repos()
    skill_repo = repos["skill_repo"]
    if not skill_repo:
        return _err("Skill repository not configured", 503)

    binding_id = body.get("binding_id") or f"skill-bind-{entity_id}-{uuid.uuid4().hex[:8]}"
    body["entity_id"] = entity_id
    body["binding_id"] = binding_id

    try:
        saved = await skill_repo.upsert_binding(binding_id, body)
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("skill_binding_save_failed", exc_info=exc)
        return _err("Failed to save skill binding", 500, "SKILL_BINDING_SAVE_FAILED")

    return _ok(_skill_binding_to_frontend(saved))


@router.delete("/entities/{entity_id}/skills/bindings/{binding_id}")
async def delete_skill_binding(request: Request, entity_id: str, binding_id: str) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    skill_repo = repos["skill_repo"]
    if not skill_repo:
        return _err("Skill repository not configured", 503)

    try:
        deleted = await skill_repo.delete_binding(binding_id)
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("skill_binding_delete_failed", exc_info=exc)
        return _err("Failed to delete skill binding", 500, "SKILL_BINDING_DELETE_FAILED")

    if not deleted:
        return _err(f"Binding not found: {binding_id}", 404)

    return _ok(True)


# ---------------------------------------------------------------------------
# Cache
# ---------------------------------------------------------------------------


@router.get("/cache/metrics")
async def get_cache_metrics(request: Request) -> JSONResponse:
    require_service_identity(request)
    repos = _resolve_repos()
    cache_metrics_repo = repos["cache_metrics_repo"]

    entity_id = request.query_params.get("entity_id")
    hours = int(request.query_params.get("hours", "24"))

    if not cache_metrics_repo:
        return _ok(
            {
                "period_hours": hours,
                "by_cache_type": [],
            }
        )

    try:
        summary = await cache_metrics_repo.get_summary(entity_id=entity_id, hours=hours)
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("cache_metrics_failed", exc_info=exc)
        return _err("Failed to get cache metrics", 500, "CACHE_METRICS_FAILED")

    return _ok(summary)


@router.post("/cache/invalidate")
async def invalidate_cache(request: Request) -> JSONResponse:
    require_service_identity(request)
    body = await request.json()
    entity_id = body.get("entity_id", "")
    cache_type = body.get("cache_type")

    repos = _resolve_repos()
    cache_manager = repos["cache_manager"]

    if not cache_manager:
        return _ok(
            {
                "invalidated": 0,
                "skipped": True,
                "reason": "cache_backend_not_configured",
            }
        )

    try:
        invalidated = await cache_manager.invalidate(
            entity_id=entity_id,
            cache_type=cache_type,
        )
    except Exception as exc:
        logger.error("mcp_cache_invalidate_failed", exc_info=exc)
        return _ok(
            {
                "invalidated": 0,
                "skipped": True,
                "reason": "internal_error",
            }
        )

    return _ok({"invalidated": invalidated, "skipped": False})


# ---------------------------------------------------------------------------
# Frontend <-> DB field mapping helpers
# ---------------------------------------------------------------------------


def _server_to_frontend(row: dict[str, Any]) -> dict[str, Any]:
    """Map DB row to frontend McpServerDefinition shape."""
    risk_policy = row.get("risk_policy", {})
    if isinstance(risk_policy, str):
        try:
            risk_policy = json.loads(risk_policy)
        except (json.JSONDecodeError, TypeError, ValueError):
            risk_policy = {}

    args = row.get("args", [])
    if isinstance(args, str):
        try:
            args = json.loads(args)
        except (json.JSONDecodeError, TypeError, ValueError):
            args = []

    env_secret_refs = row.get("env_secret_refs", [])
    if isinstance(env_secret_refs, str):
        try:
            env_secret_refs = json.loads(env_secret_refs)
        except (json.JSONDecodeError, TypeError, ValueError):
            env_secret_refs = []

    return {
        "id": row.get("server_id", ""),
        "enabled": row.get("status", "draft") == "active",
        "display_name": row.get("display_name", row.get("server_id", "")),
        "description": row.get("description"),
        "transport": row.get("transport", "stdio"),
        "command_ref": row.get("command_ref"),
        "endpoint_url": row.get("endpoint_url"),
        "args": args,
        "env_secret_refs": env_secret_refs,
        "timeout_seconds": row.get("timeout_seconds", 10),
        "startup_timeout_seconds": row.get("startup_timeout_seconds", 5),
        "max_concurrency": row.get("max_concurrency", 4),
        "risk": {
            "network_access": risk_policy.get("network_access", False),
            "filesystem_access": risk_policy.get("filesystem_access", "none"),
            "max_response_bytes": risk_policy.get("max_response_bytes", 1048576),
        },
    }


def _server_from_frontend(body: dict[str, Any]) -> dict[str, Any]:
    """Map frontend McpServerDefinition to DB row shape."""
    risk = body.get("risk", {})
    return {
        "server_id": body.get("id", ""),
        "display_name": body.get("display_name", body.get("id", "")),
        "description": body.get("description"),
        "transport": body.get("transport", "stdio"),
        "command_ref": body.get("command_ref"),
        "endpoint_url": body.get("endpoint_url"),
        "args": body.get("args", []),
        "env_secret_refs": body.get("env_secret_refs", []),
        "risk_policy": {
            "network_access": risk.get("network_access", False),
            "filesystem_access": risk.get("filesystem_access", "none"),
            "max_response_bytes": risk.get("max_response_bytes", 1048576),
        },
        "timeout_seconds": body.get("timeout_seconds", 10),
        "startup_timeout_seconds": body.get("startup_timeout_seconds", 5),
        "max_concurrency": body.get("max_concurrency", 4),
        "status": "active" if body.get("enabled", False) else "draft",
    }


def _binding_to_frontend(row: dict[str, Any]) -> dict[str, Any]:
    """Map DB row to frontend McpEntityBinding shape."""
    return {
        "binding_id": row.get("binding_id", ""),
        "entity_id": row.get("entity_id", ""),
        "server_id": row.get("server_id", ""),
        "enabled": row.get("enabled", False),
        "allowed_tools": parse_json_field(row.get("allowed_tools"), []),
        "denied_tools": parse_json_field(row.get("denied_tools"), []),
        "allowed_resources": parse_json_field(row.get("allowed_resources"), []),
        "allowed_prompts": parse_json_field(row.get("allowed_prompts"), []),
        "tool_defaults": parse_json_field(row.get("tool_defaults"), {}),
    }


def _skill_to_frontend(row: dict[str, Any]) -> dict[str, Any]:
    """Map DB row to frontend SkillRegistryEntry shape."""
    frontmatter = row.get("frontmatter", {})
    if isinstance(frontmatter, str):
        try:
            frontmatter = json.loads(frontmatter)
        except (json.JSONDecodeError, TypeError, ValueError):
            frontmatter = {}

    return {
        "skill_slug": row.get("skill_slug", ""),
        "version": row.get("version", "1.0.0"),
        "name": row.get("name", row.get("skill_slug", "")),
        "description": row.get("description"),
        "source": row.get("source", "local"),
        "canonical_path": row.get("canonical_path", ""),
        "entry_file": row.get("entry_file", ""),
        "frontmatter": frontmatter,
        "content_hash": row.get("content_hash", ""),
        "status": row.get("status", "active"),
        "indexed_at": str(row.get("indexed_at", "")),
    }


def _skill_binding_to_frontend(row: dict[str, Any]) -> dict[str, Any]:
    """Map DB row to frontend SkillEntityBinding shape."""
    return {
        "binding_id": row.get("binding_id", ""),
        "entity_id": row.get("entity_id", ""),
        "skill_slug": row.get("skill_slug", ""),
        "version": row.get("version", "1.0.0"),
        "enabled": row.get("enabled", False),
        "priority": row.get("priority", 0),
        "activation_policy": row.get("activation_policy", "always"),
        "allowed_task_types": parse_json_field(row.get("allowed_task_types"), []),
        "allowed_reference_paths": parse_json_field(row.get("allowed_reference_paths"), []),
        "allow_scripts": row.get("allow_scripts", False),
        "max_instruction_tokens": row.get("max_instruction_tokens", 4096),
    }
