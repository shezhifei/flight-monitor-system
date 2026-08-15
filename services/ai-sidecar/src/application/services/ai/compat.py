"""Deprecated compatibility shims for backward compatibility.

These wrappers provide temporary compatibility during transition from 
LLMEvalService (deprecated) to EvaluationService (new name).

DEPRECATION: Remove these shims after one release cycle.
"""

import warnings
from src.application.services.ai.llm_eval_service.service import (
    EvaluationService,
    EvalJob,
    EvalSpan,
    GateMetricsSummary,
)


class LLMEvalService:
    """Deprecated alias for EvaluationService.
    
    Use EvaluationService instead. This class will be removed in a future release.
    """
    
    def __init__(self, *args, **kwargs):
        warnings.warn(
            "LLMEvalService is deprecated. Use EvaluationService instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        self._inner = EvaluationService(*args, **kwargs)
    
    def __getattr__(self, name):
        return getattr(self._inner, name)


__all__ = [
    # Keep old names for backward compat
    "LLMEvalService",
    # New canonical names
    "EvaluationService",
    "EvalJob",
    "EvalSpan",
    "GateMetricsSummary",
]
