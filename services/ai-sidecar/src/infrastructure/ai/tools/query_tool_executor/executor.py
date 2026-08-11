"""Query Tool Executor — thin facade composing all mixins."""

from __future__ import annotations

from ..base import BaseToolExecutor
from ._builders import _BuildersMixin
from ._core import _CoreMixin
from ._filters import _FiltersMixin
from ._handlers_flights import _HandlersFlightsMixin
from ._handlers_insights import _HandlersInsightsMixin
from ._handlers_timeseries import _HandlersTimeseriesMixin


class QueryToolExecutor(
    _CoreMixin,
    _FiltersMixin,
    _BuildersMixin,
    _HandlersFlightsMixin,
    _HandlersInsightsMixin,
    _HandlersTimeseriesMixin,
    BaseToolExecutor,
):
    """Execute read-only query tools using database-side filtering."""
