"""
输出安全守卫 (Output Guardrail)

对 LLM 最终回复进行轻量级规则检查，
检测幻觉（编造的航班号/数据）、内部信息泄露、承诺不存在的能力等。

Task C2 起，主路径经 ``hooks.pipeline.OutputGuardrailHook``（Stop 相）调用本模块；
``OutputGuardrail.validate`` / ``apply_guardrail_warnings`` 保留为兼容入口。
"""

import re
from typing import ClassVar

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class GuardrailResult:
    """Guardrail 检查结果"""

    def __init__(
        self,
        passed: bool = True,
        warnings: list[str] | None = None,
        blocked: bool = False,
        block_reason: str = "",
    ):
        self.passed = passed
        self.warnings = warnings or []
        self.blocked = blocked
        self.block_reason = block_reason


class OutputGuardrail:
    """
    输出安全守卫

    使用轻量规则检查（无额外 LLM 调用），对最终回复进行以下校验：
    1. 航班号一致性：回复中提到的航班号是否出现在工具返回数据中
    2. 内部信息泄露：是否暴露了 run_id、tool_call_id 等系统内部标识
    3. 能力边界：是否承诺了系统不支持的操作
    """

    # 系统内部标识模式
    _INTERNAL_PATTERNS: ClassVar[list[str]] = [
        r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b",  # UUID
        r"run_id\s*[:=]\s*\S+",
        r"tool_call_id\s*[:=]\s*\S+",
        r"execution_id\s*[:=]\s*\S+",
        r"call_[A-Za-z0-9]{20,}",  # OpenAI tool_call_id格式
    ]

    # 航班号模式
    _FLIGHT_NUMBER_PATTERN = re.compile(r"\b([A-Z]{2}\d{3,4})\b")

    # 不应承诺的操作关键词
    _FORBIDDEN_PROMISES: ClassVar[list[str]] = [
        "已经为您",
        "已帮您",
        "已完成",
        "操作成功",
    ]

    def validate(
        self,
        response_text: str,
        tool_results: list[str] | None = None,
        had_write_operations: bool = False,
    ) -> GuardrailResult:
        """
        对 LLM 最终回复进行安全检查。

        Args:
            response_text: LLM 的最终回复文本
            tool_results: 本次执行中所有工具返回的结果文本列表
            had_write_operations: 本次执行是否包含写操作（如 change_stand）

        Returns:
            GuardrailResult
        """
        if not response_text or not response_text.strip():
            return GuardrailResult(passed=True)

        warnings = []

        # 1. 检查内部信息泄露
        self._check_internal_leakage(response_text, warnings)

        # 2. 检查航班号一致性
        if tool_results:
            self._check_flight_number_consistency(response_text, tool_results, warnings)

        # 3. 检查虚假操作承诺
        if not had_write_operations:
            self._check_false_operation_claims(response_text, warnings)

        if warnings:
            logger.warning(f"Output guardrail warnings: {warnings}")

        return GuardrailResult(
            passed=len(warnings) == 0,
            warnings=warnings,
        )

    def _check_internal_leakage(self, text: str, warnings: list[str]) -> None:
        """检查是否泄露系统内部标识"""
        for pattern in self._INTERNAL_PATTERNS:
            if re.search(pattern, text):
                warnings.append("回复中可能包含系统内部标识信息，建议过滤。")
                break

    def _check_flight_number_consistency(
        self,
        response_text: str,
        tool_results: list[str],
        warnings: list[str],
    ) -> None:
        """检查回复中提到的航班号是否出现在工具结果中"""
        response_flights = set(self._FLIGHT_NUMBER_PATTERN.findall(response_text))
        if not response_flights:
            return

        # 从工具结果中提取所有航班号
        tool_flights = set()
        for result in tool_results:
            tool_flights.update(self._FLIGHT_NUMBER_PATTERN.findall(str(result)))

        # 如果没有工具结果中的航班号数据，跳过检查
        if not tool_flights:
            return

        fabricated = response_flights - tool_flights
        if fabricated:
            warnings.append(f"回复中提到了工具数据中未出现的航班号：{', '.join(fabricated)}。可能存在编造数据的风险。")

    def _check_false_operation_claims(self, text: str, warnings: list[str]) -> None:
        """检查是否虚假声称完成了操作"""
        for phrase in self._FORBIDDEN_PROMISES:
            if phrase in text:
                # 需要更精确判断：是否在断言完成了修改操作
                context_window = text[max(0, text.index(phrase) - 20) : text.index(phrase) + len(phrase) + 30]
                operation_keywords = ["修改", "更改", "变更", "删除", "通知", "发送", "创建"]
                if any(kw in context_window for kw in operation_keywords):
                    warnings.append(
                        f"回复声称完成了操作（'{phrase}'），但本轮执行未包含写操作工具调用。可能是虚假承诺。"
                    )
                    break


def apply_guardrail_warnings(
    response_text: str,
    guardrail_result: GuardrailResult,
) -> str:
    """
    根据 Guardrail 检查结果，在回复末尾追加警告标记。

    前端可据此展示提示信息。
    """
    if guardrail_result.passed:
        return response_text

    warning_tag = "\n\n---\n⚠️ " + " | ".join(guardrail_result.warnings)
    return response_text + warning_tag
