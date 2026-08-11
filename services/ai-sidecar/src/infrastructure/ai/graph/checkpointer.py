"""
Agent 运行时自定义持久化 (Checkpointer) 层。

为 LangGraph 原生 checkpointer 提供业务仓储桥接；当 langgraph 未安装时，仍暴露兼容接口，
供本地最小运行和测试使用。
"""

import json
from collections.abc import AsyncIterator
from dataclasses import dataclass
from typing import Any

try:
    from langgraph.checkpoint.base import (
        BaseCheckpointSaver,
        Checkpoint,
        CheckpointMetadata,
        CheckpointTuple,
    )
    from langgraph.checkpoint.serde.jsonplus import JsonPlusSerializer
except ImportError:  # pragma: no cover - fallback exercised by tests
    Checkpoint = dict[str, Any]
    CheckpointMetadata = dict[str, Any]

    class JsonPlusSerializer:
        def dumps(self, value: Any) -> bytes:
            return json.dumps(value, ensure_ascii=False, default=str).encode("utf-8")

        def loads(self, value: bytes) -> Any:
            return json.loads(value.decode("utf-8"))

    class BaseCheckpointSaver:
        def __init__(self, serde: Any | None = None):
            self.serde = serde or JsonPlusSerializer()

    @dataclass
    class CheckpointTuple:
        config: dict[str, Any]
        checkpoint: Checkpoint
        metadata: CheckpointMetadata
        parent_config: dict[str, Any] | None = None


from src.infrastructure.ai.agent_execution_repository import AgentExecutionRepository
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class AgentExecutionCheckpointer(BaseCheckpointSaver):
    """将 graph checkpoint 持久化到 AgentExecution.metadata。"""

    def __init__(self, execution_repo: AgentExecutionRepository):
        super().__init__(serde=JsonPlusSerializer())
        self._repo = execution_repo

    async def aget_tuple(self, config: dict[str, Any]) -> CheckpointTuple | None:
        thread_id = (config.get("configurable") or {}).get("thread_id")
        if not thread_id:
            return None

        execution = await self._repo.get_execution(thread_id)
        if not execution:
            return None

        metadata = execution.metadata or {}
        checkpoint_data = metadata.get("langgraph_checkpoint")
        checkpoint_metadata = metadata.get("langgraph_checkpoint_metadata")
        if not checkpoint_data:
            return None

        checkpoint = self.serde.loads(str(checkpoint_data).encode("utf-8"))
        meta = self.serde.loads(str(checkpoint_metadata).encode("utf-8")) if checkpoint_metadata else {}
        return CheckpointTuple(
            config=config,
            checkpoint=checkpoint,
            metadata=meta,
            parent_config=None,
        )

    async def aput(
        self,
        config: dict[str, Any],
        checkpoint: Checkpoint,
        metadata: CheckpointMetadata,
        new_versions: dict[str, str | float | int],
    ) -> dict[str, Any]:
        thread_id = (config.get("configurable") or {}).get("thread_id")
        if not thread_id:
            logger.warning("AgentExecutionCheckpointer.aput missing thread_id in config")
            return config

        execution = await self._repo.get_execution(thread_id)
        if execution is None:
            logger.warning(
                "AgentExecutionCheckpointer: Thread %s not found in DB to save checkpoint",
                thread_id,
            )
            return config

        execution_metadata = execution.metadata or {}
        execution_metadata["langgraph_checkpoint"] = self.serde.dumps(checkpoint).decode("utf-8")
        execution_metadata["langgraph_checkpoint_metadata"] = self.serde.dumps(metadata).decode("utf-8")
        execution.metadata = execution_metadata
        await self._repo.update_execution(execution)

        return {
            "configurable": {
                "thread_id": thread_id,
                "checkpoint_ns": (config.get("configurable") or {}).get("checkpoint_ns", ""),
                "checkpoint_id": checkpoint.get("id") if isinstance(checkpoint, dict) else None,
            }
        }

    async def alist(
        self,
        config: dict[str, Any] | None,
        *,
        filter: dict[str, Any] | None = None,
        before: dict[str, Any] | None = None,
        limit: int | None = None,
    ) -> AsyncIterator[CheckpointTuple]:
        if not config:
            return

        latest = await self.aget_tuple(config)
        if latest is not None:
            yield latest
