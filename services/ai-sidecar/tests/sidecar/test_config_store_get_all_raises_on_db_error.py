"""Verify PostgresAIConfigStore.get_all() propagates DB errors (no exception swallowing)."""

from unittest.mock import AsyncMock, MagicMock

import pytest


@pytest.mark.asyncio
async def test_get_all_raises_on_db_error():
    from src.infrastructure.ai.postgres_config_store import PostgresAIConfigStore

    mock_cursor = AsyncMock()
    mock_cursor.execute.side_effect = ConnectionError("db connection lost")

    mock_db_ctx = MagicMock()
    mock_db_ctx.cursor.return_value.__aenter__.return_value = mock_cursor
    mock_db_ctx.cursor.return_value.__aexit__.return_value = None

    conn_ctx = MagicMock()
    conn_ctx.__aenter__.return_value = mock_db_ctx
    conn_ctx.__aexit__.return_value = None

    mock_conn = MagicMock()
    mock_conn.connection_context.return_value = conn_ctx

    store = PostgresAIConfigStore(mock_conn)
    store._initialized = True

    with pytest.raises(ConnectionError, match="db connection lost"):
        await store.get_all()
