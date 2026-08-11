"""
共享上下文池（Shared Context Pool）

基于黑板模式（Blackboard Pattern）的多 Agent 信息共享机制。
同一 TODO 树（业务事项）下的多个 Agent 通过该池共享执行结论，
实现上下游 Agent 之间的情报传递，避免重复查询。

设计要点：
- Memory Distillation：写入前提炼高信噪比结论
- Upsert 幂等：同一 source_todo_id 仅保留最新结论
- Token 预算：读取时自动截断，防止 Context Pollution
"""

from __future__ import annotations

import asyncio
from abc import ABC, abstractmethod
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime

from src.domain.utils.time_utils import utc_now
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


# ---------------------------------------------------------------------------
# Data Model
# ---------------------------------------------------------------------------


@dataclass
class ContextEntry:
    """黑板上的一条情报条目。

    Attributes:
        source_todo_id: 产出该结论的 TODO ID（幂等键）
        source_todo_title: 产出该结论的 TODO 标题
        agent_entity_id: 产出该结论的 AI Entity ID
        content_type: 条目类型 ("distilled_conclusion" | "observation" | "error")
        content: 提炼后的高价值内容
        tags: 可供下游按语义检索的标签 (如 ["flight_status", "CA1234"])
        created_at: 创建/更新时间戳
        token_count: 预估 token 数（用于预算控制）
    """

    source_todo_id: str
    source_todo_title: str
    agent_entity_id: str
    content_type: str  # "distilled_conclusion" | "observation" | "error"
    content: str
    tags: list[str] = field(default_factory=list)
    created_at: datetime = field(default_factory=utc_now)
    token_count: int = 0

    def __post_init__(self):
        if self.token_count <= 0 and self.content:
            # 粗略估算：中文约 2 字符/token，英文约 4 字符/token
            self.token_count = max(1, len(self.content) // 2)


# ---------------------------------------------------------------------------
# Abstract Interface
# ---------------------------------------------------------------------------


class SharedContextPool(ABC):
    """黑板模式（Blackboard）的共享上下文池抽象接口。

    每一棵 TODO 树（root_todo_id）拥有一个独立的黑板命名空间。
    """

    @abstractmethod
    async def write_or_update(self, root_todo_id: str, entry: ContextEntry) -> None:
        """写入或更新一条结论。

        以 source_todo_id 为粒度进行 Upsert，保证重试时的幂等性。
        """
        ...

    @abstractmethod
    async def read_for_dependencies(
        self,
        root_todo_id: str,
        dependency_todo_ids: list[str],
        max_tokens: int = 2000,
    ) -> list[ContextEntry]:
        """读取指定依赖 TODO 产出的结论。

        按 token 预算截断，防止 Context Pollution。
        """
        ...

    @abstractmethod
    async def read_by_tags(
        self,
        root_todo_id: str,
        tags: list[str],
        max_tokens: int = 2000,
    ) -> list[ContextEntry]:
        """按语义标签检索黑板上的情报。"""
        ...

    @abstractmethod
    async def read_all(
        self,
        root_todo_id: str,
        max_tokens: int = 4000,
    ) -> list[ContextEntry]:
        """读取指定 root 下的全部条目（受 token 预算约束）。"""
        ...

    @abstractmethod
    async def clear(self, root_todo_id: str) -> None:
        """清空指定 root 下的所有条目。"""
        ...


# ---------------------------------------------------------------------------
# Memory Implementation
# ---------------------------------------------------------------------------


class MemorySharedContextPool(SharedContextPool):
    """进程内内存实现。

    适用于单次 execute_todo_tree 调用的生命周期，
    无需跨进程共享时性能最佳。
    """

    def __init__(self) -> None:
        # root_todo_id -> { source_todo_id -> ContextEntry }
        self._store: dict[str, dict[str, ContextEntry]] = defaultdict(dict)
        self._lock = asyncio.Lock()

    async def write_or_update(self, root_todo_id: str, entry: ContextEntry) -> None:
        async with self._lock:
            bucket = self._store[root_todo_id]
            existing = bucket.get(entry.source_todo_id)
            if existing:
                logger.debug(
                    f"SharedPool upsert: overwriting entry for "
                    f"source_todo={entry.source_todo_id} in root={root_todo_id}"
                )
            bucket[entry.source_todo_id] = entry

    async def read_for_dependencies(
        self,
        root_todo_id: str,
        dependency_todo_ids: list[str],
        max_tokens: int = 2000,
    ) -> list[ContextEntry]:
        async with self._lock:
            bucket = self._store.get(root_todo_id, {})
            dep_set = set(dependency_todo_ids)
            candidates = [entry for todo_id, entry in bucket.items() if todo_id in dep_set]
        return self._apply_token_budget(candidates, max_tokens)

    async def read_by_tags(
        self,
        root_todo_id: str,
        tags: list[str],
        max_tokens: int = 2000,
    ) -> list[ContextEntry]:
        async with self._lock:
            bucket = self._store.get(root_todo_id, {})
            tag_set = set(t.lower() for t in tags)
            candidates = [entry for entry in bucket.values() if tag_set.intersection(t.lower() for t in entry.tags)]
        return self._apply_token_budget(candidates, max_tokens)

    async def read_all(
        self,
        root_todo_id: str,
        max_tokens: int = 4000,
    ) -> list[ContextEntry]:
        async with self._lock:
            bucket = self._store.get(root_todo_id, {})
            candidates = list(bucket.values())
        return self._apply_token_budget(candidates, max_tokens)

    async def clear(self, root_todo_id: str) -> None:
        async with self._lock:
            self._store.pop(root_todo_id, None)

    # ---- helpers ----

    @staticmethod
    def _apply_token_budget(
        entries: list[ContextEntry],
        max_tokens: int,
    ) -> list[ContextEntry]:
        """按创建时间排序，贪心取到 token 预算用完为止。"""
        sorted_entries = sorted(entries, key=lambda e: e.created_at)
        result: list[ContextEntry] = []
        remaining = max_tokens
        for entry in sorted_entries:
            if remaining <= 0:
                break
            if entry.token_count <= remaining:
                result.append(entry)
                remaining -= entry.token_count
            else:
                # 截断内容以适配剩余预算
                ratio = remaining / max(entry.token_count, 1)
                truncated_len = max(1, int(len(entry.content) * ratio))
                truncated_entry = ContextEntry(
                    source_todo_id=entry.source_todo_id,
                    source_todo_title=entry.source_todo_title,
                    agent_entity_id=entry.agent_entity_id,
                    content_type=entry.content_type,
                    content=entry.content[:truncated_len] + "…(已截断)",
                    tags=entry.tags,
                    created_at=entry.created_at,
                    token_count=remaining,
                )
                result.append(truncated_entry)
                remaining = 0
        return result
