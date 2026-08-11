"""Flight domain modification policies."""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass
from typing import Protocol

from src.domain.exceptions.business import BusinessRuleException
from src.domain.models.flight import Flight
from src.domain.models.state_changes import (
    FlightCreatedChange,
    FlightFieldUpdatedChange,
    FlightStateChange,
)


class FlightModificationPolicy(Protocol):
    """Domain-level policy contract for state changes."""

    def assert_change_allowed(self, flight: Flight, change: FlightStateChange) -> None: ...


@dataclass(frozen=True)
class CommercialSigningPolicy:
    """Equivalent rule migration of legacy non-commercial write restriction."""

    error_code: str = "is_commercial_signed"
    error_message: str = "非签约航班禁止修改"

    def assert_change_allowed(self, flight: Flight, change: FlightStateChange) -> None:
        if not hasattr(flight, "is_commercial_signed"):
            return
        if bool(getattr(flight, "is_commercial_signed", True)):
            return

        if isinstance(change, FlightCreatedChange):
            return

        if isinstance(change, FlightFieldUpdatedChange) and change.field_name == "is_commercial_signed":
            return

        raise BusinessRuleException(self.error_message, self.error_code)


class CompositeFlightModificationPolicy:
    """Chain multiple policies in deterministic order."""

    def __init__(self, policies: Iterable[FlightModificationPolicy]):
        self._policies = tuple(policies or ())

    def assert_change_allowed(self, flight: Flight, change: FlightStateChange) -> None:
        for policy in self._policies:
            policy.assert_change_allowed(flight, change)


__all__ = [
    "CommercialSigningPolicy",
    "CompositeFlightModificationPolicy",
    "FlightModificationPolicy",
]
