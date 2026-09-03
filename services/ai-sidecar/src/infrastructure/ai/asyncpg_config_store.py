"""asyncpg 原生 AI 配置存储。

提供一个轻量、无第三方查询构建器依赖的 ``AIConfigStoreInterface`` 实现，直接使用
``asyncpg`` 连接池读取 ``ai_entities`` 表（``config`` JSONB + ``config_revision``）。

设计要点：

* **只读 / 写入策略来源**：``ai_entities.config`` 是实体策略的事实来源；本存储不触碰
  独立资产表（MCP server / skill registry / model catalog / cache metrics），那些由
  各自的 repo 负责。写路径（含默认与 pilot 实体播种）由 Rust api-server 统一持有
  （ADR-0004），本存储不写 ``ai_entities``。
* **加密边界**：API Key 通过 :class:`ConfigEncryptor` 解密；解密结果
  绝不携带 ``_key_*`` 内部元字段。Rust 写入的 ``fernet_v1`` 密文与本组件格式一致。
* **revision 语义**：``get`` 会把 ``config_revision`` 注入 ``config['_config_revision']``，
  供 ``CapabilityResolver`` 生成每轮快照缓存键。
* **schema 归属**：表结构与种子数据均由 Rust 侧 / ``migrations/*.sql`` 负责，本存储
  不执行任何 DDL 或写入。
* **JSONB 解码**：asyncpg 默认把 JSONB 以字符串返回，统一用 ``json.loads`` 兼容
  dict / str 两种情况。
"""

from __future__ import annotations

import json
from typing import Any

from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

from .config.ai_config_crypto import ConfigEncryptor, get_config_encryptor
from .config.cache_mixin import ConfigCacheMixin
from .config.config_normalizer import normalize_config
from .config_store import AIConfigStoreInterface

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
    ) -> None:
        """初始化 asyncpg 配置存储。

        Args:
            pool: asyncpg 连接池（需支持 ``async with pool.acquire() as conn``）。
            encryptor: 可选的注入式加密器；缺省使用进程级共享实例。
        """
        self._pool = pool
        self._encryptor = encryptor or get_config_encryptor()
        self._init_cache()
        logger.info("AsyncpgAIConfigStore initialized")

    # ------------------------------------------------------------------
    # AIConfigStoreInterface
    # ------------------------------------------------------------------
    async def get_all(self) -> dict[str, dict[str, Any]]:
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
