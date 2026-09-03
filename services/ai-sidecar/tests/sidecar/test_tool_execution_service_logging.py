"""验证 tool_execution_service 在异常时记录日志。"""

from unittest.mock import MagicMock, patch

import pytest


@pytest.mark.asyncio
async def test_tool_execution_logs_on_exception():
    from src.infrastructure.ai.services.tool_execution_service import ToolExecutionService

    service = ToolExecutionService(ai_client=MagicMock())
    service.metrics_callback = MagicMock()

    with (
        patch("src.infrastructure.ai.services.tool_execution_service.logger.error") as mock_logger_error,
        patch.object(service, "_request_ai", side_effect=RuntimeError("LLM timeout")),
        pytest.raises(RuntimeError),
    ):
        await service.execute_with_tools(
            message="test",
            tools=[],
            tool_executor=MagicMock(),
            config=MagicMock(),
        )

    mock_logger_error.assert_called_once()
    call_args, _ = mock_logger_error.call_args
    assert "tool execution failed" in str(call_args)
