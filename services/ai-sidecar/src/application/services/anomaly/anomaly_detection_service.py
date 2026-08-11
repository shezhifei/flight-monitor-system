"""Anomaly detection service based on rule checks."""

from __future__ import annotations

import asyncio
from datetime import timedelta
from typing import Any

from src.application.services.anomaly.ports import (
    AnomalyFlightReadPort,
    AnomalyNotifyPort,
    AnomalyTodoWritePort,
    build_anomaly_payload,
)
from src.domain.models.anomaly import Anomaly, AnomalySeverity, AnomalyType
from src.domain.models.anomaly_rule import AnomalyRule
from src.domain.utils.time_utils import now_in_tz, utc_now
from src.infrastructure.logging.core import get_logger
from src.shared.id_generator import generate_id

logger = get_logger(__name__)


class AnomalyDetectionService:
    """Run anomaly checks for active flights."""

    def __init__(
        self,
        anomaly_repo: Any,
        *,
        alert_service: Any,
        kpi_aggregation_service: Any = None,
        scan_concurrency: int = 24,
        flight_read_port: AnomalyFlightReadPort | None = None,
        todo_write_port: AnomalyTodoWritePort | None = None,
        notify_port: AnomalyNotifyPort | None = None,
        notification_service: Any = None,
    ):
        if flight_read_port is None:
            raise ValueError("flight_read_port is required for AnomalyDetectionService")
        if todo_write_port is None:
            raise ValueError("todo_write_port is required for AnomalyDetectionService")
        if notify_port is None:
            raise ValueError("notify_port is required for AnomalyDetectionService")

        self._anomaly_repo = anomaly_repo
        self._alert_service = alert_service
        self._flight_read_port = flight_read_port
        self._todo_write_port = todo_write_port
        self._notify_port = notify_port
        self._notification_service = notification_service
        self._kpi_aggregation_service = kpi_aggregation_service
        self._scan_concurrency = max(1, int(scan_concurrency or 1))

    async def scan_all_active_flights(self) -> list[Anomaly]:
        """Scheduled full scan over active flights."""
        rules = await self._load_enabled_rules()
        if not rules:
            return []

        flights = await self._flight_read_port.get_active_flights(limit=500)
        flight_items = [item.get_flight() if hasattr(item, "get_flight") else item for item in flights]
        gate_conflicts_by_window = self._build_gate_conflicts_by_window(flight_items, rules)
        known_signatures = await self._load_existing_signatures(flight_items, rules)
        signature_lock = asyncio.Lock() if known_signatures is not None else None

        # KPI 规则是系统级的，不需要逐航班检查，提前标记为已评估
        kpi_rule_ids: set[str] = {rule.rule_id for rule in rules if rule.rule_type == AnomalyType.KPI_DEGRADATION.value}

        # 有界并行评估航班，避免对下游依赖和事件循环造成瞬时压力
        concurrency_limit = min(self._scan_concurrency, max(1, len(flight_items)))
        semaphore = asyncio.Semaphore(concurrency_limit)

        async def _evaluate_with_limit(flight: Any) -> list[Anomaly]:
            async with semaphore:
                return await self._evaluate_flight_with_rules(
                    flight,
                    rules,
                    evaluated_kpi_rules=set(kpi_rule_ids),
                    gate_conflicts_by_window=gate_conflicts_by_window,
                    signature_cache=known_signatures,
                    signature_cache_lock=signature_lock,
                )

        batch_results = await asyncio.gather(*(_evaluate_with_limit(flight) for flight in flight_items))

        anomalies: list[Anomaly] = []
        for batch in batch_results:
            anomalies.extend(batch)
        return anomalies

    async def evaluate_flight(self, flight_id: str) -> list[Anomaly]:
        """Evaluate one flight by id (event-driven path)."""
        aggregate = await self._flight_read_port.get_flight(flight_id)
        if not aggregate:
            return []
        flight = aggregate.get_flight() if hasattr(aggregate, "get_flight") else aggregate
        rules = await self._load_enabled_rules()
        # KPI degradation is system-level and handled by scheduled full scan.
        kpi_rule_ids = {rule.rule_id for rule in rules if rule.rule_type == AnomalyType.KPI_DEGRADATION.value}
        has_gate_rules = any(rule.enabled and rule.rule_type == AnomalyType.GATE_STAND_CONFLICT.value for rule in rules)
        gate_conflicts_by_window: dict[int, dict[str, dict[str, Any]]] = {}
        if has_gate_rules:
            flight_items = await self._load_local_conflict_candidates(flight, rules)
            if not flight_items:
                flight_items = [flight]
            gate_conflicts_by_window = self._build_gate_conflicts_by_window(flight_items, rules)
        signature_cache = await self._load_existing_signatures([flight], rules)
        signature_lock = asyncio.Lock() if signature_cache is not None else None

        return await self._evaluate_flight_with_rules(
            flight,
            rules,
            evaluated_kpi_rules=kpi_rule_ids,
            gate_conflicts_by_window=gate_conflicts_by_window,
            signature_cache=signature_cache,
            signature_cache_lock=signature_lock,
        )

    async def _load_local_conflict_candidates(
        self,
        flight: Any,
        rules: list[AnomalyRule],
    ) -> list[Any]:
        """Load nearby flights for conflict checks using query-side filters."""
        search_flights = getattr(self._flight_read_port, "search_flights", None)
        if not callable(search_flights):
            return []

        scheduled_departure = getattr(flight, "scheduled_departure", None)
        gate = self._safe_value(getattr(flight, "gate", None))
        stand = self._safe_value(getattr(flight, "stand", None))
        if scheduled_departure is None or (not gate and not stand):
            return []

        conflict_windows = [
            self._resolve_conflict_window(rule)
            for rule in rules
            if rule.enabled and rule.rule_type == AnomalyType.GATE_STAND_CONFLICT.value
        ]
        max_window_minutes = max(conflict_windows) if conflict_windows else 45
        window_start = scheduled_departure - timedelta(minutes=max_window_minutes)
        window_end = scheduled_departure + timedelta(minutes=max_window_minutes)

        flights_by_id: dict[str, Any] = {}

        async def _query_by_resource(resource_key: str, resource_value: str) -> None:
            criteria = {
                resource_key: resource_value,
                "scheduled_departure_from": window_start,
                "scheduled_departure_to": window_end,
            }
            results = await search_flights(criteria, limit=300, offset=0)
            for item in results:
                related_flight = item.get_flight() if hasattr(item, "get_flight") else item
                related_flight_id = self._extract_flight_id(related_flight)
                flights_by_id[related_flight_id] = related_flight

        if gate:
            await _query_by_resource("gate", gate)
        if stand:
            await _query_by_resource("stand", stand)

        flights_by_id[self._extract_flight_id(flight)] = flight
        return list(flights_by_id.values())

    async def _load_enabled_rules(self) -> list[AnomalyRule]:
        rules = await self._anomaly_repo.list_rules(enabled_only=True)
        if rules:
            return rules
        return [
            AnomalyRule(
                rule_id="service_node_timeout",
                rule_type=AnomalyType.SERVICE_NODE_TIMEOUT.value,
                name="Service node timeout",
                config={"minutes_after_arrival": 20},
                severity=AnomalySeverity.MEDIUM,
            ),
            AnomalyRule(
                rule_id="gate_stand_conflict",
                rule_type=AnomalyType.GATE_STAND_CONFLICT.value,
                name="Gate or stand conflict",
                config={"conflict_window_minutes": 45},
                severity=AnomalySeverity.HIGH,
            ),
        ]

    async def _evaluate_flight_with_rules(
        self,
        flight: Any,
        rules: list[AnomalyRule],
        evaluated_kpi_rules: set[str] | None = None,
        gate_conflicts_by_window: dict[int, dict[str, dict[str, Any]]] | None = None,
        signature_cache: set[tuple[str, str, str | None]] | None = None,
        signature_cache_lock: asyncio.Lock | None = None,
    ) -> list[Anomaly]:
        anomalies: list[Anomaly] = []
        for rule in rules:
            if not rule.enabled:
                continue

            anomaly: Anomaly | None = None
            if rule.rule_type == AnomalyType.SERVICE_NODE_TIMEOUT.value:
                anomaly = await self._check_service_node_timeout(
                    flight,
                    rule,
                    signature_cache=signature_cache,
                    signature_cache_lock=signature_cache_lock,
                )
            elif rule.rule_type == AnomalyType.GATE_STAND_CONFLICT.value:
                conflicts_for_window = None
                if gate_conflicts_by_window is not None:
                    conflict_window = self._resolve_conflict_window(rule)
                    conflicts_for_window = gate_conflicts_by_window.get(conflict_window)
                anomaly = await self._check_gate_stand_conflict(
                    flight,
                    rule,
                    precomputed_conflicts=conflicts_for_window,
                    signature_cache=signature_cache,
                    signature_cache_lock=signature_cache_lock,
                )
            elif rule.rule_type == AnomalyType.KPI_DEGRADATION.value:
                if evaluated_kpi_rules is not None and rule.rule_id in evaluated_kpi_rules:
                    continue
                anomaly = await self._check_kpi_degradation(
                    flight,
                    rule,
                    signature_cache=signature_cache,
                    signature_cache_lock=signature_cache_lock,
                )
                if evaluated_kpi_rules is not None:
                    evaluated_kpi_rules.add(rule.rule_id)

            if anomaly:
                anomalies.append(anomaly)

        return anomalies

    async def _check_kpi_degradation(
        self,
        flight: Any,
        rule: AnomalyRule,
        *,
        signature_cache: set[tuple[str, str, str | None]] | None = None,
        signature_cache_lock: asyncio.Lock | None = None,
    ) -> Anomaly | None:
        """Rule: KPI metric value falls below configured threshold."""
        if self._kpi_aggregation_service is None:
            logger.debug("Skip KPI degradation check: kpi_aggregation_service is unavailable")
            return None

        config = rule.config or {}
        metric = str(config.get("metric") or "on_time_departure_rate").strip()
        threshold = float(config.get("threshold") or 0.7)
        window_hours = max(1, int(config.get("window_hours") or 4))

        now = utc_now()
        start_date = (now - timedelta(hours=window_hours)).date()
        end_date = now.date()

        try:
            snapshot = await self._kpi_aggregation_service.get_kpi_snapshot(
                time_range="custom",
                start_date=start_date,
                end_date=end_date,
            )
        except Exception as exc:  # noqa: BLE001 - KPI snapshot aggregation may fail in various ways
            logger.warning(f"KPI degradation check failed while loading snapshot: {exc}")
            return None

        current_value_raw = getattr(snapshot, metric, None)
        if current_value_raw is None:
            logger.warning(f"KPI degradation check skipped: metric '{metric}' not found in snapshot")
            return None

        current_value = float(current_value_raw)
        if current_value >= threshold:
            return None

        flight_id = self._extract_flight_id(flight)
        flight_no = self._extract_flight_number(flight)
        description = (
            f"Metric {metric} dropped to {current_value:.3f} in last {window_hours}h, below threshold {threshold:.3f}."
        )
        context = {
            "flight_number": flight_no,
            "inbound_leg": {
                "flight_no": self._extract_leg_flight_no(flight, "inbound"),
            },
            "outbound_leg": {
                "flight_no": self._extract_leg_flight_no(flight, "outbound"),
            },
            "metric": metric,
            "current_value": current_value,
            "threshold": threshold,
            "window_hours": window_hours,
            "time_range": {
                "start_date": start_date.isoformat(),
                "end_date": end_date.isoformat(),
            },
        }

        return await self._create_anomaly(
            flight_id=flight_id,
            anomaly_type=AnomalyType.KPI_DEGRADATION,
            severity=rule.severity,
            title=f"KPI degradation: {metric}",
            description=description,
            rule=rule,
            context_data=context,
            signature_cache=signature_cache,
            signature_cache_lock=signature_cache_lock,
        )

    async def _check_service_node_timeout(
        self,
        flight: Any,
        rule: AnomalyRule,
        *,
        signature_cache: set[tuple[str, str, str | None]] | None = None,
        signature_cache_lock: asyncio.Lock | None = None,
    ) -> Anomaly | None:
        """Rule: arrived flight without cleaning start over threshold."""
        arrival_at = getattr(flight, "actual_arrival", None)
        cleaning_start = getattr(flight, "cleaning_start_time", None)
        if arrival_at is None or cleaning_start is not None:
            return None

        threshold = int((rule.config or {}).get("minutes_after_arrival", 20) or 20)
        now = now_in_tz(arrival_at.tzinfo) if hasattr(arrival_at, "tzinfo") else utc_now()
        elapsed_minutes = (now - arrival_at).total_seconds() / 60
        if elapsed_minutes < threshold:
            return None

        flight_id = self._extract_flight_id(flight)
        flight_no = self._extract_flight_number(flight)
        title = f"Service node timeout: {flight_no}"
        description = (
            f"Cleaning has not started for {round(elapsed_minutes)} minutes after arrival. "
            f"Threshold is {threshold} minutes."
        )
        context = {
            "flight_number": flight_no,
            "inbound_leg": {
                "flight_no": self._extract_leg_flight_no(flight, "inbound"),
            },
            "outbound_leg": {
                "flight_no": self._extract_leg_flight_no(flight, "outbound"),
            },
            "elapsed_minutes": round(elapsed_minutes, 2),
            "threshold_minutes": threshold,
            "actual_arrival": arrival_at.isoformat() if hasattr(arrival_at, "isoformat") else str(arrival_at),
        }
        return await self._create_anomaly(
            flight_id=flight_id,
            anomaly_type=AnomalyType.SERVICE_NODE_TIMEOUT,
            severity=rule.severity,
            title=title,
            description=description,
            rule=rule,
            context_data=context,
            signature_cache=signature_cache,
            signature_cache_lock=signature_cache_lock,
        )

    async def _check_gate_stand_conflict(
        self,
        flight: Any,
        rule: AnomalyRule,
        precomputed_conflicts: dict[str, dict[str, Any]] | None = None,
        signature_cache: set[tuple[str, str, str | None]] | None = None,
        signature_cache_lock: asyncio.Lock | None = None,
    ) -> Anomaly | None:
        """Rule: gate/stand overlapping in close departure window."""
        gate = self._safe_value(getattr(flight, "gate", None))
        stand = self._safe_value(getattr(flight, "stand", None))
        scheduled_departure = getattr(flight, "scheduled_departure", None)
        if not gate and not stand:
            return None
        if scheduled_departure is None:
            return None

        current_flight_id = self._extract_flight_id(flight)
        conflict_window = self._resolve_conflict_window(rule)

        if precomputed_conflicts is not None:
            conflict = precomputed_conflicts.get(current_flight_id)
            if not conflict:
                return None

            flight_no = self._extract_flight_number(flight)
            description = (
                f"Flight {flight_no} conflicts with {conflict['other_flight_number']} "
                f"on shared {conflict['resource_type']} {conflict['resource_value']}. "
                f"Departure window overlap is {round(conflict['window_minutes'])} minutes."
            )
            context = {
                "flight_number": flight_no,
                "inbound_leg": {
                    "flight_no": self._extract_leg_flight_no(flight, "inbound"),
                },
                "outbound_leg": {
                    "flight_no": self._extract_leg_flight_no(flight, "outbound"),
                },
                "other_flight_number": conflict["other_flight_number"],
                "resource_type": conflict["resource_type"],
                "resource_value": conflict["resource_value"],
                "window_minutes": conflict["window_minutes"],
                "threshold_minutes": conflict["threshold_minutes"],
            }
            return await self._create_anomaly(
                flight_id=current_flight_id,
                anomaly_type=AnomalyType.GATE_STAND_CONFLICT,
                severity=rule.severity,
                title=f"Gate/Stand conflict: {flight_no}",
                description=description,
                rule=rule,
                context_data=context,
                signature_cache=signature_cache,
                signature_cache_lock=signature_cache_lock,
            )

        logger.debug(
            "skip gate/stand conflict check without precomputed candidates "
            f"flight_id={current_flight_id} conflict_window={conflict_window}"
        )
        return None

    def _build_gate_conflicts_by_window(
        self,
        flights: list[Any],
        rules: list[AnomalyRule],
    ) -> dict[int, dict[str, dict[str, Any]]]:
        windows = sorted(
            {
                self._resolve_conflict_window(rule)
                for rule in rules
                if rule.enabled and rule.rule_type == AnomalyType.GATE_STAND_CONFLICT.value
            }
        )
        if not windows:
            return {}

        return {window: self._build_gate_stand_conflicts(flights, window) for window in windows}

    @staticmethod
    def _resolve_conflict_window(rule: AnomalyRule) -> int:
        return max(1, int((rule.config or {}).get("conflict_window_minutes", 45) or 45))

    def _build_gate_stand_conflicts(
        self,
        flights: list[Any],
        conflict_window_minutes: int,
    ) -> dict[str, dict[str, Any]]:
        prepared: list[dict[str, Any]] = []
        for flight in flights:
            scheduled_departure = getattr(flight, "scheduled_departure", None)
            if scheduled_departure is None:
                continue

            prepared.append(
                {
                    "flight_id": self._extract_flight_id(flight),
                    "flight_no": self._extract_flight_number(flight),
                    "scheduled_departure": scheduled_departure,
                    "gate": self._safe_value(getattr(flight, "gate", None)),
                    "stand": self._safe_value(getattr(flight, "stand", None)),
                }
            )

        grouped_resources: dict[str, dict[str, list[dict[str, Any]]]] = {
            "gate": {},
            "stand": {},
        }

        for item in prepared:
            for resource_type in ("gate", "stand"):
                resource_value = item.get(resource_type)
                if not resource_value:
                    continue
                grouped_resources[resource_type].setdefault(resource_value, []).append(item)

        conflicts: dict[str, dict[str, Any]] = {}
        threshold_seconds = conflict_window_minutes * 60

        for resource_type, resource_map in grouped_resources.items():
            for resource_value, resource_flights in resource_map.items():
                if len(resource_flights) < 2:
                    continue

                ordered = sorted(resource_flights, key=lambda x: x["scheduled_departure"])
                left = 0
                for right in range(1, len(ordered)):
                    current = ordered[right]
                    while left < right:
                        diff_seconds = (
                            current["scheduled_departure"] - ordered[left]["scheduled_departure"]
                        ).total_seconds()
                        if diff_seconds <= threshold_seconds:
                            break
                        left += 1

                    for idx in range(left, right):
                        other = ordered[idx]
                        window_minutes = (
                            abs((current["scheduled_departure"] - other["scheduled_departure"]).total_seconds()) / 60
                        )
                        if window_minutes > conflict_window_minutes:
                            continue

                        self._record_conflict_hint(
                            conflicts,
                            source=current,
                            target=other,
                            resource_type=resource_type,
                            resource_value=resource_value,
                            window_minutes=window_minutes,
                            threshold_minutes=conflict_window_minutes,
                        )
                        self._record_conflict_hint(
                            conflicts,
                            source=other,
                            target=current,
                            resource_type=resource_type,
                            resource_value=resource_value,
                            window_minutes=window_minutes,
                            threshold_minutes=conflict_window_minutes,
                        )

        return conflicts

    @staticmethod
    def _record_conflict_hint(
        conflicts: dict[str, dict[str, Any]],
        *,
        source: dict[str, Any],
        target: dict[str, Any],
        resource_type: str,
        resource_value: str,
        window_minutes: float,
        threshold_minutes: int,
    ) -> None:
        source_id = str(source.get("flight_id") or "")
        target_id = str(target.get("flight_id") or "")
        if not source_id or not target_id or source_id == target_id:
            return

        candidate = {
            "other_flight_id": target_id,
            "other_flight_number": str(target.get("flight_no") or "UNKNOWN"),
            "resource_type": resource_type,
            "resource_value": resource_value,
            "window_minutes": round(float(window_minutes), 2),
            "threshold_minutes": int(threshold_minutes),
        }
        existing = conflicts.get(source_id)
        if existing is None or candidate["window_minutes"] < existing["window_minutes"]:
            conflicts[source_id] = candidate

    async def _create_anomaly(
        self,
        *,
        flight_id: str,
        anomaly_type: AnomalyType,
        severity: AnomalySeverity,
        title: str,
        description: str,
        rule: AnomalyRule,
        context_data: dict[str, Any],
        signature_cache: set[tuple[str, str, str | None]] | None = None,
        signature_cache_lock: asyncio.Lock | None = None,
    ) -> Anomaly | None:
        signature = (str(flight_id), anomaly_type.value, str(rule.rule_id).strip() if rule.rule_id else None)
        should_check_duplicate = True
        if signature_cache is not None and signature_cache_lock is not None:
            async with signature_cache_lock:
                if signature in signature_cache:
                    return None
                signature_cache.add(signature)

            # 已通过批量预热缓存确认过 open signature，常规路径可避免再次往返数据库。
            should_check_duplicate = False

        if should_check_duplicate:
            duplicate = await self._anomaly_repo.find_open_by_signature(
                flight_id=flight_id,
                anomaly_type=anomaly_type.value,
                rule_id=rule.rule_id,
            )
            if duplicate:
                return None

        anomaly = Anomaly(
            anomaly_id=generate_id(),
            flight_id=flight_id,
            anomaly_type=anomaly_type,
            severity=severity,
            title=title,
            description=description,
            rule_id=rule.rule_id,
            context_data=context_data,
        )
        persisted = await self._anomaly_repo.create_anomaly(anomaly)

        if rule.auto_create_todo:
            todo_id = await self._create_anomaly_todo(persisted, rule)
            if todo_id:
                persisted.linked_todo_id = todo_id

        await self._publish_anomaly_event("anomaly_created", persisted)
        await self._send_anomaly_notification(persisted)
        return persisted

    async def _load_existing_signatures(
        self,
        flights: list[Any],
        rules: list[AnomalyRule],
    ) -> set[tuple[str, str, str | None]] | None:
        loader = getattr(self._anomaly_repo, "list_open_signatures", None)
        if not callable(loader):
            return None

        flight_ids = [self._extract_flight_id(flight) for flight in flights]
        rule_ids = [str(rule.rule_id).strip() for rule in rules if getattr(rule, "rule_id", None)]
        anomaly_types = [str(rule.rule_type).strip() for rule in rules if getattr(rule, "rule_type", None)]

        try:
            signatures = await loader(
                flight_ids=flight_ids,
                rule_ids=rule_ids,
                anomaly_types=anomaly_types,
            )
            if isinstance(signatures, set):
                return signatures
            return set(signatures or [])
        except Exception as exc:  # noqa: BLE001 - anomaly signature loader may fail in various ways
            logger.warning(f"load open anomaly signatures failed: {exc}")
            return None

    async def _create_anomaly_todo(self, anomaly: Anomaly, rule: AnomalyRule) -> str | None:
        """Create TODO task for anomaly."""
        if self._todo_write_port is None:
            return None

        todo_id = await self._todo_write_port.create_anomaly_todo(anomaly, rule)
        if not todo_id:
            return None
        linked = await self._anomaly_repo.update_linked_todo(anomaly.anomaly_id, todo_id)
        return todo_id if linked else None

    async def _publish_anomaly_event(self, event_type: str, anomaly: Anomaly) -> None:
        if self._notify_port is None:
            return
        payload = build_anomaly_payload(event_type, anomaly)
        try:
            await self._notify_port.publish("anomaly_alerts", payload)
        except Exception as exc:  # noqa: BLE001 - notification publish must not break detection
            logger.warning(f"Failed to publish anomaly event: {exc}")

    async def _send_anomaly_notification(self, anomaly: Anomaly) -> None:
        """Send in-app notification for anomaly creation with graceful degradation."""
        if self._notification_service is None:
            return

        recipient_user_id = self._resolve_notification_recipient(anomaly)
        if not recipient_user_id:
            logger.debug(f"Skipping anomaly notification: no recipient resolved for anomaly_id={anomaly.anomaly_id}")
            return

        severity_value = anomaly.severity.value if hasattr(anomaly.severity, "value") else str(anomaly.severity)
        try:
            await self._notification_service.send(
                user_id=recipient_user_id,
                title=anomaly.title,
                body=anomaly.description,
                category="anomaly",
                severity=severity_value,
                flight_id=anomaly.flight_id,
                related_entity_type="anomaly",
                related_entity_id=anomaly.anomaly_id,
            )
        except Exception as exc:  # noqa: BLE001 - notification send must not break detection
            logger.warning(f"Failed to send anomaly notification for anomaly_id={anomaly.anomaly_id}: {exc}")

    @staticmethod
    def _resolve_notification_recipient(anomaly: Anomaly) -> str | None:
        """Resolve notification recipient from anomaly context data.

        Returns user_id if found in context, None otherwise.
        Prefers low-risk recipient from existing anomaly context.
        """
        context = anomaly.context_data or {}
        candidate_keys = ("assigned_to", "assignee", "notify_user", "recipient_user_id", "user_id")
        for key in candidate_keys:
            value = context.get(key)
            if value:
                text = str(value).strip()
                if text:
                    return text
        return None

    @staticmethod
    def _safe_value(value: Any) -> str | None:
        if value is None:
            return None
        raw = value.value if hasattr(value, "value") else value
        text = str(raw).strip()
        return text or None

    @staticmethod
    def _extract_flight_id(flight: Any) -> str:
        raw = getattr(flight, "flight_id", None)
        if hasattr(raw, "value"):
            return str(raw.value)
        return str(raw)

    @staticmethod
    def _extract_flight_number(flight: Any) -> str:
        for leg_attr in ("outbound_leg", "inbound_leg"):
            leg = getattr(flight, leg_attr, None)
            if leg is None:
                continue
            leg_no = getattr(leg, "flight_no", None)
            if leg_no:
                text = str(leg_no).strip()
                if text:
                    return text

        for field_name in ("flight_number",):
            value = getattr(flight, field_name, None)
            if value is None:
                continue
            if hasattr(value, "value"):
                text = str(value.value).strip()
            else:
                text = str(value).strip()
            if text:
                return text
        return "UNKNOWN"

    @classmethod
    def _extract_leg_flight_no(cls, flight: Any, leg_type: str) -> str | None:
        normalized = str(leg_type or "").strip().lower()
        if normalized not in {"inbound", "outbound"}:
            return None
        leg = getattr(flight, f"{normalized}_leg", None)
        if leg is not None:
            leg_no = cls._safe_value(getattr(leg, "flight_no", None))
            if leg_no:
                return leg_no
        return None
