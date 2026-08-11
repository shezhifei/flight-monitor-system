"""Shared unit-of-work connection context helpers."""

from __future__ import annotations

from contextvars import ContextVar, Token
from typing import Any

_current_connection: ContextVar[Any | None] = ContextVar(
    "shared_uow_current_connection",
    default=None,
)


def get_current_connection() -> Any | None:
    """Get current transactional connection from context."""
    return _current_connection.get()


def bind_connection(connection: Any | None) -> Token:
    """Bind transactional connection to current context."""
    return _current_connection.set(connection)


def reset_connection(token: Token) -> None:
    """Reset transactional connection context using previous token."""
    _current_connection.reset(token)
