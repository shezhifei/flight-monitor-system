"""Data models for the Todo Agent Executor."""

from __future__ import annotations

import time
from dataclasses import dataclass


class _RuntimeStatus(str):
    @property
    def value(self) -> str:
        return str(self)


PENDING_APPROVAL_STATUS = _RuntimeStatus("pending_approval")


@dataclass
class AgentLoopContext:
    execution_id: str
    start_time: float
    iterations: int = 0
    total_tokens: int = 0
    total_tool_calls: int = 0

    @property
    def elapsed_time(self) -> float:
        return time.time() - self.start_time
