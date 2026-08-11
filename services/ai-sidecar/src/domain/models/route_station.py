from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class RouteStation:
    code: str
    name: str | None = None

    def __post_init__(self) -> None:
        normalized_code = str(self.code or "").strip().upper()
        if not normalized_code:
            raise ValueError("route station code is required")
        normalized_name = str(self.name).strip() if self.name is not None else None
        object.__setattr__(self, "code", normalized_code)
        object.__setattr__(self, "name", normalized_name or None)

    def to_dict(self) -> dict[str, str | None]:
        return {
            "code": self.code,
            "name": self.name,
        }

    @classmethod
    def from_raw(cls, value: Any) -> RouteStation:
        if isinstance(value, cls):
            return value
        if isinstance(value, dict):
            return cls(code=value.get("code"), name=value.get("name"))
        raise ValueError(f"unsupported route station value: {value!r}")


def normalize_route_stations(values: Iterable[Any] | None) -> list[RouteStation]:
    if values is None:
        return []

    normalized: list[RouteStation] = []
    seen_codes: set[str] = set()
    for item in values:
        if item is None:
            continue
        station = RouteStation.from_raw(item)
        if station.code in seen_codes:
            continue
        seen_codes.add(station.code)
        normalized.append(station)
    return normalized
