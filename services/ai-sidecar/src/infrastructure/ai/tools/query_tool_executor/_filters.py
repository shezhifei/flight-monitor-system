"""Query tool executor for natural-language flight queries."""

from __future__ import annotations

import logging
from datetime import date, datetime, timedelta
from datetime import time as dt_time
from typing import Any
from zoneinfo import ZoneInfo

from src.domain.utils.time_utils import utc_now

from ..base import ToolExecutionError, ToolExecutionStatus

logger = logging.getLogger(__name__)


class _FiltersMixin:
    """QueryToolExecutor mixin."""

    def _first_non_empty(self: dict[str, Any], *keys: str) -> str | None:
        for key in keys:
            value = self.get(key)
            if value is None:
                continue
            text = str(value).strip()
            if text:
                return text
        return None

    def _resolve_datetime_range_for_timeseries(
        self,
        *,
        filters: dict[str, Any],
        time_from: str | None,
        time_to: str | None,
    ) -> tuple[datetime, datetime]:
        start_date, end_date = self._resolve_datetime_range(
            filters=filters,
            time_from=time_from,
            time_to=time_to,
        )
        now = utc_now()

        if start_date is None and end_date is None:
            start_date = now.replace(hour=0, minute=0, second=0, microsecond=0)
            end_date = now.replace(hour=23, minute=59, second=59, microsecond=0)
            return start_date, end_date

        if start_date is None and end_date is not None:
            reference = end_date
            start_date = reference.replace(hour=0, minute=0, second=0, microsecond=0)
        if end_date is None and start_date is not None:
            reference = start_date
            end_date = reference.replace(hour=23, minute=59, second=59, microsecond=0)

        if start_date is None or end_date is None:
            raise ToolExecutionError(
                "Unable to resolve timeseries range",
                ToolExecutionStatus.VALIDATION_ERROR,
            )
        return start_date, end_date

    def _resolve_timeseries_granularity(
        self,
        *,
        filters: dict[str, Any],
        start_date: datetime,
        end_date: datetime,
    ) -> str:
        explicit = self._first_non_empty(filters, "granularity", "interval", "bucket")
        normalized = self._normalize_timeseries_granularity(explicit)
        if normalized:
            return normalized

        span_seconds = max(1.0, self._datetime_to_epoch(end_date) - self._datetime_to_epoch(start_date))
        return "hour" if span_seconds <= 48 * 3600 else "day"

    def _normalize_timeseries_granularity(self: str | None) -> str | None:
        if not self:
            return None
        normalized = str(self).strip().lower()
        if normalized in {"hour", "hourly", "1h", "h"}:
            return "hour"
        if normalized in {"day", "daily", "1d", "d"}:
            return "day"
        return None

    def _build_empty_timeseries_series(
        self,
        *,
        start_date: datetime,
        end_date: datetime,
        granularity: str,
    ) -> list[dict[str, Any]]:
        return self._build_timeseries_series_from_items(
            items=[],
            timestamp_fields=("created_at",),
            start_date=start_date,
            end_date=end_date,
            granularity=granularity,
        )

    def _build_timeseries_series_from_bucket_rows(
        self,
        *,
        rows: list[dict[str, Any]],
        start_date: datetime,
        end_date: datetime,
        granularity: str,
    ) -> list[dict[str, Any]]:
        empty_series = self._build_empty_timeseries_series(
            start_date=start_date,
            end_date=end_date,
            granularity=granularity,
        )
        bucket_counts: dict[str, int] = {str(entry.get("time") or ""): 0 for entry in empty_series if entry.get("time")}

        for row in rows:
            if not isinstance(row, dict):
                continue
            raw_time = row.get("time")
            if raw_time in (None, ""):
                raw_time = row.get("bucket")
            if raw_time in (None, ""):
                raw_time = row.get("date")
            parsed_time = self._parse_datetime_value(raw_time)
            if parsed_time is None:
                continue
            bucket_key = self._align_bucket_start(self._to_utc_aware(parsed_time), granularity).isoformat()
            if bucket_key not in bucket_counts:
                continue
            count = self._safe_int(row.get("count"), default=None)
            if count is None:
                count = self._safe_int(row.get("total"), default=0) or 0
            bucket_counts[bucket_key] += int(count)

        return [
            {
                "time": key,
                "count": int(value),
            }
            for key, value in sorted(bucket_counts.items(), key=lambda item: item[0])
        ]

    def _sum_series_counts(self: list[dict[str, Any]]) -> int:
        return int(sum(int(item.get("count") or 0) for item in self if isinstance(item, dict)))

    def _build_timeseries_series_from_items(
        self,
        *,
        items: list[dict[str, Any]],
        timestamp_fields: tuple[str, ...],
        start_date: datetime,
        end_date: datetime,
        granularity: str,
    ) -> list[dict[str, Any]]:
        start_utc = self._to_utc_aware(start_date)
        end_utc = self._to_utc_aware(end_date)
        aligned_start = self._align_bucket_start(start_utc, granularity)
        aligned_end = self._align_bucket_start(end_utc, granularity)
        step = timedelta(hours=1) if granularity == "hour" else timedelta(days=1)

        buckets: dict[datetime, int] = {}
        cursor = aligned_start
        while cursor <= aligned_end:
            buckets[cursor] = 0
            cursor += step

        for item in items:
            point = self._extract_item_datetime(item, timestamp_fields)
            if point is None:
                continue
            point_utc = self._to_utc_aware(point)
            if point_utc < start_utc or point_utc > end_utc:
                continue
            bucket_key = self._align_bucket_start(point_utc, granularity)
            if bucket_key in buckets:
                buckets[bucket_key] += 1

        return [
            {
                "time": bucket.isoformat(),
                "count": int(count),
            }
            for bucket, count in sorted(buckets.items(), key=lambda item: item[0])
        ]

    def _extract_item_datetime(self, item: dict[str, Any], timestamp_fields: tuple[str, ...]) -> datetime | None:
        for field in timestamp_fields:
            value = item.get(field)
            parsed = self._parse_datetime_value(value)
            if parsed is not None:
                return parsed
        return None

    def _align_bucket_start(self: datetime, granularity: str) -> datetime:
        if granularity == "day":
            return self.replace(hour=0, minute=0, second=0, microsecond=0)
        return self.replace(minute=0, second=0, microsecond=0)

    def _to_utc_aware(self: datetime) -> datetime:
        if self.tzinfo is None:
            return self.replace(tzinfo=ZoneInfo("UTC"))
        return self.astimezone(ZoneInfo("UTC"))

    def _parse_datetime_value(self: Any) -> datetime | None:
        if self in (None, ""):
            return None
        if isinstance(self, datetime):
            return self
        text = str(self).strip()
        if not text:
            return None
        if text.endswith("Z"):
            text = f"{text[:-1]}+00:00"
        try:
            return datetime.fromisoformat(text)
        except ValueError:
            return None

    def _resolve_datetime_range(
        self,
        *,
        filters: dict[str, Any],
        time_from: str | None,
        time_to: str | None,
    ) -> tuple[datetime | None, datetime | None]:
        start_date = self._parse_datetime_flexible(time_from, field_name="time_range.from", end_of_day=False)
        end_date = self._parse_datetime_flexible(time_to, field_name="time_range.to", end_of_day=True)

        if start_date is None:
            start_date = self._parse_datetime_flexible(
                filters.get("date_from"), field_name="date_from", end_of_day=False
            )
        if end_date is None:
            end_date = self._parse_datetime_flexible(filters.get("date_to"), field_name="date_to", end_of_day=True)

        if start_date is None and end_date is None:
            exact_date = self._parse_datetime_flexible(filters.get("date"), field_name="date", end_of_day=False)
            if exact_date is not None:
                start_date = datetime.combine(exact_date.date(), dt_time.min, tzinfo=exact_date.tzinfo)
                end_date = datetime.combine(exact_date.date(), dt_time.max, tzinfo=exact_date.tzinfo)

        if (
            start_date is not None
            and end_date is not None
            and self._datetime_to_epoch(start_date) > self._datetime_to_epoch(end_date)
        ):
            raise ToolExecutionError(
                "Invalid date range: start must be before end",
                ToolExecutionStatus.VALIDATION_ERROR,
            )
        return start_date, end_date

    def _parse_datetime_flexible(
        self,
        value: Any,
        *,
        field_name: str,
        end_of_day: bool,
    ) -> datetime | None:
        if value in (None, ""):
            return None
        if isinstance(value, datetime):
            return value
        if isinstance(value, date):
            return datetime.combine(value, dt_time.max if end_of_day else dt_time.min)

        text = str(value).strip()
        if not text:
            return None
        if text.endswith("Z"):
            text = f"{text[:-1]}+00:00"

        try:
            return datetime.fromisoformat(text)
        except ValueError:
            try:
                parsed_date = date.fromisoformat(text)
            except ValueError as exc:
                raise ToolExecutionError(
                    f"Invalid {field_name}: {value}",
                    ToolExecutionStatus.VALIDATION_ERROR,
                ) from exc
            return datetime.combine(parsed_date, dt_time.max if end_of_day else dt_time.min)

    def _datetime_to_epoch(self: datetime) -> float:
        normalized = self
        if normalized.tzinfo is None:
            normalized = normalized.replace(tzinfo=ZoneInfo("UTC"))
        return float(normalized.timestamp())

    def _parse_date(self: object) -> date | None:
        if self in (None, ""):
            return None
        if isinstance(self, date) and not isinstance(self, datetime):
            return self
        if isinstance(self, datetime):
            return self.date()
        try:
            return datetime.fromisoformat(str(self)).date()
        except (TypeError, ValueError) as exc:
            logger.warning("date parse failed; returning None: %s", exc)
            return None

    def _parse_datetime(self: object, field_name: str) -> datetime:
        if isinstance(self, datetime):
            return self
        if not isinstance(self, str):
            raise ToolExecutionError(f"{field_name} must be ISO datetime", ToolExecutionStatus.VALIDATION_ERROR)
        text = self.strip()
        if text.endswith("Z"):
            text = f"{text[:-1]}+00:00"
        try:
            return datetime.fromisoformat(text)
        except ValueError as exc:
            raise ToolExecutionError(
                f"Invalid {field_name}: {self}",
                ToolExecutionStatus.VALIDATION_ERROR,
            ) from exc
