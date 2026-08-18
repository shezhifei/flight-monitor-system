"""Templates must not advertise tools that are not registered (Task F6).

After Task F5 the templates name ontology tools in their system prompt
addenda. This guard rejects any template that promises a tool the runtime
cannot actually offer: every backticked dotted tool name and every
``ontology.*`` mention must resolve to a tool in the builtin catalog or the
plan-board tool set (plan/skill tools are the only non-catalog tool faces).
"""

from __future__ import annotations

import re

from src.infrastructure.ai.ai_runtime_bootstrap import _builtin_tool_catalog
from src.infrastructure.ai.templates import (
    ANOMALY_OPS_TEMPLATE,
    DISPATCH_OPS_TEMPLATE,
    QUERY_OPS_TEMPLATE,
)
from src.infrastructure.ai.tools.plan_tools import PLAN_TOOL_NAMES

# Backticked dotted lowercase names, e.g. `flights.lookup` or `ontology.lookup`.
_BACKTICKED_DOTTED_RE = re.compile(r"`([a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)+)`")
# Bare ``ontology.<action>`` mentions anywhere in the addendum.
_ONTOLOGY_MENTION_RE = re.compile(r"\bontology\.([a-z][a-z0-9_]*)")

_TEMPLATES = (QUERY_OPS_TEMPLATE, ANOMALY_OPS_TEMPLATE, DISPATCH_OPS_TEMPLATE)


def _registered_tool_names() -> set[str]:
    names = {entry["name"] for entry in _builtin_tool_catalog()}
    names |= set(PLAN_TOOL_NAMES)
    return names


def _advertised_tool_names(addendum: str) -> set[str]:
    names = set(_BACKTICKED_DOTTED_RE.findall(addendum))
    names |= {f"ontology.{match}" for match in _ONTOLOGY_MENTION_RE.findall(addendum)}
    return names


def test_builtin_catalog_contains_the_ontology_tools() -> None:
    names = _registered_tool_names()
    assert {"ontology.lookup", "ontology.explain_constraints", "ontology.propose_action"} <= names


def test_templates_only_advertise_registered_tools() -> None:
    registered = _registered_tool_names()
    violations: list[str] = []
    for template in _TEMPLATES:
        for name in sorted(_advertised_tool_names(template.system_prompt_addendum)):
            if name not in registered:
                violations.append(f"{template.task_type}: {name}")
    assert not violations, (
        "Templates advertise tools that are not registered in the builtin "
        f"catalog or the plan-board tool set: {violations}"
    )


def test_templates_actually_mention_ontology_tools() -> None:
    """The guard is only meaningful if the addenda really name ontology tools."""
    assert _advertised_tool_names(QUERY_OPS_TEMPLATE.system_prompt_addendum) >= {
        "ontology.lookup",
        "ontology.explain_constraints",
    }
    assert "ontology.propose_action" in _advertised_tool_names(
        ANOMALY_OPS_TEMPLATE.system_prompt_addendum
    )
    assert _advertised_tool_names(DISPATCH_OPS_TEMPLATE.system_prompt_addendum) >= {
        "ontology.lookup",
        "ontology.explain_constraints",
        "ontology.propose_action",
    }


def test_scanner_catches_unregistered_names() -> None:
    """Self-check: the scanner must flag a fabricated dotted tool name."""
    fake = "Prefer `flights.lookup` and ontology.teleport for this."
    assert _advertised_tool_names(fake) == {"flights.lookup", "ontology.teleport"}
    registered = _registered_tool_names()
    assert "flights.lookup" not in registered
    assert "ontology.teleport" not in registered
