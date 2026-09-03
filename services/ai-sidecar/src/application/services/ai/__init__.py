"""
AI 服务模块
"""

# Import compatibility shim for backward compatibility (deprecated)
from .compat import LLMEvalService
from .llm_eval_service import EvalJob, EvalSpan, EvaluationService, GateMetricsSummary
from .nl_query_service import (
    NLQueryResult,
    NLQueryService,
)

__all__ = [
    "EvalJob",
    "EvalSpan",
    # New canonical names
    "EvaluationService",
    "GateMetricsSummary",
    # Legacy names (deprecated but kept for compatibility)
    "LLMEvalService",
    "NLQueryResult",
    "NLQueryService",
]
