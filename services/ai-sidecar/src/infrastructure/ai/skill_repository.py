"""Agent Skill 仓储 - 操作 ai_agent_skill_registry, ai_entity_skill_bindings 表"""

import logging
from abc import ABC, abstractmethod
from typing import Any

logger = logging.getLogger(__name__)


class SkillRepository(ABC):
    """Skill 仓储接口"""

    # === Registry ===
    @abstractmethod
    async def find_all_skills(self, status_filter: str | None = None) -> list[dict[str, Any]]:
        pass

    @abstractmethod
    async def find_skill(self, skill_slug: str, version: str) -> dict[str, Any] | None:
        pass

    @abstractmethod
    async def upsert_skill(self, skill_slug: str, version: str, data: dict[str, Any]) -> dict[str, Any]:
        pass

    @abstractmethod
    async def delete_skill(self, skill_slug: str, version: str) -> bool:
        pass

    # === Bindings ===
    @abstractmethod
    async def find_bindings_by_entity(self, entity_id: str) -> list[dict[str, Any]]:
        pass

    @abstractmethod
    async def upsert_binding(self, binding_id: str, data: dict[str, Any]) -> dict[str, Any]:
        pass

    @abstractmethod
    async def delete_binding(self, binding_id: str) -> bool:
        pass


class PostgresSkillRepository(SkillRepository):
    """PostgreSQL Skill 仓储实现"""

    def __init__(self, pool):
        self._pool = pool

    async def find_all_skills(self, status_filter: str | None = None) -> list[dict[str, Any]]:
        if status_filter:
            query = (
                "SELECT * FROM ai_agent_skill_registry WHERE status = $1 AND deleted_at IS NULL "
                "ORDER BY skill_slug, version"
            )
            async with self._pool.acquire() as conn:
                rows = await conn.fetch(query, status_filter)
        else:
            query = "SELECT * FROM ai_agent_skill_registry WHERE deleted_at IS NULL ORDER BY skill_slug, version"
            async with self._pool.acquire() as conn:
                rows = await conn.fetch(query)
        return [dict(row) for row in rows]

    async def find_skill(self, skill_slug: str, version: str) -> dict[str, Any] | None:
        query = (
            "SELECT * FROM ai_agent_skill_registry "
            "WHERE skill_slug = $1 AND version = $2 AND deleted_at IS NULL"
        )
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(query, skill_slug, version)
            return dict(row) if row else None

    async def upsert_skill(self, skill_slug: str, version: str, data: dict[str, Any]) -> dict[str, Any]:
        import json

        query = """
            INSERT INTO ai_agent_skill_registry (
                skill_slug, version, name, description, source,
                canonical_path, entry_file, frontmatter,
                content_hash, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (skill_slug, version) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                source = EXCLUDED.source,
                canonical_path = EXCLUDED.canonical_path,
                entry_file = EXCLUDED.entry_file,
                frontmatter = EXCLUDED.frontmatter,
                content_hash = EXCLUDED.content_hash,
                status = EXCLUDED.status,
                deleted_at = NULL,
                updated_at = now()
            RETURNING *
        """
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(
                query,
                skill_slug,
                version,
                data.get("name", skill_slug),
                data.get("description"),
                data.get("source", "unknown"),
                data.get("canonical_path", ""),
                data.get("entry_file", "SKILL.md"),
                json.dumps(data.get("frontmatter", {})),
                data.get("content_hash", ""),
                data.get("status", "draft"),
            )
            return dict(row)

    async def delete_skill(self, skill_slug: str, version: str) -> bool:
        # 审计要求软删除：仅标记 deleted_at，行保留
        query = (
            "UPDATE ai_agent_skill_registry SET deleted_at = now(), updated_at = now() "
            "WHERE skill_slug = $1 AND version = $2 AND deleted_at IS NULL"
        )
        async with self._pool.acquire() as conn:
            result = await conn.execute(query, skill_slug, version)
            deleted = result == "UPDATE 1"
            if deleted:
                await _write_soft_delete_audit(
                    conn, "ai_agent_skill", f"{skill_slug}@{version}"
                )
            return deleted

    async def find_bindings_by_entity(self, entity_id: str) -> list[dict[str, Any]]:
        query = """
            SELECT * FROM ai_entity_skill_bindings
            WHERE entity_id = $1 AND deleted_at IS NULL
            ORDER BY priority ASC, skill_slug
        """
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(query, entity_id)
            return [dict(row) for row in rows]

    async def upsert_binding(self, binding_id: str, data: dict[str, Any]) -> dict[str, Any]:
        import json

        query = """
            INSERT INTO ai_entity_skill_bindings (
                binding_id, entity_id, skill_slug, version,
                enabled, priority, activation_policy,
                allowed_task_types, allowed_reference_paths,
                allow_scripts, max_instruction_tokens
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (binding_id) DO UPDATE SET
                entity_id = EXCLUDED.entity_id,
                skill_slug = EXCLUDED.skill_slug,
                version = EXCLUDED.version,
                enabled = EXCLUDED.enabled,
                priority = EXCLUDED.priority,
                activation_policy = EXCLUDED.activation_policy,
                allowed_task_types = EXCLUDED.allowed_task_types,
                allowed_reference_paths = EXCLUDED.allowed_reference_paths,
                allow_scripts = EXCLUDED.allow_scripts,
                max_instruction_tokens = EXCLUDED.max_instruction_tokens,
                deleted_at = NULL,
                updated_at = now()
            RETURNING *
        """
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(
                query,
                binding_id,
                data.get("entity_id"),
                data.get("skill_slug"),
                data.get("version"),
                data.get("enabled", False),
                data.get("priority", 100),
                data.get("activation_policy", "task_routed"),
                json.dumps(data.get("allowed_task_types", [])),
                json.dumps(data.get("allowed_reference_paths", [])),
                data.get("allow_scripts", False),
                data.get("max_instruction_tokens", 3000),
            )
            return dict(row)

    async def delete_binding(self, binding_id: str) -> bool:
        # 审计要求软删除：仅标记 deleted_at，行保留
        query = (
            "UPDATE ai_entity_skill_bindings SET deleted_at = now(), updated_at = now() "
            "WHERE binding_id = $1 AND deleted_at IS NULL"
        )
        async with self._pool.acquire() as conn:
            result = await conn.execute(query, binding_id)
            deleted = result == "UPDATE 1"
            if deleted:
                await _write_soft_delete_audit(conn, "ai_entity_skill_binding", binding_id)
            return deleted


async def _write_soft_delete_audit(conn, entity_type: str, entity_id: str) -> None:
    """写入一条软删除审计记录（best-effort，失败仅记日志不阻断主流程）。"""
    try:
        await conn.execute(
            "INSERT INTO system_audit_logs (entity_type, entity_id, action, changes) "
            "VALUES ($1, $2, $3, $4)",
            entity_type,
            str(entity_id),
            "soft_delete",
            '{"reason": "soft_delete"}',
        )
    except Exception as e:  # noqa: BLE001 - 审计失败不阻断主流程
        logger.warning(
            "写入软删除审计失败 entity_type=%s entity_id=%s: %s", entity_type, entity_id, e
        )
