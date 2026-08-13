"""
服务接口定义

定义基础设施层可依赖的应用层服务接口。
"""

from typing import Any, Protocol


class FlightServiceInterface(Protocol):
    """航班服务接口"""

    async def get_flight_by_id(self, flight_id: str) -> Any | None: ...

    async def search_flights(self, criteria: dict[str, Any]) -> list[Any]: ...
