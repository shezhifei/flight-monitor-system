import asyncio
import random
from typing import Any

import httpx

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class ExecutionRecovery:
    """执行恢复机制（含因果记忆）"""

    def __init__(self, max_retries: int = 3, retry_delay: float = 1.0):
        self.max_retries = max_retries
        self.retry_delay = retry_delay
        # 因果记忆：记录本次执行中已经犯过的错误模式
        self.error_memory: list[dict[str, str]] = []

    def _compute_backoff(self, retry_count: int) -> float:
        """Exponential backoff with jitter."""
        base = self.retry_delay * (2**retry_count)
        jitter = random.uniform(0.8, 1.2)
        return max(0.1, base * jitter)

    def _classify_error(self, error: Exception) -> tuple[bool, str]:
        """Return (should_retry, reason) based on exception semantics."""
        if isinstance(error, asyncio.CancelledError):
            return False, "cancelled"

        if isinstance(error, (asyncio.TimeoutError, TimeoutError)):
            return True, "timeout"

        if isinstance(error, (httpx.ConnectError, httpx.NetworkError, httpx.TimeoutException)):
            return True, "network"

        if isinstance(error, httpx.HTTPStatusError):
            status_code = error.response.status_code
            if status_code == 429:
                return True, "http_429"
            if 500 <= status_code < 600:
                return True, f"http_{status_code}"
            if status_code in {408, 409, 425}:
                return True, f"http_{status_code}"
            return False, f"http_{status_code}"

        error_str = str(error).lower()
        retryable_keywords = (
            "rate limit",
            "timeout",
            "connection",
            "temporarily unavailable",
            "server error",
        )
        for keyword in retryable_keywords:
            if keyword in error_str:
                return True, f"keyword:{keyword}"

        return False, "non_retryable"

    def record_tool_error(self, tool_name: str, error_code: str, args: dict | None = None) -> None:
        """记录工具错误到因果记忆"""
        self.error_memory.append(
            {
                "tool": tool_name,
                "error": error_code,
                "args_snippet": str(args)[:80] if args else "",
            }
        )
        # 限制记忆大小，只保留最近 10 条
        if len(self.error_memory) > 10:
            self.error_memory = self.error_memory[-10:]

    def get_avoidance_hints(self) -> str:
        """
        从因果记忆生成避免提示。
        注入到 system prompt 中帮助模型避免重复犯同样错误。
        """
        if not self.error_memory:
            return ""
        hints = ["[系统提示] 以下是你之前犯过的错误，请务必避免重复："]
        for err in self.error_memory[-3:]:
            hints.append(f"- 调用工具 '{err['tool']}' 时出错（{err['error']}）")
        hints.append("请根据上述经验选择正确的工具和参数。")
        return "\n".join(hints)

    async def recover_from_error(self, error: Exception, retry_count: int, context: Any | None = None) -> bool:
        """
        尝试从错误中恢复

        Returns:
            bool: 如果应该重试则返回True，否则False
        """
        if retry_count >= self.max_retries:
            return False

        should_retry, reason = self._classify_error(error)
        if should_retry:
            delay_seconds = self._compute_backoff(retry_count)
            execution_id = getattr(context, "execution_id", "unknown") if context else "unknown"
            logger.warning(
                f"Recoverable error encountered ({reason}) for execution {execution_id}: {error}. "
                f"Retrying ({retry_count + 1}/{self.max_retries}) in {delay_seconds:.2f}s"
            )
            await asyncio.sleep(delay_seconds)
            return True

        return False
