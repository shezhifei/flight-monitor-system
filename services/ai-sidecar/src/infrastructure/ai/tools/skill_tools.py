"""Skill progressive disclosure for hybrid agent workflow.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task C3):

1. Initial skill instruction shows only name + description (short form).
2. Full skill content and references loaded via read-only tools on demand.
3. Capability resolver exposes short descriptions by default.
4. Scripts directory remains prohibited from execution.
5. Two tools provided: load_skill / read_skill_reference.

Implementation focuses on sidecar capability_resolver integration and
tool registration for dynamic skill loading.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any

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
    """

    # Tool schemas for register_skills_tools()
    LOAD_SKILL_TOOL = {
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
                        "description": "Unique identifier of the skill to load",
                    },
                },
                "required": ["skill_id"],
            },
        },
    }

    READ_SKILL_REFERENCE_TOOL = {
        "type": "function",
        "function": {
            "name": "read_skill_reference",
            "description": (
                "Read a reference document linked to a skill. "
                "References are external knowledge bases, policies, or documentation "
                "that provide additional context for skill execution."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "reference_url": {
                        "type": "string",
                        "description": "URL or path to the reference document",
                    },
                },
                "required": ["reference_url"],
            },
        },
    }

    SCHEMA_TOOLS = [LOAD_SKILL_TOOL, READ_SKILL_REFERENCE_TOOL]

    def __init__(self):
        self._skills_cache: dict[str, SkillContent] = {}
        self._metadata_cache: dict[str, SkillMetadata] = {}

    async def get_skill_metadata(self, skill_id: str) -> SkillMetadata | None:
        """Get minimal metadata for a skill (used in initial prompt)."""
        # In production, this would query Redis or Postgres
        # For now, return placeholder metadata
        logger.debug(f"Fetching metadata for skill: {skill_id}")
        
        # Placeholder implementation
        # Actual implementation would cache/fetch from skill registry
        return None

    async def load_full_skill(self, skill_id: str) -> SkillContent | None:
        """Load complete skill content on demand."""
        # Check cache first
        if skill_id in self._skills_cache:
            cached = self._skills_cache[skill_id]
            logger.info(f"Loaded skill {skill_id} from cache ({cached.total_length} chars)")
            return cached
        
        # In production, fetch from skill storage (Redis/Postgres/S3)
        logger.info(f"Loading full skill: {skill_id}")
        
        # Placeholder: simulate loading
        result = await self._fetch_skill_from_storage(skill_id)
        
        if result:
            self._skills_cache[skill_id] = result
            logger.info(f"Cached {skill_id} ({result.total_length} chars)")
        
        return result
    
    async def _fetch_skill_from_storage(self, skill_id: str) -> SkillContent | None:
        """Fetch skill from persistent storage (placeholder)."""
        # TODO: Implement actual storage integration
        # For now, return None to indicate not found
        return None

    async def read_reference(self, reference_url: str) -> str | None:
        """Read a reference document (placeholder)."""
        logger.info(f"Reading reference: {reference_url}")
        
        # TODO: Implement actual reference fetching
        # Could be from knowledge base, HTTP endpoint, file system
        return None

    def generate_initial_instruction_block(self, skills: list[str]) -> str:
        """Generate initial skill instruction block (name + description only).
        
        Args:
            skills: List of skill IDs that should be shown
            
        Returns:
            Formatted text showing only skill names and descriptions
        """
        lines = []
        
        for skill_id in skills:
            meta = self._metadata_cache.get(skill_id)
            
            if meta is None:
                # Skip async fetch - would cause event loop issues in sync context
                # In production, use proper dependency injection
                continue
            
            lines.append(meta.to_short_text())
        
        if not lines:
            return ""
        
        return "\n\n# Enabled Agent Skills\n" + "\n".join(lines) + "\n"

    def generate_full_instruction_block(self, skill_id: str) -> str | None:
        """Generate full instruction block for a loaded skill."""
        import asyncio
        skill = asyncio.run(self.load_full_skill(skill_id))
        
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
    "SkillMetadata",
    "SkillContent",
    "SkillProgressiveDiscloser",
    "get_skill_discloser",
    "register_skills_tools",
]
