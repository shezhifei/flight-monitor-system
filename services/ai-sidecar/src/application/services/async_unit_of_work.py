"""Async unit-of-work abstraction for application services."""

from __future__ import annotations

from contextvars import Token
from typing import Any

from src.shared.uow_context import bind_connection, get_current_connection, reset_connection


class AsyncUnitOfWork:
    """Manage transactional scope and expose current connection via context."""

    def __init__(self, db_pool: Any):
        self._db_pool = db_pool
        self._context_manager: Any | None = None
        self._connection: Any | None = None
        self._token: Token | None = None

    @classmethod
    def get_current_connection(cls) -> Any | None:
        return get_current_connection()

    async def __aenter__(self) -> AsyncUnitOfWork:
        transaction_context = getattr(self._db_pool, "transaction_context", None)
        if callable(transaction_context):
            self._context_manager = transaction_context()
        else:
            connection_context = getattr(self._db_pool, "connection_context", None)
            if not callable(connection_context):
                raise RuntimeError("db_pool does not provide transaction_context/connection_context")
            self._context_manager = connection_context()

        self._connection = await self._context_manager.__aenter__()
        self._token = bind_connection(self._connection)
        return self

    async def __aexit__(self, exc_type, exc, tb) -> bool:
        if self._token is not None:
            reset_connection(self._token)
            self._token = None
        if self._context_manager is None:
            return False
        return bool(await self._context_manager.__aexit__(exc_type, exc, tb))
