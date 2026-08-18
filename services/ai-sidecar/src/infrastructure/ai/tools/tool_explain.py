"""Explain entity × tool governance decisions without executing the tool.

Pure observability helper used by ``GET /internal/ai/v1/tools/explain``.
Re-runs the same resolution surfaces (snapshot membership, ACL, task
template, lease requirement) that the streaming loop would apply, and
returns a machine-readable chain. Does not change allow/deny semantics
and does not touch business tables.
"""

from __future__ import annotations

from typing import Any

from src.infrastructure.ai.capability_resolver import is_tool_allowed
from src.infrastructure.ai.governance.governance_resolver import is_public_l0_tool
from src.infrastructure.ai.templates import get_task_template, template_allows_tool
from src.infrastructure.ai.tools.ontology_tool_definitions import is_ontology_tool
from src.infrastructure.ai.tools.tool_executor import (
    BLOCKED_BY_ACL,
    BLOCKED_BY_SNAPSHOT,
    BLOCKED_BY_TEMPLATE,
    is_write_action_tool,
)


def explain_tool_access(
    *,
    entity_id: str,
    tool_name: str,
    snapshot_tools: list[Any] | None = None,
    tooling_config: dict[str, Any] | None = None,
    task_type: str | None = None,
) -> dict[str, Any]:
    """Build a full governance decision chain for ``entity × tool``.

    Args:
        entity_id: Entity whose snapshot is being inspected.
        tool_name: Tool name as the model would call it.
        snapshot_tools: Resolved snapshot tools (``ResolvedToolConfig`` or
            dicts with at least ``name``; optional ``category``).
        tooling_config: Raw entity ``tooling`` config used for ACL
            re-check (``denied_tools`` / ``allowed_tools`` /
            ``allowed_tool_categories``). Optional — when omitted, ACL
            step only reports snapshot membership.
        task_type: Optional envelope task type for template narrowing.

    Returns:
        Dict with ``entity_id``, ``tool``, ``checks`` (ordered steps),
        ``decision`` (``allow``|``deny``), and when denied ``blocked_by``,
        ``rule``, ``detail``.
    """
    name = (tool_name or "").strip()
    tools = list(snapshot_tools or [])
    tooling = dict(tooling_config or {})
    checks: list[dict[str, Any]] = []

    matched = _find_tool(tools, name)
    in_snapshot = matched is not None
    checks.append(
        {
            "step": "snapshot",
            "result": "present" if in_snapshot else "missing",
            "detail": (
                f"Tool '{name}' is in the resolved entity tool snapshot"
                if in_snapshot
                else f"Tool '{name}' is not in the resolved entity tool snapshot"
            ),
        }
    )

    # ACL: re-check against raw tooling config when available.
    acl_denied = False
    acl_rule: str | None = None
    acl_detail: str | None = None
    if tooling:
        denied_list = tooling.get("denied_tools") or []
        allowed_list = tooling.get("allowed_tools")
        category = _tool_category(matched) if matched is not None else ""
        tool_desc = {"name": name, "category": category}
        if name in denied_list:
            acl_denied = True
            acl_rule = "DENIED_TOOLS"
            acl_detail = f"Tool '{name}' is listed in entity denied_tools"
        elif not is_tool_allowed(tool_desc, tooling):
            acl_denied = True
            if allowed_list is not None and name not in allowed_list:
                acl_rule = "NOT_IN_ALLOWED_TOOLS"
                acl_detail = f"Tool '{name}' is not in entity allowed_tools"
            else:
                acl_rule = "CATEGORY_NOT_ALLOWED"
                acl_detail = f"Tool '{name}' category is not in allowed_tool_categories"
        checks.append(
            {
                "step": "acl",
                "result": "deny" if acl_denied else "allow",
                "rule": acl_rule,
                "detail": acl_detail
                or (
                    f"Tool '{name}' passes entity tooling ACL "
                    f"(denied_tools / allowed_tools / categories)"
                ),
            }
        )
    else:
        checks.append(
            {
                "step": "acl",
                "result": "skipped",
                "detail": "No tooling_config provided; ACL re-check skipped",
            }
        )

    # Template policy (task_type filter).
    template = get_task_template(task_type) if task_type else None
    template_denied = False
    template_rule: str | None = None
    template_detail: str | None = None
    if template is None:
        checks.append(
            {
                "step": "template",
                "result": "skipped",
                "detail": (
                    "No task_type provided or no matching template; "
                    "template narrowing not applied"
                ),
            }
        )
    else:
        category = _tool_category(matched) if matched is not None else None
        if not template_allows_tool(template, tool_name=name, tool_category=category):
            template_denied = True
            if name in template.denied_tools:
                template_rule = "TEMPLATE_DENIED_TOOLS"
                template_detail = (
                    f"Task template '{template.task_type}' denies tool '{name}'"
                )
            else:
                template_rule = "TEMPLATE_CATEGORY_FILTER"
                template_detail = (
                    f"Task template '{template.task_type}' does not allow "
                    f"category '{category or ''}' for tool '{name}'"
                )
        checks.append(
            {
                "step": "template",
                "result": "deny" if template_denied else "allow",
                "task_type": template.task_type,
                "requires_plan_first": bool(getattr(template, "requires_plan_first", False)),
                "rule": template_rule,
                "detail": template_detail
                or (
                    f"Task template '{template.task_type}' allows tool '{name}'"
                ),
            }
        )

    # Lease requirement (classification only — no MQ call).
    public_l0 = is_public_l0_tool(name)
    write_action = is_write_action_tool(name)
    ontology = is_ontology_tool(name)
    lease_required = not public_l0
    checks.append(
        {
            "step": "lease",
            "result": "required" if lease_required else "not_required",
            "public_l0": public_l0,
            "write_action": write_action,
            "ontology": ontology,
            "detail": (
                f"Tool '{name}' is public L0; may execute without Rust lease"
                if public_l0
                else f"Tool '{name}' requires Rust lease authorization (fail-closed without MQ gate)"
            ),
        }
    )

    # Final decision: first hard deny wins, matching the runtime first
    # impression. denied_tools are already filtered out of the snapshot at
    # resolver build, so snapshot membership precedes ACL. Template policy
    # is its own gate (not ACL).
    if not in_snapshot:
        return _deny_payload(
            entity_id=entity_id,
            tool_name=name,
            checks=checks,
            blocked_by=BLOCKED_BY_SNAPSHOT,
            rule="TOOL_NOT_IN_SNAPSHOT",
            detail=f"Tool '{name}' is not present in the resolved entity tool snapshot",
            lease_required=lease_required,
        )
    if acl_denied:
        return _deny_payload(
            entity_id=entity_id,
            tool_name=name,
            checks=checks,
            blocked_by=BLOCKED_BY_ACL,
            rule=acl_rule or "ACL_DENIED",
            detail=acl_detail or f"Tool '{name}' denied by entity ACL",
            lease_required=lease_required,
        )
    if template_denied:
        return _deny_payload(
            entity_id=entity_id,
            tool_name=name,
            checks=checks,
            blocked_by=BLOCKED_BY_TEMPLATE,
            rule=template_rule or "TEMPLATE_DENIED",
            detail=template_detail or f"Tool '{name}' denied by task template",
            lease_required=lease_required,
        )

    return {
        "entity_id": entity_id,
        "tool": name,
        "decision": "allow",
        "blocked_by": None,
        "rule": None,
        "detail": f"Tool '{name}' is allowed by snapshot, ACL, and template policy",
        "lease_required": lease_required,
        "checks": checks,
    }


def explain_from_snapshot(
    *,
    entity_id: str,
    tool_name: str,
    snapshot: Any,
    tooling_config: dict[str, Any] | None = None,
    task_type: str | None = None,
) -> dict[str, Any]:
    """Convenience wrapper that pulls tools off a resolved snapshot object."""
    tools = getattr(snapshot, "tools", None) or []
    return explain_tool_access(
        entity_id=entity_id,
        tool_name=tool_name,
        snapshot_tools=list(tools),
        tooling_config=tooling_config,
        task_type=task_type,
    )


def _find_tool(tools: list[Any], name: str) -> Any | None:
    for tool in tools:
        tool_name = tool.get("name") if isinstance(tool, dict) else getattr(tool, "name", None)
        if tool_name == name:
            return tool
    return None


def _tool_category(tool: Any) -> str:
    if tool is None:
        return ""
    if isinstance(tool, dict):
        return str(tool.get("category") or "")
    return str(getattr(tool, "category", "") or "")


def _deny_payload(
    *,
    entity_id: str,
    tool_name: str,
    checks: list[dict[str, Any]],
    blocked_by: str,
    rule: str,
    detail: str,
    lease_required: bool,
) -> dict[str, Any]:
    return {
        "entity_id": entity_id,
        "tool": tool_name,
        "decision": "deny",
        "blocked_by": blocked_by,
        "rule": rule,
        "detail": detail,
        "lease_required": lease_required,
        "checks": checks,
    }


__all__ = [
    "explain_tool_access",
    "explain_from_snapshot",
]
