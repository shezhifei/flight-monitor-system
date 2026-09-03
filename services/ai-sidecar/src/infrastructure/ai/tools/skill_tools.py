"""Skill progressive disclosure for hybrid agent workflow.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task C3):

1. Initial skill instruction shows only name + description (short form).
2. Full skill content and references loaded via read-only tools on demand.
3. Capability resolver exposes short descriptions by default.
4. Scripts directory remains prohibited from execution AND reading.
5. Two tools provided: load_skill / read_skill_reference.

Skill content is fetched through the real storage path
(:class:`SkillLoader` over allowlisted roots), resolved lazily from the AI
container. All tool surfaces are read-only: no scripts/ execution, no writes.
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from src.infrastructure.common.exceptions import IO_EXCEPTIONS

logger = logging.getLogger(__name__)


@dataclass
class SkillMetadata:
    """Minimal skill metadata shown in initial prompt."""

    skill_id: str
    name: str
    description: str
    category: str = "general"
    tags: list[str] = field(default_factory=list)
    version: str = "1.0"

    def to_short_text(self) -> str:
        """Return short text representation (name + description)."""
        return f"- {self.name}: {self.description}"


@dataclass
class SkillContent:
    """Full skill content with instructions and references."""

    skill_id: str
    full_instructions: str
    references: list[str] = field(default_factory=list)
    examples: list[dict[str, Any]] = field(default_factory=list)
    constraints: list[str] = field(default_factory=list)

    @property
    def total_length(self) -> int:
        """Total character length of full content."""
        content = self.full_instructions + "\n".join(self.references)
        return len(content)


class SkillProgressiveDiscloser:
    """Manages progressive disclosure of skill information.

    Provides two modes:
    - Short mode: Name + description only (for initial prompt)
    - Full mode: Complete instructions + references (loaded on demand)

    Storage path: delegates to :class:`SkillLoader` (allowlisted roots,
    SKILL.md frontmatter parsing, content hashing). The loader is either
    injected or resolved lazily from the AI container.
    """

    # Tool schemas for register_skills_tools()
    LOAD_SKILL_TOOL = {  # noqa: RUF012
        "type": "function",
        "function": {
            "name": "load_skill",
            "description": (
                "Load full content of a specific skill by its ID. "
                "Returns complete instructions, examples, and references. "
                "Only available if the skill is enabled in current entity config."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "skill_id": {
                        "type": "string",
                        "description": "Unique identifier (slug) of the skill to load",
                    },
                },
                "required": ["skill_id"],
            },
        },
    }

    READ_SKILL_REFERENCE_TOOL = {  # noqa: RUF012
        "type": "function",
        "function": {
            "name": "read_skill_reference",
            "description": (
                "Read a reference document linked to a skill. "
                "References are external knowledge bases, policies, or documentation "
                "that provide additional context for skill execution. "
                "Read-only; scripts/ paths are always rejected."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "skill_id": {
                        "type": "string",
                        "description": "Unique identifier (slug) of the owning skill",
                    },
                    "reference_path": {
                        "type": "string",
                        "description": "Reference file path relative to the skill directory",
                    },
                },
                "required": ["skill_id", "reference_path"],
            },
        },
    }

    SCHEMA_TOOLS = [LOAD_SKILL_TOOL, READ_SKILL_REFERENCE_TOOL]  # noqa: RUF012

    def __init__(self, skill_loader: Any | None = None):
        self._skill_loader = skill_loader
        self._skills_cache: dict[str, SkillContent] = {}
        self._metadata_cache: dict[str, SkillMetadata] = {}

    def _get_loader(self) -> Any | None:
        """Return the skill loader, lazily resolving from the AI container."""
        if self._skill_loader is None:
            try:
                from src.infrastructure.ai.ai_container import resolve_skill_loader

                self._skill_loader = resolve_skill_loader()
            except Exception as exc:  # noqa: BLE001 - container may be unbootstrapped in tests
                logger.debug(f"Skill loader resolution failed: {exc}")
                self._skill_loader = None
        return self._skill_loader

    async def get_skill_metadata(self, skill_id: str) -> SkillMetadata | None:
        """Get minimal metadata for a skill (used in initial prompt)."""
        if skill_id in self._metadata_cache:
            return self._metadata_cache[skill_id]

        loader = self._get_loader()
        if loader is None:
            logger.debug(f"No skill loader available; cannot fetch metadata for {skill_id}")
            return None

        skill = await loader.load_skill(skill_id)
        if skill is None:
            return None

        meta = SkillMetadata(
            skill_id=skill.slug,
            name=skill.name,
            description=skill.description,
            tags=list(skill.frontmatter.get("tags", []) or []),
            version=skill.version,
        )
        self._metadata_cache[skill_id] = meta
        return meta

    async def load_full_skill(self, skill_id: str) -> SkillContent | None:
        """Load complete skill content on demand."""
        # Check cache first
        if skill_id in self._skills_cache:
            cached = self._skills_cache[skill_id]
            logger.info(f"Loaded skill {skill_id} from cache ({cached.total_length} chars)")
            return cached

        logger.info(f"Loading full skill: {skill_id}")
        result = await self._fetch_skill_from_storage(skill_id)

        if result:
            self._skills_cache[skill_id] = result
            logger.info(f"Cached {skill_id} ({result.total_length} chars)")

        return result

    async def _fetch_skill_from_storage(self, skill_id: str) -> SkillContent | None:
        """Fetch skill from the real storage path (SkillLoader over allowlisted roots)."""
        loader = self._get_loader()
        if loader is None:
            return None

        skill = await loader.load_skill(skill_id)
        if skill is None:
            return None

        return SkillContent(
            skill_id=skill.slug,
            full_instructions=skill.content,
            references=sorted(skill.references.keys()),
        )

    async def read_reference(self, skill_id: str, reference_path: str) -> str | None:
        """Read a reference document within a skill directory (read-only).

        Safety constraints:
        - scripts/ paths are always rejected (plan hard constraint: no script execution)
        - path traversal outside the skill directory is rejected
        - absolute paths are rejected
        """
        logger.info(f"Reading reference: skill={skill_id}, path={reference_path}")

        if not reference_path or not reference_path.strip():
            logger.warning("Rejected empty reference path")
            return None

        parts = [p for p in re.split(r"[\\/]+", reference_path.strip()) if p]
        if not parts or any(p in (".", "..") for p in parts):
            logger.warning(f"Rejected reference path with traversal segments: {reference_path}")
            return None
        if any(p.lower() == "scripts" for p in parts):
            logger.warning(f"Rejected scripts/ reference path: {reference_path}")
            return None
        if Path(reference_path).is_absolute():
            logger.warning(f"Rejected absolute reference path: {reference_path}")
            return None

        loader = self._get_loader()
        if loader is None:
            return None

        skill = await loader.load_skill(skill_id)
        if skill is None:
            return None

        skill_dir = Path(skill.canonical_path).resolve()
        ref_file = (skill_dir / reference_path).resolve()

        # Defense in depth: resolved path must stay inside the skill directory
        # (the loader already validated the skill dir against the allowlist).
        try:
            ref_file.relative_to(skill_dir)
        except ValueError:
            logger.warning(f"Rejected reference outside skill directory: {reference_path}")
            return None

        if not ref_file.is_file():
            logger.warning(f"Reference not found: {ref_file}")
            return None

        try:
            return ref_file.read_text(encoding="utf-8")
        except IO_EXCEPTIONS as exc:
            logger.warning(f"Failed to read reference {reference_path}: {exc}")
            return None

    async def execute_tool(self, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        """Read-only execution entrypoint for the skill tools.

        Routing here keeps tool invocation off ToolExecutor write paths;
        both tools only ever read from allowlisted skill roots.
        """
        if tool_name == "load_skill":
            skill_id = str(arguments.get("skill_id", "") or "")
            if not skill_id:
                return {"success": False, "error": "skill_id is required"}
            content = await self.load_full_skill(skill_id)
            if content is None:
                return {"success": False, "error": f"SKILL_NOT_FOUND: {skill_id}"}
            return {
                "success": True,
                "skill_id": content.skill_id,
                "content": content.full_instructions,
                "references": content.references,
            }
        if tool_name == "read_skill_reference":
            skill_id = str(arguments.get("skill_id", "") or "")
            reference_path = str(arguments.get("reference_path", "") or "")
            text = await self.read_reference(skill_id, reference_path)
            if text is None:
                return {
                    "success": False,
                    "error": f"SKILL_REFERENCE_UNAVAILABLE: {skill_id}:{reference_path}",
                }
            return {"success": True, "skill_id": skill_id, "reference_path": reference_path, "content": text}
        return {"success": False, "error": f"UNKNOWN_SKILL_TOOL: {tool_name}"}

    def generate_initial_instruction_block(self, skills: list[str]) -> str:
        """Generate initial skill instruction block (name + description only).

        Only skills whose metadata is already cached are included; use
        :meth:`agenerate_initial_instruction_block` to fetch metadata first.

        Args:
            skills: List of skill IDs that should be shown

        Returns:
            Formatted text showing only skill names and descriptions
        """
        lines = []

        for skill_id in skills:
            meta = self._metadata_cache.get(skill_id)

            if meta is None:
                continue

            lines.append(meta.to_short_text())

        if not lines:
            return ""

        return "\n\n# Enabled Agent Skills\n" + "\n".join(lines) + "\n"

    async def agenerate_initial_instruction_block(self, skills: list[str]) -> str:
        """Async variant: fetch metadata for each skill, then build the short block."""
        for skill_id in skills:
            await self.get_skill_metadata(skill_id)
        return self.generate_initial_instruction_block(skills)

    async def generate_full_instruction_block(self, skill_id: str) -> str | None:
        """Generate full instruction block for a loaded skill."""
        skill = await self.load_full_skill(skill_id)

        if not skill:
            return None

        parts = [f"# Skill: {skill_id}\n"]
        parts.append(skill.full_instructions)

        if skill.references:
            parts.append("\n## References\n")
            parts.extend([f"- {ref}" for ref in skill.references])

        if skill.examples:
            parts.append("\n## Examples\n")
            for ex in skill.examples:
                parts.append(f"\n### {ex.get('title', 'Example')}\n")
                parts.append(ex.get("content", ""))

        if skill.constraints:
            parts.append("\n## Constraints\n")
            parts.extend([f"- {c}" for c in skill.constraints])

        return "\n".join(parts)


SKILL_TOOL_NAMES = frozenset({"load_skill", "read_skill_reference"})


def get_skill_discloser() -> SkillProgressiveDiscloser:
    """Get singleton instance of SkillProgressiveDiscloser."""
    from src.infrastructure.ai.tool_registry import get_tool_executor

    executor = get_tool_executor()
    if not hasattr(executor, "_skill_discloser"):
        executor._skill_discloser = SkillProgressiveDiscloser()
    return executor._skill_discloser


async def register_skills_tools(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Register skill tools into global tool schema list."""
    tools.extend(SkillProgressiveDiscloser.SCHEMA_TOOLS)
    return tools


__all__ = [
    "SKILL_TOOL_NAMES",
    "SkillContent",
    "SkillMetadata",
    "SkillProgressiveDiscloser",
    "get_skill_discloser",
    "register_skills_tools",
]
