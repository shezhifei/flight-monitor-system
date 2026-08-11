"""
Unified Logging Core

Provides a centralized, structured logging system using structlog.
Supports JSON output (production) and colored console output (development).
Includes async-safe request context binding via ContextVars.
"""

import logging
import os
import sys
from contextvars import ContextVar
from datetime import datetime
from typing import Any
from zoneinfo import ZoneInfo

import structlog

# Context variables for async request tracking
request_id_var: ContextVar[str] = ContextVar("request_id", default="")
user_id_var: ContextVar[str] = ContextVar("user_id", default="")

_configured = False


def _str_to_bool(value: str | None, *, default: bool) -> bool:
    if value is None:
        return default
    return str(value).strip().lower() in {"1", "true", "yes", "on"}


def _resolve_console_timezone():
    timezone_name = str(os.getenv("LOG_TIMEZONE", "") or "").strip()
    if timezone_name:
        try:
            return ZoneInfo(timezone_name)
        except (KeyError, ValueError) as exc:
            logging.getLogger(__name__).warning("LOG_TIMEZONE '%s' invalid: %s", timezone_name, exc)
    return datetime.now().astimezone().tzinfo


def _build_timestamp_processor(json_format: bool) -> structlog.types.Processor:
    use_local_console_time = _str_to_bool(
        os.getenv("LOG_CONSOLE_LOCAL_TIME"),
        default=not json_format,
    )
    if json_format or not use_local_console_time:
        return structlog.processors.TimeStamper(fmt="iso", utc=True)

    timezone_info = _resolve_console_timezone()

    def add_local_timestamp(
        logger: structlog.types.WrappedLogger,
        method_name: str,
        event_dict: dict[str, Any],
    ) -> dict[str, Any]:
        event_dict["timestamp"] = datetime.now(timezone_info).isoformat(timespec="microseconds")
        return event_dict

    return add_local_timestamp


def add_request_context(
    logger: structlog.types.WrappedLogger,
    method_name: str,
    event_dict: dict[str, Any],
) -> dict[str, Any]:
    """Processor that adds request context from ContextVars."""
    request_id = request_id_var.get()
    user_id = user_id_var.get()
    if request_id:
        event_dict["request_id"] = request_id
    if user_id:
        event_dict["user_id"] = user_id
    return event_dict


def sensitive_data_processor(
    logger: structlog.types.WrappedLogger,
    method_name: str,
    event_dict: dict[str, Any],
) -> dict[str, Any]:
    """Processor that sanitizes sensitive data in log events."""
    from src.infrastructure.monitoring.sensitive_data_filter import sanitize_log_message

    for key, value in list(event_dict.items()):
        if key in ("event", "timestamp", "level", "logger"):
            continue  # Skip metadata fields
        if isinstance(value, (str, dict, list)):
            event_dict[key] = sanitize_log_message(value)
    return event_dict


def configure_logging(json_format: bool | None = None, level: str = "INFO") -> None:
    """
    Configure the global logging system.

    Args:
        json_format: If True, output JSON. If False, colored console.
                     If None, auto-detect from LOG_FORMAT env var.
        level: Log level (DEBUG, INFO, WARNING, ERROR, CRITICAL)
    """
    global _configured

    if _configured:
        return

    if json_format is None:
        json_format = os.getenv("LOG_FORMAT", "").lower() == "json"

    log_level = getattr(logging, level.upper(), logging.INFO)

    # Shared processors for all outputs
    shared_processors: list[structlog.types.Processor] = [
        structlog.contextvars.merge_contextvars,
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        _build_timestamp_processor(json_format),
        add_request_context,
        sensitive_data_processor,  # Auto-sanitize sensitive data
        structlog.processors.StackInfoRenderer(),
        structlog.processors.UnicodeDecoder(),
    ]

    if json_format:
        # Production: JSON output
        shared_processors.append(structlog.processors.format_exc_info)
        renderer = structlog.processors.JSONRenderer(ensure_ascii=False)
    else:
        # Development: colored console output
        renderer = structlog.dev.ConsoleRenderer(colors=True, pad_level=False)

    structlog.configure(
        processors=[*shared_processors, structlog.stdlib.ProcessorFormatter.wrap_for_formatter],
        logger_factory=structlog.stdlib.LoggerFactory(),
        wrapper_class=structlog.stdlib.BoundLogger,
        cache_logger_on_first_use=True,
    )

    # Configure stdlib logging to use structlog formatter
    formatter = structlog.stdlib.ProcessorFormatter(
        foreign_pre_chain=shared_processors,
        processors=[
            structlog.stdlib.ProcessorFormatter.remove_processors_meta,
            renderer,
        ],
    )

    # Setup root logger
    root_logger = logging.getLogger()
    root_logger.handlers.clear()

    handler = logging.StreamHandler(sys.stdout)
    handler.setFormatter(formatter)
    handler.setLevel(log_level)
    root_logger.addHandler(handler)
    root_logger.setLevel(log_level)

    # Silence noisy third-party loggers
    for noisy_logger in ["httpx", "httpcore", "asyncio", "uvicorn.access", "hypercorn.error"]:
        logging.getLogger(noisy_logger).setLevel(logging.WARNING)

    _configured = True


def get_logger(name: str) -> structlog.stdlib.BoundLogger:
    """
    Get a structured logger instance.

    Args:
        name: Logger name (typically __name__)

    Returns:
        A bound structlog logger
    """
    if not _configured:
        configure_logging()
    return structlog.get_logger(name)


def bind_request_id(request_id: str) -> None:
    """Bind a request ID to the current async context."""
    request_id_var.set(request_id)


def bind_user_id(user_id: str) -> None:
    """Bind a user ID to the current async context."""
    user_id_var.set(user_id)


def clear_context() -> None:
    """Clear all context variables (call at end of request)."""
    request_id_var.set("")
    user_id_var.set("")
