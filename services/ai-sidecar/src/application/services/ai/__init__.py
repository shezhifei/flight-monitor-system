"""
AI 服务模块
"""

# Import compatibility shim for backward compatibility (deprecated)
from .compat import LLMEvalService
from .llm_eval_service import EvaluationService, EvalJob, EvalSpan, GateMetricsSummary
from .nl_query_service import (
    NLQueryResult,
    NLQueryService,
)

__all__ = [
    # Legacy names (deprecated but kept for compatibility)
    "LLMEvalService",
    # New canonical names
    "EvaluationService",
    "EvalJob",
    "EvalSpan",
    "GateMetricsSummary",
    "NLQueryResult",
    "NLQueryService",
]
