"""Shared constants for LLM evaluation configuration."""

from __future__ import annotations

from typing import Final

DEFAULT_EVAL_SUITE: Final[str] = "quick"
SUPPORTED_EVAL_SUITES: Final[tuple[str, ...]] = (
    "quick",
    "standard",
    "full",
    "reasoning",
    "text2sql",
    "human_in_the_loop",
)


__all__ = ["DEFAULT_EVAL_SUITE", "SUPPORTED_EVAL_SUITES"]
