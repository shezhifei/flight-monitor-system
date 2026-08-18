"""共享的异常分类与日志工具。"""

from __future__ import annotations

import json
import logging
from typing import Any, TypeVar

logger = logging.getLogger(__name__)
T = TypeVar("T")

try:
    import redis as _redis

    _REDIS_ERROR: type[BaseException] = _redis.exceptions.RedisError
except ModuleNotFoundError:  # pragma: no cover - optional dep

    class _RedisStubError(Exception):
        """Placeholder redis error when redis package is unavailable."""

    _REDIS_ERROR = _RedisStubError

try:
    import asyncpg as _asyncpg

    _POSTGRES_ERROR: type[BaseException] = _asyncpg.exceptions.PostgresError
except ModuleNotFoundError:  # pragma: no cover - optional dep

    class _PostgresStubError(Exception):
        """Placeholder postgres error when asyncpg package is unavailable."""

    _POSTGRES_ERROR = _PostgresStubError

try:
    import httpx as _httpx

    _HTTPX_ERROR: type[BaseException] = _httpx.HTTPError
except ModuleNotFoundError:  # pragma: no cover - optional dep

    class _HttpxStubError(Exception):
        """Placeholder httpx error when httpx package is unavailable."""

    _HTTPX_ERROR = _HttpxStubError

# 按基础设施分类的异常元组, 便于统一捕获
REDIS_EXCEPTIONS: tuple[type[BaseException], ...] = (
    _REDIS_ERROR,
    ConnectionError,
    TimeoutError,
)

POSTGRES_EXCEPTIONS: tuple[type[BaseException], ...] = (
    _POSTGRES_ERROR,
    ConnectionError,
    TimeoutError,
)

HTTP_EXCEPTIONS: tuple[type[BaseException], ...] = (
    _HTTPX_ERROR,
    ConnectionError,
    TimeoutError,
)

JSON_EXCEPTIONS: tuple[type[BaseException], ...] = (json.JSONDecodeError, TypeError, ValueError)
IO_EXCEPTIONS: tuple[type[BaseException], ...] = (OSError,)

# LLM / 运行时通用异常 (非 HTTP 层面的 LLM 调用失败)
LLM_EXCEPTIONS: tuple[type[BaseException], ...] = (
    _HTTPX_ERROR,
    RuntimeError,
    ValueError,
    ConnectionError,
    TimeoutError,
)

# 启动/DI 装配阶段可容忍的失败类别 (K5/W2-5 异常收敛):
# 导入失败、注册表缺键、构造错误、参数/类型错误、网络不可达。
# 其余异常(如 AttributeError/AssertionError)属于编程错误, 应让启动显式失败。
BOOTSTRAP_EXCEPTIONS: tuple[type[BaseException], ...] = (
    ImportError,
    LookupError,
    RuntimeError,
    ValueError,
    TypeError,
    ConnectionError,
    TimeoutError,
    OSError,
)


def log_and_fallback(
    operation: str,
    error: BaseException,
    fallback: T,
    context: dict[str, Any] | None = None,
) -> T:
    """记录异常并返回兜底值, 用于原本 bare except 后静默吞掉的场景。"""
    ctx = context or {}
    logger.warning(
        "%s_failed",
        operation,
        exc_info=error,
        extra={"error": str(error), **ctx},
    )
    return fallback


def log_and_reraise(operation: str, error: BaseException, context: dict[str, Any] | None = None) -> None:
    """记录异常后重新抛出, 用于调用方应当感知的失败。"""
    ctx = context or {}
    logger.exception(
        "%s_failed",
        operation,
        extra={"error": str(error), **ctx},
    )
    raise
