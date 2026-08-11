"""Tests for AIP action handlers - must not fake success when services unavailable

Task 5 (P0): AIP write actions stop faking success

Two error paths are tested:
1. Service module unavailable (ImportError) → raises RuntimeError (no fake success)
2. Service call fails (DB error etc.) → returns {"success": False, "error": ...} (honest failure, not fake success)
"""

import sys
import types
import pytest
from unittest.mock import AsyncMock, MagicMock, patch
from src.infrastructure.ai.aip.action_handlers import (
    _handle_flight_change_stand,
    _handle_flight_delay_flight,
    _handle_flight_assign_team,
    _handle_flight_update_status,
    _handle_anomaly_acknowledge,
    _handle_todo_create,
)


# ---------------------------------------------------------------------------
# Path 1: Service module unavailable → ImportError → RuntimeError
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_flight_change_stand_raises_when_service_unavailable():
    """Test that flight stand change raises error when FlightCommandGateway unavailable"""
    with patch.dict(sys.modules, {"src.application.services.flight.flight_command_gateway": None}):
        with pytest.raises(RuntimeError, match="FlightCommandGateway not available"):
            await _handle_flight_change_stand("flight-123", {"new_stand": "A01"})


@pytest.mark.asyncio
async def test_flight_delay_flight_raises_when_service_unavailable():
    """Test that flight delay recording raises error when FlightCommandGateway unavailable"""
    with patch.dict(sys.modules, {"src.application.services.flight.flight_command_gateway": None}):
        with pytest.raises(RuntimeError, match="FlightCommandGateway not available"):
            await _handle_flight_delay_flight("flight-123", {"delay_minutes": 30})


@pytest.mark.asyncio
async def test_flight_assign_team_raises_when_service_unavailable():
    """Test that team assignment raises error when DispatchCommandService unavailable"""
    with patch.dict(sys.modules, {"src.application.services.dispatch.dispatch_command_service": None}):
        with pytest.raises(RuntimeError, match="DispatchCommandService not available"):
            await _handle_flight_assign_team("flight-123", {"team_id": "team-456"})


@pytest.mark.asyncio
async def test_flight_update_status_raises_when_service_unavailable():
    """Test that flight status update raises error when FlightCommandGateway unavailable"""
    with patch.dict(sys.modules, {"src.application.services.flight.flight_command_gateway": None}):
        with pytest.raises(RuntimeError, match="FlightCommandGateway not available"):
            await _handle_flight_update_status("flight-123", {"status": "departed"})


@pytest.mark.asyncio
async def test_anomaly_acknowledge_raises_when_service_unavailable():
    """Test that anomaly acknowledgment raises error when AnomalyDetectionService unavailable"""
    with patch.dict(sys.modules, {"src.application.services.anomaly.anomaly_detection_service": None}):
        with pytest.raises(RuntimeError, match="AnomalyDetectionService not available"):
            await _handle_anomaly_acknowledge("anomaly-789", {"acknowledged_by": "AI"})


@pytest.mark.asyncio
async def test_todo_create_raises_when_service_unavailable():
    """Test that todo creation raises error when AsyncTodoService unavailable"""
    with patch.dict(sys.modules, {"src.application.services.async_todo_service": None}):
        with pytest.raises(RuntimeError, match="AsyncTodoService not available"):
            await _handle_todo_create("todo-001", {"title": "Test task", "priority": "high"})


# ---------------------------------------------------------------------------
# Path 2: Service call fails → returns {"success": False, "error": ...}
#         (honest failure, NOT fake success)
# ---------------------------------------------------------------------------


def _make_mock_module(module_path: str, class_name: str, mock_instance: MagicMock):
    """Create a mock module with a class that returns the given mock instance.

    This registers the module in sys.modules so that `from <module> import <class>`
    works inside the handler's try block.
    """
    mock_mod = types.ModuleType(module_path)
    mock_cls = MagicMock(return_value=mock_instance)
    setattr(mock_mod, class_name, mock_cls)
    return mock_mod, mock_cls


@pytest.mark.asyncio
async def test_flight_change_stand_returns_failure_on_db_error():
    """DB errors return success=False, NOT fake success=True"""
    mock_gateway = MagicMock()
    mock_gateway.update_flight_stand = AsyncMock(side_effect=Exception("Database connection failed"))
    mock_mod, mock_cls = _make_mock_module(
        "src.application.services.flight.flight_command_gateway",
        "FlightCommandGateway",
        mock_gateway,
    )
    with patch.dict(sys.modules, {"src.application.services.flight.flight_command_gateway": mock_mod}):
        result = await _handle_flight_change_stand("flight-123", {"new_stand": "A01"})

    assert result["success"] is False
    assert result["error"] == "action_failed"


@pytest.mark.asyncio
async def test_flight_delay_flight_returns_failure_on_db_error():
    """DB errors return success=False, NOT fake success=True"""
    mock_gateway = MagicMock()
    mock_gateway.update_flight_delay = AsyncMock(side_effect=Exception("Database timeout"))
    mock_mod, mock_cls = _make_mock_module(
        "src.application.services.flight.flight_command_gateway",
        "FlightCommandGateway",
        mock_gateway,
    )
    with patch.dict(sys.modules, {"src.application.services.flight.flight_command_gateway": mock_mod}):
        result = await _handle_flight_delay_flight("flight-123", {"delay_minutes": 30})

    assert result["success"] is False
    assert result["error"] == "action_failed"


@pytest.mark.asyncio
async def test_flight_assign_team_returns_failure_on_db_error():
    """DB errors return success=False, NOT fake success=True"""
    mock_service = MagicMock()
    mock_service.assign_team_to_flight = AsyncMock(side_effect=Exception("Database connection failed"))
    mock_mod, mock_cls = _make_mock_module(
        "src.application.services.dispatch.dispatch_command_service",
        "DispatchCommandService",
        mock_service,
    )
    with patch.dict(sys.modules, {"src.application.services.dispatch.dispatch_command_service": mock_mod}):
        result = await _handle_flight_assign_team("flight-123", {"team_id": "team-456"})

    assert result["success"] is False
    assert result["error"] == "action_failed"


@pytest.mark.asyncio
async def test_flight_update_status_returns_failure_on_db_error():
    """DB errors return success=False, NOT fake success=True"""
    mock_gateway = MagicMock()
    mock_gateway.update_flight_status = AsyncMock(side_effect=Exception("Database connection failed"))
    mock_mod, mock_cls = _make_mock_module(
        "src.application.services.flight.flight_command_gateway",
        "FlightCommandGateway",
        mock_gateway,
    )
    with patch.dict(sys.modules, {"src.application.services.flight.flight_command_gateway": mock_mod}):
        result = await _handle_flight_update_status("flight-123", {"status": "departed"})

    assert result["success"] is False
    assert result["error"] == "action_failed"


@pytest.mark.asyncio
async def test_anomaly_acknowledge_returns_failure_on_db_error():
    """DB errors return success=False, NOT fake success=True"""
    mock_service = MagicMock()
    mock_service.acknowledge_anomaly = AsyncMock(side_effect=Exception("Database connection failed"))
    mock_mod, mock_cls = _make_mock_module(
        "src.application.services.anomaly.anomaly_detection_service",
        "AnomalyDetectionService",
        mock_service,
    )
    with patch.dict(sys.modules, {"src.application.services.anomaly.anomaly_detection_service": mock_mod}):
        result = await _handle_anomaly_acknowledge("anomaly-789", {"acknowledged_by": "AI"})

    assert result["success"] is False
    assert result["error"] == "action_failed"


@pytest.mark.asyncio
async def test_todo_create_returns_failure_on_db_error():
    """DB errors return success=False, NOT fake success=True"""
    mock_service = MagicMock()
    mock_service.create_todo = AsyncMock(side_effect=Exception("Database connection failed"))
    mock_mod, mock_cls = _make_mock_module(
        "src.application.services.async_todo_service",
        "AsyncTodoService",
        mock_service,
    )
    with patch.dict(sys.modules, {"src.application.services.async_todo_service": mock_mod}):
        result = await _handle_todo_create("todo-001", {"title": "Test task", "priority": "high"})

    assert result["success"] is False
    assert result["error"] == "action_failed"
