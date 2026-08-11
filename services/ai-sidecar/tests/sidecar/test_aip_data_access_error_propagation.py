"""
Tests for AIP data_access error propagation.

Verifies that database errors are propagated to callers rather than being
silently replaced with fabricated stub data.
"""

from __future__ import annotations

import sys
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.infrastructure.ai.aip.data_access import ObjectDataAccessor
from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS


class TestAipDataAccessErrorPropagation:
    """AIP data access must propagate DB errors, not fabricate stubs."""

    @pytest.fixture
    def accessor(self):
        return ObjectDataAccessor()

    @pytest.mark.asyncio
    async def test_flight_state_propagates_db_error(self, accessor):
        """RED: DB error must propagate, not return stub data."""
        # Create a mock service that raises DB exception
        mock_service_instance = MagicMock()
        mock_service_instance.get_flight_by_id = AsyncMock(
            side_effect=POSTGRES_EXCEPTIONS[0]("DB connection failed")
        )

        # Mock the module and class
        mock_module = MagicMock()
        mock_module.FlightQueryService = MagicMock(return_value=mock_service_instance)

        # Patch sys.modules to intercept the import
        with pytest.MonkeyPatch.context() as m:
            m.setitem(
                sys.modules,
                "src.application.services.flight.flight_query_service",
                mock_module,
            )

            # Act & Assert: Should raise, not return stub
            with pytest.raises(Exception):
                await accessor.get_object_state("Flight", "CA1234_20240101")

    @pytest.mark.asyncio
    async def test_stand_state_propagates_db_error(self, accessor):
        """RED: DB error must propagate, not return stub data."""
        mock_service_instance = MagicMock()
        mock_service_instance.get_stand_by_id = AsyncMock(
            side_effect=POSTGRES_EXCEPTIONS[0]("DB connection failed")
        )

        mock_module = MagicMock()
        mock_module.DispatchQueryService = MagicMock(return_value=mock_service_instance)

        with pytest.MonkeyPatch.context() as m:
            m.setitem(
                sys.modules,
                "src.application.services.dispatch.dispatch_query_service",
                mock_module,
            )

            with pytest.raises(Exception):
                await accessor.get_object_state("Stand", "A01")

    @pytest.mark.asyncio
    async def test_team_state_propagates_db_error(self, accessor):
        """RED: DB error must propagate, not return stub data."""
        mock_service_instance = MagicMock()
        mock_service_instance.get_team_by_id = AsyncMock(
            side_effect=POSTGRES_EXCEPTIONS[0]("DB connection failed")
        )

        mock_module = MagicMock()
        mock_module.DispatchQueryService = MagicMock(return_value=mock_service_instance)

        with pytest.MonkeyPatch.context() as m:
            m.setitem(
                sys.modules,
                "src.application.services.dispatch.dispatch_query_service",
                mock_module,
            )

            with pytest.raises(Exception):
                await accessor.get_object_state("Team", "team-001")

    @pytest.mark.asyncio
    async def test_anomaly_state_propagates_db_error(self, accessor):
        """RED: DB error must propagate, not return stub data."""
        mock_service_instance = MagicMock()
        mock_service_instance.get_anomaly_by_id = AsyncMock(
            side_effect=POSTGRES_EXCEPTIONS[0]("DB connection failed")
        )

        mock_module = MagicMock()
        mock_module.AnomalyQueryService = MagicMock(return_value=mock_service_instance)

        with pytest.MonkeyPatch.context() as m:
            m.setitem(
                sys.modules,
                "src.application.services.anomaly.anomaly_query_service",
                mock_module,
            )

            with pytest.raises(Exception):
                await accessor.get_object_state("Anomaly", "anomaly-001")

    @pytest.mark.asyncio
    async def test_todo_state_propagates_db_error(self, accessor):
        """RED: DB error must propagate, not return stub data."""
        mock_service_instance = MagicMock()
        mock_service_instance.get_todo_by_id = AsyncMock(
            side_effect=POSTGRES_EXCEPTIONS[0]("DB connection failed")
        )

        mock_module = MagicMock()
        mock_module.AsyncTodoService = MagicMock(return_value=mock_service_instance)

        with pytest.MonkeyPatch.context() as m:
            m.setitem(
                sys.modules,
                "src.application.services.async_todo_service",
                mock_module,
            )

            with pytest.raises(Exception):
                await accessor.get_object_state("Todo", "todo-001")

    @pytest.mark.asyncio
    async def test_equipment_state_returns_none_not_stub(self, accessor):
        """RED: Equipment state should return None (not implemented), not fabricated stub."""
        result = await accessor.get_object_state("Equipment", "equip-001")
        # Should be None (not found/not implemented), not a stub with fake fuel_level=80
        assert result is None

    @pytest.mark.asyncio
    async def test_flight_state_returns_none_when_not_found(self, accessor):
        """GREEN: When flight doesn't exist, return None (not stub)."""
        mock_service_instance = MagicMock()
        mock_service_instance.get_flight_by_id = AsyncMock(return_value=None)

        mock_module = MagicMock()
        mock_module.FlightQueryService = MagicMock(return_value=mock_service_instance)

        with pytest.MonkeyPatch.context() as m:
            m.setitem(
                sys.modules,
                "src.application.services.flight.flight_query_service",
                mock_module,
            )

            result = await accessor.get_object_state("Flight", "nonexistent_flight")
            assert result is None

    @pytest.mark.asyncio
    async def test_flight_state_returns_real_data(self, accessor):
        """GREEN: When flight exists, return real data (not stub)."""
        mock_flight = MagicMock()
        mock_flight.id = "CA1234_20240101"
        mock_flight.flight_number = "CA1234"
        mock_flight.flight_type = "departure"
        mock_flight.status = "in_flight"
        mock_flight.stand = "A01"
        mock_flight.gate = "G1"
        mock_flight.aircraft_type = "B737"
        mock_flight.origin = "PEK"
        mock_flight.destination = "SHA"
        mock_flight.scheduled_departure = None
        mock_flight.scheduled_arrival = None
        mock_flight.actual_departure = None
        mock_flight.actual_arrival = None
        mock_flight.delay_minutes = 0
        mock_flight.assigned_team_id = "team-001"

        mock_service_instance = MagicMock()
        mock_service_instance.get_flight_by_id = AsyncMock(return_value=mock_flight)

        mock_module = MagicMock()
        mock_module.FlightQueryService = MagicMock(return_value=mock_service_instance)

        with pytest.MonkeyPatch.context() as m:
            m.setitem(
                sys.modules,
                "src.application.services.flight.flight_query_service",
                mock_module,
            )

            result = await accessor.get_object_state("Flight", "CA1234_20240101")
            assert result is not None
            assert result["flight_id"] == "CA1234_20240101"
            assert result["status"] == "in_flight"  # Real status, not stub "scheduled"
            assert result["assigned_team_id"] == "team-001"  # Real team, not stub None
