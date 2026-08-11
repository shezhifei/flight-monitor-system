"""航班核心子域服务包"""

from .async_flight_service import AsyncFlightApplicationService
from .flight_insight_service import AIUnavailableError, FlightInsightService
from .flight_query_service import FlightQueryService
from .flight_update_translator import FlightUpdateTranslator
from .flight_write_commands import (
    AssignGateCommand,
    AssignStandCommand,
    ChangeStatusCommand,
    PatchActualTimesCommand,
    PatchEstimatedTimesCommand,
    PatchScheduledTimesCommand,
    UpdateRegistrationCommand,
    UpsertInboundLegCommand,
    UpsertOutboundLegCommand,
)

__all__ = [
    "AIUnavailableError",
    "AssignGateCommand",
    "AssignStandCommand",
    "AsyncFlightApplicationService",
    "ChangeStatusCommand",
    "FlightInsightService",
    "FlightQueryService",
    "FlightUpdateTranslator",
    "PatchActualTimesCommand",
    "PatchEstimatedTimesCommand",
    "PatchScheduledTimesCommand",
    "UpdateRegistrationCommand",
    "UpsertInboundLegCommand",
    "UpsertOutboundLegCommand",
]
