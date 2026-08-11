"""Read-only tools that execute locally in the Python sidecar.

These tools query state but never mutate it. Write actions must go through
Rust DomainActionExecutor via the proposal-ingest path.

Tools defined here are intended for use during streaming inference,
where they can be called synchronously by the LLMStreamRunner's tool loop.
"""

import asyncio
from datetime import UTC, datetime
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


# ---------------------------------------------------------------------------
# Tool implementations
# ---------------------------------------------------------------------------


async def flight_status_lookup(flight_id: str) -> dict[str, Any]:
    """Look up the status of a flight by its identifier.

    This is a read-only tool that queries flight information without
    modifying any state. In production, this would call an internal API
    or read from a read replica of the flight database.

    Args:
        flight_id: The unique identifier for the flight (e.g., "CA1234").

    Returns:
        A dictionary containing flight status information including
        departure/arrival times, gate, and current status.
    """
    # Simulate async I/O (e.g., HTTP call to internal API)
    await asyncio.sleep(0.01)

    # In production, replace this mock with actual API calls:
    #   response = await http_client.get(f"/api/flights/{flight_id}/status")
    #   return await response.json()

    # Mock response structured like a real API response
    return {
        "flight_id": flight_id,
        "status": "on_time",
        "gate": "A12",
        "departure_airport": "PEK",
        "arrival_airport": "PVG",
        "scheduled_departure": "2026-05-18T14:30:00Z",
        "scheduled_arrival": "2026-05-18T16:45:00Z",
        "estimated_departure": "2026-05-18T14:30:00Z",
        "estimated_arrival": "2026-05-18T16:45:00Z",
        "aircraft_type": "B737",
        "last_updated": datetime.now(UTC).isoformat(),
    }


async def flight_list_by_date(date: str) -> dict[str, Any]:
    """List all flights scheduled for a given date.

    Returns a list of flight summaries without detailed status information.
    This is useful for overview queries like "what flights are scheduled tomorrow?"

    Args:
        date: The date in ISO format (YYYY-MM-DD).

    Returns:
        A dictionary containing the date and a list of flight summaries.
    """
    # Simulate async I/O
    await asyncio.sleep(0.01)

    # Mock response
    return {
        "date": date,
        "total_count": 2,
        "flights": [
            {
                "flight_id": "CA1234",
                "route": "PEK-PVG",
                "departure_airport": "PEK",
                "arrival_airport": "PVG",
                "scheduled_departure": f"{date}T08:00:00Z",
                "scheduled_arrival": f"{date}T10:15:00Z",
            },
            {
                "flight_id": "CA5678",
                "route": "PVG-CTU",
                "departure_airport": "PVG",
                "arrival_airport": "CTU",
                "scheduled_departure": f"{date}T11:30:00Z",
                "scheduled_arrival": f"{date}T14:00:00Z",
            },
        ],
    }


async def weather_at_airport(airport_code: str) -> dict[str, Any]:
    """Get current weather conditions at a specified airport.

    Args:
        airport_code: The IATA airport code (e.g., "PEK", "PVG").

    Returns:
        A dictionary containing weather information including temperature,
        conditions, wind, and visibility.
    """
    # Simulate async I/O
    await asyncio.sleep(0.01)

    # Mock response
    return {
        "airport": airport_code.upper(),
        "temperature_celsius": 22,
        "temperature_fahrenheit": 71.6,
        "condition": "clear",
        "condition_code": "CLR",
        "wind_speed_kt": 5,
        "wind_direction": "NW",
        "visibility_km": 10.0,
        "visibility_miles": 6.2,
        "humidity_percent": 45,
        "pressure_hpa": 1013,
        "recorded_at": datetime.now(UTC).isoformat(),
    }


async def get_flight_crew_info(flight_id: str) -> dict[str, Any]:
    """Get crew assignment information for a specific flight.

    Args:
        flight_id: The unique identifier for the flight.

    Returns:
        A dictionary containing crew member information including
        captain, first officer, and cabin crew assignments.
    """
    await asyncio.sleep(0.01)

    return {
        "flight_id": flight_id,
        "captain": {
            "name": "Zhang Wei",
            "employee_id": "CA001",
            "role": "Captain",
            "total_flight_hours": 8500,
        },
        "first_officer": {
            "name": "Li Ming",
            "employee_id": "CA002",
            "role": "First Officer",
            "total_flight_hours": 4200,
        },
        "cabin_crew": [
            {"name": "Wang Fang", "employee_id": "CC001", "position": "Cabin Manager"},
            {"name": "Liu Yang", "employee_id": "CC002", "position": "Flight Attendant"},
        ],
        "last_updated": datetime.now(UTC).isoformat(),
    }


async def query_resource_availability(
    resource_type: str,
    location: str | None = None,
    start_time: str | None = None,
    end_time: str | None = None,
) -> dict[str, Any]:
    """Query availability of airport resources like gates, stands, or equipment.

    Args:
        resource_type: Type of resource (e.g., "gate", "stand", "equipment").
        location: Optional location filter (e.g., "Terminal 1").
        start_time: Optional start of time window (ISO format).
        end_time: Optional end of time window (ISO format).

    Returns:
        A dictionary containing available resources matching the query.
    """
    await asyncio.sleep(0.01)

    # Mock response
    available_resources = [
        {"id": "A12", "type": "gate", "terminal": "T1", "status": "available"},
        {"id": "B5", "type": "gate", "terminal": "T2", "status": "available"},
        {"id": "S3", "type": "stand", "terminal": "T1", "status": "available"},
        {"id": "E1", "type": "equipment", "category": "boarding_bridge", "status": "available"},
    ]

    filtered = available_resources
    if resource_type:
        filtered = [r for r in filtered if r.get("type") == resource_type.lower()]
    if location:
        filtered = [r for r in filtered if location.lower() in r.get("terminal", "").lower()]

    return {
        "query": {
            "resource_type": resource_type,
            "location": location,
            "start_time": start_time,
            "end_time": end_time,
        },
        "total_available": len(filtered),
        "resources": filtered,
    }


# ---------------------------------------------------------------------------
# Tool registry
# ---------------------------------------------------------------------------

# Mapping of tool names to their async implementations
READ_ONLY_TOOLS: dict[str, Any] = {
    "flight_status_lookup": flight_status_lookup,
    "flight_list_by_date": flight_list_by_date,
    "weather_at_airport": weather_at_airport,
    "get_flight_crew_info": get_flight_crew_info,
    "query_resource_availability": query_resource_availability,
}


def get_read_only_tool_names() -> list[str]:
    """Return the list of available read-only tool names."""
    return list(READ_ONLY_TOOLS.keys())


def is_read_only_tool(tool_name: str) -> bool:
    """Check if a tool name is registered as a read-only tool."""
    return tool_name in READ_ONLY_TOOLS


async def execute_read_only_tool(
    tool_name: str,
    arguments: dict[str, Any],
) -> dict[str, Any]:
    """Execute a read-only tool by name with the given arguments.

    Args:
        tool_name: The name of the read-only tool to execute.
        arguments: Keyword arguments to pass to the tool function.

    Returns:
        The result from the tool execution.

    Raises:
        ValueError: If the tool name is not a registered read-only tool.
    """
    if tool_name not in READ_ONLY_TOOLS:
        raise ValueError(f"Unknown read-only tool: {tool_name}")

    tool_fn = READ_ONLY_TOOLS[tool_name]

    try:
        result = await tool_fn(**arguments)
        return result
    except TypeError as exc:
        # Handle mismatched arguments
        raise ValueError(f"Invalid arguments for tool '{tool_name}': {exc}") from exc


# ---------------------------------------------------------------------------
# OpenAI tool schema definitions
# ---------------------------------------------------------------------------

# These schemas define the tools in OpenAI function-calling format.
# They can be used to pass to the LLM alongside the actual implementations.
READ_ONLY_TOOL_SCHEMAS: list[dict[str, Any]] = [
    {
        "type": "function",
        "function": {
            "name": "flight_status_lookup",
            "description": "Look up the current status of a flight by its identifier.",
            "parameters": {
                "type": "object",
                "properties": {
                    "flight_id": {
                        "type": "string",
                        "description": "The unique flight identifier (e.g., CA1234)",
                    },
                },
                "required": ["flight_id"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "flight_list_by_date",
            "description": "List all flights scheduled for a specific date.",
            "parameters": {
                "type": "object",
                "properties": {
                    "date": {
                        "type": "string",
                        "description": "The date in ISO format (YYYY-MM-DD)",
                        "example": "2026-05-18",
                    },
                },
                "required": ["date"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "weather_at_airport",
            "description": "Get current weather conditions at a specified airport.",
            "parameters": {
                "type": "object",
                "properties": {
                    "airport_code": {
                        "type": "string",
                        "description": "The IATA airport code (e.g., PEK, PVG)",
                    },
                },
                "required": ["airport_code"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "get_flight_crew_info",
            "description": "Get crew assignment information for a specific flight.",
            "parameters": {
                "type": "object",
                "properties": {
                    "flight_id": {
                        "type": "string",
                        "description": "The unique flight identifier",
                    },
                },
                "required": ["flight_id"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "query_resource_availability",
            "description": "Query availability of airport resources like gates, stands, or equipment.",
            "parameters": {
                "type": "object",
                "properties": {
                    "resource_type": {
                        "type": "string",
                        "description": "Type of resource: gate, stand, or equipment",
                        "enum": ["gate", "stand", "equipment"],
                    },
                    "location": {
                        "type": "string",
                        "description": "Optional location filter (e.g., Terminal 1)",
                    },
                    "start_time": {
                        "type": "string",
                        "description": "Optional start of time window (ISO format)",
                    },
                    "end_time": {
                        "type": "string",
                        "description": "Optional end of time window (ISO format)",
                    },
                },
                "required": ["resource_type"],
            },
        },
    },
]
