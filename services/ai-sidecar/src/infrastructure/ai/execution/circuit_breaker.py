"""
工具调用熔断器 (Tool Call Circuit Breaker)

检测 Agent 循环中的工具调用死循环模式（如连续失败、重复调用相同错误工具），
触发熔断后强制模型停止工具调用并以文字回答用户。
"""

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class ToolCallCircuitBreaker:
    """
    工具调用熔断器

    监控工具调用模式，检测以下异常情况：
    1. 连续 N 次工具调用失败
    2. 连续调用同一个工具名（含失败重试）
    3. 调用不存在的工具超过阈值

    触发熔断后，agent loop 应注入系统消息要求模型停止工具调用。
    """

    def __init__(
        self,
        max_consecutive_failures: int = 3,
        max_same_tool_repeats: int = 2,
        max_not_found_tools: int = 2,
    ):
        self.max_consecutive_failures = max_consecutive_failures
        self.max_same_tool_repeats = max_same_tool_repeats
        self.max_not_found_tools = max_not_found_tools

        self._consecutive_failures: int = 0
        self._recent_tool_calls: list[tuple[str, str]] = []  # (tool_name, status)
        self._not_found_count: int = 0
        self._is_open: bool = False
        self._break_reason: str = ""

    def record(self, tool_name: str, status: str) -> None:
        """
        记录一次工具调用结果。

        Args:
            tool_name: 工具名称
            status: 工具执行状态 (success / error / validation_error / not_found 等)
        """
        self._recent_tool_calls.append((tool_name, status))

        if status == "success" or status == "pending_approval":
            # 成功调用重置连续失败计数
            self._consecutive_failures = 0
            return

        # 失败情况
        self._consecutive_failures += 1

        # 检测重复调用不存在的工具
        if "TOOL_NOT_REGISTERED" in str(status) or "未知的工具" in str(status):
            self._not_found_count += 1

        # 检查是否应触发熔断
        self._evaluate()

    def _evaluate(self) -> None:
        """评估是否应触发熔断"""

        # 规则 1：连续失败超过阈值
        if self._consecutive_failures >= self.max_consecutive_failures:
            self._is_open = True
            self._break_reason = (
                f"连续 {self._consecutive_failures} 次工具调用失败。请停止调用工具，直接用文字回答用户的问题。"
            )
            logger.warning(f"Circuit breaker OPEN: {self._consecutive_failures} consecutive failures")
            return

        # 规则 2：连续调用同一个工具失败
        if len(self._recent_tool_calls) >= self.max_same_tool_repeats:
            recent = self._recent_tool_calls[-self.max_same_tool_repeats :]
            names = [t[0] for t in recent]
            statuses = [t[1] for t in recent]
            if len(set(names)) == 1 and all(s != "success" and s != "pending_approval" for s in statuses):
                self._is_open = True
                self._break_reason = (
                    f"你已连续 {self.max_same_tool_repeats} 次调用工具 "
                    f"'{names[0]}' 且全部失败。"
                    "这个工具可能不适用于当前请求。"
                    "请换一个工具，或直接用文字回答用户。"
                )
                logger.warning(f"Circuit breaker OPEN: repeated calls to '{names[0]}'")
                return

        # 规则 3：调用不存在的工具过多
        if self._not_found_count >= self.max_not_found_tools:
            self._is_open = True
            self._break_reason = (
                f"你已 {self._not_found_count} 次尝试调用不存在的工具。"
                "不要编造工具名称。请直接用文字回答用户，"
                "或从已提供的工具列表中选择。"
            )
            logger.warning(f"Circuit breaker OPEN: {self._not_found_count} not-found tool attempts")
            return

    @property
    def is_open(self) -> bool:
        """熔断器是否已打开"""
        return self._is_open

    @property
    def break_reason(self) -> str:
        """熔断原因"""
        return self._break_reason

    def reset(self) -> None:
        """重置熔断器状态"""
        self._consecutive_failures = 0
        self._recent_tool_calls.clear()
        self._not_found_count = 0
        self._is_open = False
        self._break_reason = ""

    def get_circuit_break_message(self) -> str:
        """
        生成注入 agent loop 的熔断系统消息。
        当 is_open 为 True 时调用。
        """
        return f"[系统提示] {self._break_reason}\n如果你认为无法完成用户请求，请坦诚告知用户你目前能做什么。"
