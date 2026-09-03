"""asyncpg 原生 AI 配置存储。

提供一个轻量、无第三方查询构建器依赖的 ``AIConfigStoreInterface`` 实现，直接使用
``asyncpg`` 连接池读写 ``ai_entities`` 表（``config`` JSONB + ``config_revision``）。

设计要点：

* **只读 / 写入策略来源**：``ai_entities.config`` 是实体策略的事实来源；本存储不触碰
  独立资产表（MCP server / skill registry / model catalog / cache metrics），那些由
  各自的 repo 负责。
* **加密边界**：API Key 通过 :class:`ConfigEncryptor` 加解密，明文绝不落库；解密结果
  绝不携带 ``_key_*`` 内部元字段。
* **revision 语义**：``get`` 会把 ``config_revision`` 注入 ``config['_config_revision']``，
  供 ``CapabilityResolver`` 生成每轮快照缓存键；``update`` 会自增 ``config_revision``，
  使下游缓存自然失效。
* **schema 归属**：表结构由 ``migrations/*.sql`` 负责，本存储不执行任何 DDL；仅在首次
  访问时尽力（best-effort）幂等播种默认实体（``INSERT ... ON CONFLICT DO NOTHING``）。
* **JSONB 解码**：asyncpg 默认把 JSONB 以字符串返回，统一用 ``json.loads`` 兼容
  dict / str 两种情况。
"""

from __future__ import annotations

import asyncio
import json
from typing import Any

from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

from .config.ai_config_crypto import ConfigEncryptor, get_config_encryptor
from .config.cache_mixin import ConfigCacheMixin
from .config.config_normalizer import normalize_config
from .config_store import (
    AIConfigStoreInterface,
    build_seed_entity_configs,
)

logger = get_logger(__name__)


def _coerce_config(raw: Any) -> dict[str, Any]:
    """Normalize a JSONB column value (dict or JSON string) into a dict.

    Raises:
        ValueError: If the raw value is a string/bytes that cannot be parsed as JSON,
            or if the parsed JSON is not a dict. This indicates data corruption in the
            ai_entities.config column and should not be silently swallowed.
    """
    if raw is None:
        return {}
    if isinstance(raw, dict):
        return dict(raw)
    if isinstance(raw, (str, bytes, bytearray)):
        try:
            parsed = json.loads(raw)
        except (json.JSONDecodeError, TypeError, ValueError) as exc:
            raise ValueError(f"Corrupted JSONB in ai_entities.config: {exc}") from exc
        if not isinstance(parsed, dict):
            raise ValueError(f"ai_entities.config decoded to {type(parsed).__name__}, expected dict")
        return dict(parsed)
    raise ValueError(f"Unexpected config column type: {type(raw).__name__}")


class AsyncpgAIConfigStore(AIConfigStoreInterface, ConfigCacheMixin):
    """基于 asyncpg 连接池的 AI 配置存储实现。"""

    def __init__(
        self,
        pool: Any,
        *,
        encryptor: ConfigEncryptor | None = None,
        seed_on_start: bool = True,
    ) -> None:
        """初始化 asyncpg 配置存储。

        Args:
            pool: asyncpg 连接池（需支持 ``async with pool.acquire() as conn``）。
            encryptor: 可选的注入式加密器；缺省使用进程级共享实例。
            seed_on_start: 是否在首次访问时幂等播种默认实体。
        """
        self._pool = pool
        self._encryptor = encryptor or get_config_encryptor()
        self._seed_on_start = seed_on_start
        self._seeded = False
        self._init_cache()
        self._lock = asyncio.Lock()
        logger.info("AsyncpgAIConfigStore initialized")

    # ------------------------------------------------------------------
    # Seeding (best-effort, idempotent, no DDL)
    # ------------------------------------------------------------------
    async def _ensure_seeded(self) -> None:
        if self._seeded or not self._seed_on_start:
            self._seeded = True
            return
        async with self._lock:
            if self._seeded:
                return
            try:
                async with self._pool.acquire() as conn:
                    for entity_id, seed_config in build_seed_entity_configs().items():
                        encrypted = self._encryptor.encrypt_config(seed_config)
                        await conn.execute(
                            """
                            INSERT INTO ai_entities (id, config)
                            VALUES ($1, $2::jsonb)
                            ON CONFLICT (id) DO NOTHING
                            """,
                            entity_id,
                            json.dumps(encrypted),
                        )
            except POSTGRES_EXCEPTIONS as exc:  # pragma: no cover - depends on live DB
                # Seeding is best-effort; schema is owned by migrations. A failure here
                # (e.g. table absent in a half-migrated DB) must not break reads.
                logger.warning("AsyncpgAIConfigStore seed skipped: %s", exc)
            finally:
                self._seeded = True

    # ------------------------------------------------------------------
    # AIConfigStoreInterface
    # ------------------------------------------------------------------
    async def get_all(self) -> dict[str, dict[str, Any]]:
        await self._ensure_seeded()
        try:
            async with self._pool.acquire() as conn:
                rows = await conn.fetch("SELECT id, config, config_revision FROM ai_entities WHERE deleted_at IS NULL")
        except POSTGRES_EXCEPTIONS as exc:
            logger.error("AsyncpgAIConfigStore.get_all failed: %s", exc)
            raise

        result: dict[str, dict[str, Any]] = {}
        for row in rows:
            config = _coerce_config(row["config"])
            config["_config_revision"] = row["config_revision"]
            result[row["id"]] = normalize_config(self._encryptor.decrypt_config(config))
        return result

    async def get(self, entity_id: str) -> dict[str, Any] | None:
        await self._ensure_seeded()

        if self._cache_valid(entity_id):
            return self._get_cached(entity_id)

        try:
            async with self._pool.acquire() as conn:
                row = await conn.fetchrow(
                    "SELECT config, config_revision FROM ai_entities WHERE id = $1 AND deleted_at IS NULL",
                    entity_id,
                )
        except POSTGRES_EXCEPTIONS as exc:
            logger.error("AsyncpgAIConfigStore.get('%s') failed: %s", entity_id, exc)
            raise

        if not row:
            return None

        config = _coerce_config(row["config"])
        config["_config_revision"] = row["config_revision"]
        decrypted = normalize_config(self._encryptor.decrypt_config(config))
        self._set_cached(entity_id, decrypted)
        return decrypted

    async def reload(self) -> None:
        """Clear the in-memory read cache; DB remains the source of truth."""
        self._invalidate_cache()


__all__ = ["AsyncpgAIConfigStore"]
