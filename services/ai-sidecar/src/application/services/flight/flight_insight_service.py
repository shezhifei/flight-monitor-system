"""Flight insight generation service for reports and event journeys."""

from __future__ import annotations

import json
import logging
from datetime import datetime, timedelta
from typing import Any, ClassVar

from src.domain.utils.time_utils import utc_now
from src.infrastructure.ai.ai_entity import AIEntity, AIEntityConfig
from src.infrastructure.ai.config_store import AIConfigStoreInterface
from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
from src.infrastructure.ai.openai_client import Message, MessageRole
from src.infrastructure.ai.responses_adapter import (
    extract_message_content as _extract_content_fn,
)
from src.infrastructure.ai.responses_adapter import (
    normalize_api_format as _normalize_fn,
)

logger = logging.getLogger(__name__)


class AIUnavailableError(RuntimeError):
    """Raised when AI runtime is not available."""


class FlightInsightService:
    """Generate AI-assisted history reports and event journey summaries."""

    KEY_FIELDS: ClassVar[set[str]] = {
        "status",
        "scheduled_departure",
        "estimated_departure",
        "actual_departure",
        "scheduled_arrival",
        "estimated_arrival",
        "actual_arrival",
        "stand",
        "gate",
        "cobt_time",
        "on_blocks_time",
        "boarding_allowed_time",
        "start_boarding_time",
        "passenger_ready_time",
        "end_boarding_time",
        "off_blocks_time",
    }

    AI_TIMEOUT_SECONDS = 20.0
    AI_MAX_RETRIES = 1
    AI_TEMPERATURE = 0.2
    AI_MAX_TOKENS = 2200

    def __init__(self, history_service, flight_service, ai_config_store: AIConfigStoreInterface | None):
        self._history_service = history_service
        self._flight_service = flight_service
        self._ai_config_store = ai_config_store

    async def generate_history_report(
        self,
        flight_id: str,
        hours: int,
        incident_type: str | None,
        user_id: str,
    ) -> dict[str, Any]:
        """Generate flight history report with markdown + json payload."""
        flight = await self._flight_service.find_by_flight_id(flight_id)
        if not flight:
            raise ValueError(f"航班不存在: {flight_id}")

        flight_number = self._resolve_flight_number(flight)
        window_end = utc_now()
        window_start = window_end - timedelta(hours=hours)

        history_records = await self._history_service.get_flight_update_history(
            flight_id=flight_id,
            start_time=window_start,
            end_time=window_end,
            page=1,
            page_size=500,
        )
        timeline = self._build_history_timeline(history_records or [])

        if incident_type and incident_type.strip():
            timeline = self._filter_timeline_by_keyword(timeline, incident_type.strip())

        summary = self._build_history_summary(timeline)
        ai_entity, model_name = await self._build_ai_entity()
        if ai_entity is None:
            raise AIUnavailableError("AI 不可用：未检测到有效 AI 配置")

        report_markdown, response_model = await self._generate_history_markdown(
            ai_entity=ai_entity,
            model=model_name,
            flight_id=flight_id,
            flight_number=flight_number,
            window_start=window_start,
            window_end=window_end,
            timeline=timeline,
            summary=summary,
            incident_type=incident_type,
            user_id=user_id,
        )
        resolved_model = response_model or model_name or "unknown-model"

        report_json = {
            "flight_id": flight_id,
            "flight_number": flight_number,
            "window_start": window_start.isoformat(),
            "window_end": window_end.isoformat(),
            "timeline": timeline,
            "summary": summary,
            "incident_type": incident_type,
            "generated_by": user_id,
        }
        return {
            "flight_id": flight_id,
            "flight_number": flight_number,
            "window_start": window_start.isoformat(),
            "window_end": window_end.isoformat(),
            "timeline": timeline,
            "summary": summary,
            "report_markdown": report_markdown,
            "report_json": report_json,
            "generated_at": utc_now().isoformat(),
            "model": resolved_model,
        }

    async def generate_event_journey(
        self,
        flight_id: str,
        hours: int,
        user_id: str,
    ) -> dict[str, Any]:
        """Generate merged event journey from business cases + history changes."""
        flight = await self._flight_service.find_by_flight_id(flight_id)
        if not flight:
            raise ValueError(f"航班不存在: {flight_id}")

        window_end = utc_now()
        window_start = window_end - timedelta(hours=hours)

        history_records = await self._history_service.get_flight_update_history(
            flight_id=flight_id,
            start_time=window_start,
            end_time=window_end,
            page=1,
            page_size=500,
        )
        flight_change_timeline = self._build_history_timeline(history_records or [])

        cases_map = await self._flight_service.batch_get_business_cases([flight_id])
        raw_cases = (cases_map or {}).get(flight_id, [])
        business_case_timeline = self._build_business_case_timeline(raw_cases, window_start, window_end)

        merged_timeline = sorted(
            business_case_timeline + flight_change_timeline,
            key=lambda item: item.get("_dt") or datetime.min,
        )

        ai_entity, model_name = await self._build_ai_entity()
        if ai_entity is None:
            raise AIUnavailableError("AI 不可用：未检测到有效 AI 配置")

        flight_number = self._resolve_flight_number(flight)
        journey_markdown, response_model = await self._generate_journey_markdown(
            ai_entity=ai_entity,
            model=model_name,
            flight_id=flight_id,
            flight_number=flight_number,
            window_start=window_start,
            window_end=window_end,
            business_case_timeline=business_case_timeline,
            flight_change_timeline=flight_change_timeline,
            merged_timeline=merged_timeline,
            user_id=user_id,
        )
        resolved_model = response_model or model_name or "unknown-model"

        journey_json = {
            "flight_id": flight_id,
            "flight_number": flight_number,
            "window_start": window_start.isoformat(),
            "window_end": window_end.isoformat(),
            "business_case_timeline": self._strip_internal_fields(business_case_timeline),
            "flight_change_timeline": self._strip_internal_fields(flight_change_timeline),
            "merged_timeline": self._strip_internal_fields(merged_timeline),
            "generated_by": user_id,
        }

        return {
            "flight_id": flight_id,
            "flight_number": flight_number,
            "window_start": window_start.isoformat(),
            "window_end": window_end.isoformat(),
            "business_case_timeline": self._strip_internal_fields(business_case_timeline),
            "flight_change_timeline": self._strip_internal_fields(flight_change_timeline),
            "merged_timeline": self._strip_internal_fields(merged_timeline),
            "journey_markdown": journey_markdown,
            "journey_json": journey_json,
            "generated_at": utc_now().isoformat(),
            "model": resolved_model,
        }

    async def _generate_history_markdown(
        self,
        *,
        ai_entity: AIEntity,
        model: str,
        flight_id: str,
        flight_number: str,
        window_start: datetime,
        window_end: datetime,
        timeline: list[dict[str, Any]],
        summary: dict[str, int],
        incident_type: str | None,
        user_id: str,
    ) -> tuple[str, str | None]:
        payload = {
            "flight_id": flight_id,
            "flight_number": flight_number,
            "window_start": window_start.isoformat(),
            "window_end": window_end.isoformat(),
            "incident_type": incident_type,
            "summary": summary,
            "timeline": self._strip_internal_fields(timeline),
            "requested_by": user_id,
        }
        response = await self._request_ai(
            ai_entity=ai_entity,
            system_prompt=(
                "你是机场运行复盘分析助手。"
                "请输出 JSON 对象，包含 markdown 字段。"
                "markdown 需包括：概览、关键时间线、风险提示、建议。"
            ),
            user_content=f"请根据以下数据生成航班动态/历史报表：\n{json.dumps(payload, ensure_ascii=False)}",
            model=model,
            temperature=self.AI_TEMPERATURE,
            max_tokens=self.AI_MAX_TOKENS,
            expect_json=True,
        )
        response_model = getattr(response, "model", None)
        if response_model is None and isinstance(response, dict):
            response_model = response.get("model")
        content = self._extract_message_content(response)
        parsed = self._parse_ai_json(content)
        markdown = str(parsed.get("markdown") or "").strip()
        if markdown:
            return markdown, response_model

        if content.strip().startswith("#"):
            return content.strip(), response_model

        return self._fallback_history_markdown(
            flight_id=flight_id,
            flight_number=flight_number,
            window_start=window_start,
            window_end=window_end,
            summary=summary,
        ), response_model

    async def _generate_journey_markdown(
        self,
        *,
        ai_entity: AIEntity,
        model: str,
        flight_id: str,
        flight_number: str,
        window_start: datetime,
        window_end: datetime,
        business_case_timeline: list[dict[str, Any]],
        flight_change_timeline: list[dict[str, Any]],
        merged_timeline: list[dict[str, Any]],
        user_id: str,
    ) -> tuple[str, str | None]:
        payload = {
            "flight_id": flight_id,
            "flight_number": flight_number,
            "window_start": window_start.isoformat(),
            "window_end": window_end.isoformat(),
            "business_case_timeline": self._strip_internal_fields(business_case_timeline),
            "flight_change_timeline": self._strip_internal_fields(flight_change_timeline),
            "merged_timeline": self._strip_internal_fields(merged_timeline),
            "requested_by": user_id,
        }
        response = await self._request_ai(
            ai_entity=ai_entity,
            system_prompt=(
                "你是机场运行事件叙事分析助手。"
                "请输出 JSON 对象，包含 markdown 字段。"
                "markdown 必须按时间顺序描述事件经过，并明确主线（业务事项）和补充线（航班变更）。"
            ),
            user_content=f"请根据以下数据生成事件经过：\n{json.dumps(payload, ensure_ascii=False)}",
            model=model,
            temperature=self.AI_TEMPERATURE,
            max_tokens=self.AI_MAX_TOKENS,
            expect_json=True,
        )
        response_model = getattr(response, "model", None)
        if response_model is None and isinstance(response, dict):
            response_model = response.get("model")
        content = self._extract_message_content(response)
        parsed = self._parse_ai_json(content)
        markdown = str(parsed.get("markdown") or "").strip()
        if markdown:
            return markdown, response_model
        if content.strip().startswith("#"):
            return content.strip(), response_model
        return self._fallback_journey_markdown(
            flight_id=flight_id,
            flight_number=flight_number,
            window_start=window_start,
            window_end=window_end,
            merged_count=len(merged_timeline),
        ), response_model

    async def _build_ai_entity(self) -> tuple[AIEntity | None, str | None]:
        entity_id, config = await self._resolve_ai_config()
        if not config:
            return None, None

        entity_config = AIEntityConfig(
            api_key=str(config.get("api_key") or "").strip(),
            base_url=str(config.get("base_url") or "https://api.openai.com/v1"),
            default_model=str(config.get("default_model") or "gpt-4o-mini"),
            api_format=self._normalize_api_format(config.get("api_format")),
            temperature=self.AI_TEMPERATURE,
            max_tokens=int(config.get("max_tokens", self.AI_MAX_TOKENS) or self.AI_MAX_TOKENS),
            timeout=self.AI_TIMEOUT_SECONDS,
            max_retries=self.AI_MAX_RETRIES,
            retry_delay=float(config.get("retry_delay", 0.5) or 0.5),
            system_prompt=str(config.get("system_prompt") or "") or None,
        )
        entity = AIEntity(config=entity_config, entity_id=f"flight_insight_{entity_id or 'default'}")
        await entity._ensure_initialized()
        return entity, entity_config.default_model

    async def _resolve_ai_config(self) -> tuple[str | None, dict | None]:
        store = self._ai_config_store
        if store is None:
            return None, None

        try:
            all_configs = await store.get_all()
        except Exception as exc:  # noqa: BLE001 - AI config store read may fail in various ways
            logger.warning("reading AI config store failed", exc_info=exc)
            return None, None

        if not isinstance(all_configs, dict) or not all_configs:
            return None, None

        default_config = all_configs.get("default")
        if isinstance(default_config, dict) and str(default_config.get("api_key") or "").strip():
            return "default", default_config

        for entity_id, config in all_configs.items():
            if not isinstance(config, dict):
                continue
            if str(config.get("api_key") or "").strip():
                return str(entity_id), config

        return None, None

    def _build_history_timeline(self, history_records: list[dict]) -> list[dict[str, Any]]:
        timeline: list[dict[str, Any]] = []
        for record in history_records:
            event = self._normalize_history_record(record)
            if event:
                timeline.append(event)
        return sorted(timeline, key=lambda item: item.get("_dt") or datetime.min)

    def _build_business_case_timeline(
        self,
        cases: list[Any],
        window_start: datetime,
        window_end: datetime,
    ) -> list[dict[str, Any]]:
        timeline: list[dict[str, Any]] = []
        for case in cases or []:
            case_dict = self._case_to_dict(case)
            created_dt = self._to_datetime(case_dict.get("created_at"))
            if created_dt and window_start <= created_dt <= window_end:
                timeline.append(
                    self._build_event(
                        dt=created_dt,
                        source="business_case",
                        title=f"事项创建：{case_dict.get('case_type') or '未命名事项'}",
                        detail=case_dict.get("description") or "无描述",
                        severity="medium" if case_dict.get("status") in {"FAILED", "DEAD_LETTER"} else "low",
                        raw=case_dict,
                    )
                )

            finished_dt = self._to_datetime(case_dict.get("finished_at"))
            if finished_dt and window_start <= finished_dt <= window_end:
                timeline.append(
                    self._build_event(
                        dt=finished_dt,
                        source="business_case",
                        title=f"事项完成：{case_dict.get('case_type') or '未命名事项'}",
                        detail=f"状态: {case_dict.get('status') or 'UNKNOWN'}",
                        severity="low",
                        raw=case_dict,
                    )
                )

            cancelled_dt = self._to_datetime(case_dict.get("cancelled_at"))
            if cancelled_dt and window_start <= cancelled_dt <= window_end:
                timeline.append(
                    self._build_event(
                        dt=cancelled_dt,
                        source="business_case",
                        title=f"事项取消：{case_dict.get('case_type') or '未命名事项'}",
                        detail=case_dict.get("description") or "事项取消",
                        severity="high",
                        raw=case_dict,
                    )
                )

        return sorted(timeline, key=lambda item: item.get("_dt") or datetime.min)

    def _normalize_history_record(self, record: dict[str, Any]) -> dict[str, Any] | None:
        if not isinstance(record, dict):
            return None
        dt = self._to_datetime(record.get("timestamp") or record.get("created_at"))
        if dt is None:
            return None

        changes = record.get("changes") if isinstance(record.get("changes"), dict) else {}
        old_values = changes.get("old") if isinstance(changes.get("old"), dict) else {}
        new_values = changes.get("new") if isinstance(changes.get("new"), dict) else {}

        fields = []
        raw_fields = changes.get("fields")
        if isinstance(raw_fields, list):
            fields = [str(item).strip() for item in raw_fields if str(item).strip()]
        if not fields:
            fields = list({*old_values.keys(), *new_values.keys()})

        fields = [field for field in fields if field]
        status_changed = "status" in fields
        key_field_count = len([field for field in fields if field in self.KEY_FIELDS])
        source = str(record.get("entity_type") or "flight_history")
        actor = str(record.get("user_id") or "system")
        operation = str(record.get("operation") or record.get("action") or "UPDATE")

        title = f"{operation}: {', '.join(fields[:3])}" if fields else operation
        detail = self._build_history_detail(fields, old_values, new_values, actor)

        severity = "low"
        if status_changed:
            severity = "high"
        elif key_field_count > 0:
            severity = "medium"

        raw = self._sanitize_json(
            {
                **record,
                "meta": {
                    "status_changed": status_changed,
                    "key_field_change_count": key_field_count,
                },
            }
        )

        return self._build_event(
            dt=dt,
            source=source,
            title=title,
            detail=detail,
            severity=severity,
            raw=raw,
        )

    @staticmethod
    def _build_history_detail(
        fields: list[str],
        old_values: dict[str, Any],
        new_values: dict[str, Any],
        actor: str,
    ) -> str:
        if not fields:
            return f"操作人: {actor}"

        fragments: list[str] = []
        for field_name in fields[:6]:
            old_val = old_values.get(field_name)
            new_val = new_values.get(field_name)
            if old_val == new_val:
                fragments.append(f"{field_name}={new_val}")
            else:
                fragments.append(f"{field_name}: {old_val} -> {new_val}")
        return f"{'; '.join(fragments)}; 操作人: {actor}"

    @staticmethod
    def _build_history_summary(timeline: list[dict[str, Any]]) -> dict[str, int]:
        total_events = len(timeline)
        status_changes = 0
        key_field_changes = 0
        for event in timeline:
            meta = ((event.get("raw") or {}).get("meta") or {}) if isinstance(event.get("raw"), dict) else {}
            if meta.get("status_changed"):
                status_changes += 1
            if int(meta.get("key_field_change_count") or 0) > 0:
                key_field_changes += 1

        return {
            "total_events": total_events,
            "status_changes": status_changes,
            "key_field_changes": key_field_changes,
        }

    @staticmethod
    def _filter_timeline_by_keyword(timeline: list[dict[str, Any]], keyword: str) -> list[dict[str, Any]]:
        lowered = keyword.lower()
        filtered: list[dict[str, Any]] = []
        for event in timeline:
            blob = " ".join(
                [
                    str(event.get("title") or ""),
                    str(event.get("detail") or ""),
                    json.dumps(event.get("raw") or {}, ensure_ascii=False),
                ]
            ).lower()
            if lowered in blob:
                filtered.append(event)
        return filtered

    @staticmethod
    def _build_event(
        *,
        dt: datetime,
        source: str,
        title: str,
        detail: str,
        severity: str,
        raw: dict[str, Any],
    ) -> dict[str, Any]:
        return {
            "timestamp": dt.isoformat(),
            "source": source,
            "title": title,
            "detail": detail,
            "severity": severity,
            "raw": raw,
            "_dt": dt,
        }

    @staticmethod
    def _extract_message_content(response: object) -> str:
        return _extract_content_fn(response)

    async def _request_ai(
        self,
        *,
        ai_entity: AIEntity,
        system_prompt: str,
        user_content: str,
        model: str,
        temperature: float,
        max_tokens: int,
        expect_json: bool,
    ) -> object:
        runner = LLMStreamRunner(ai_entity._ai_client)
        api_format = self._normalize_api_format(getattr(ai_entity.config, "api_format", "chat_completions"))
        if api_format == "responses":
            result = await runner.run_responses(
                model=model,
                instructions=system_prompt,
                input=[{"role": "user", "content": user_content}],
                temperature=temperature,
                max_output_tokens=max_tokens,
            )
            return result.raw_response or result

        request_kwargs: dict[str, Any] = {}
        if expect_json:
            request_kwargs["response_format"] = {"type": "json_object"}
        result = await runner.run_chat(
            messages=[
                Message(role=MessageRole.SYSTEM, content=system_prompt),
                Message(role=MessageRole.USER, content=user_content),
            ],
            model=model,
            temperature=temperature,
            max_tokens=max_tokens,
            **request_kwargs,
        )
        return result.raw_response or result

    @staticmethod
    def _normalize_api_format(api_format: Any) -> str:
        return _normalize_fn(api_format)

    @staticmethod
    def _parse_ai_json(content: str) -> dict[str, Any]:
        if not content:
            return {}
        try:
            payload = json.loads(content)
            return payload if isinstance(payload, dict) else {}
        except json.JSONDecodeError:
            return {}

    @staticmethod
    def _to_datetime(value: Any) -> datetime | None:
        if value is None:
            return None
        if isinstance(value, datetime):
            return value
        if hasattr(value, "isoformat") and hasattr(value, "year"):
            try:
                return datetime(
                    value.year,
                    value.month,
                    value.day,
                    getattr(value, "hour", 0),
                    getattr(value, "minute", 0),
                    getattr(value, "second", 0),
                    getattr(value, "microsecond", 0),
                    tzinfo=getattr(value, "tzinfo", None),
                )
            except Exception as exc:  # noqa: BLE001 - datetime construction may fail in various ways
                logger.warning("datetime construction failed; returning None: %s", exc)
                return None

        text = str(value).strip()
        if not text:
            return None
        if text.endswith("Z"):
            text = text[:-1] + "+00:00"
        try:
            return datetime.fromisoformat(text)
        except ValueError:
            return None

    @staticmethod
    def _resolve_flight_number(flight: Any) -> str:
        outbound_leg = getattr(flight, "outbound_leg", None)
        inbound_leg = getattr(flight, "inbound_leg", None)
        candidates = [
            getattr(flight, "flight_number", None),
            getattr(outbound_leg, "flight_no", None) if outbound_leg is not None else None,
            getattr(inbound_leg, "flight_no", None) if inbound_leg is not None else None,
            getattr(flight, "flight_id", None),
        ]
        for candidate in candidates:
            normalized = FlightInsightService._normalize_value(candidate)
            if normalized:
                return normalized
        return "UNKNOWN"

    @staticmethod
    def _normalize_value(value: Any) -> str:
        if value is None:
            return ""
        actual = getattr(value, "value", value)
        if actual is None:
            return ""
        return str(actual).strip()

    @staticmethod
    def _sanitize_json(value: Any) -> Any:
        if isinstance(value, dict):
            return {str(k): FlightInsightService._sanitize_json(v) for k, v in value.items()}
        if isinstance(value, list):
            return [FlightInsightService._sanitize_json(item) for item in value]
        if isinstance(value, datetime):
            return value.isoformat()
        if hasattr(value, "isoformat") and hasattr(value, "year"):
            try:
                return value.isoformat()
            except Exception as exc:  # noqa: BLE001 - isoformat may fail for exotic datetime-like objects
                logger.warning("isoformat() call failed; using str(): %s", exc)
                return str(value)
        return value

    def _case_to_dict(self, case: Any) -> dict[str, Any]:
        if isinstance(case, dict):
            return self._sanitize_json(case)

        return self._sanitize_json(
            {
                "case_id": getattr(case, "case_id", None),
                "case_type": getattr(case, "case_type", None),
                "flight_id": getattr(case, "flight_id", None),
                "flight_no": getattr(case, "flight_no", None),
                "description": getattr(case, "description", None),
                "status": getattr(case, "status", None),
                "created_at": getattr(case, "created_at", None),
                "finished_at": getattr(case, "finished_at", None),
                "cancelled_at": getattr(case, "cancelled_at", None),
                "created_by": getattr(case, "created_by", None),
                "updated_by": getattr(case, "updated_by", None),
                "stand": getattr(case, "stand", None),
                "gate": getattr(case, "gate", None),
                "context": getattr(case, "context", None),
                "log": getattr(case, "log", None),
            }
        )

    @staticmethod
    def _strip_internal_fields(timeline: list[dict[str, Any]]) -> list[dict[str, Any]]:
        stripped: list[dict[str, Any]] = []
        for event in timeline:
            copy_item = {key: value for key, value in event.items() if key != "_dt"}
            stripped.append(copy_item)
        return stripped

    @staticmethod
    def _fallback_history_markdown(
        *,
        flight_id: str,
        flight_number: str,
        window_start: datetime,
        window_end: datetime,
        summary: dict[str, int],
    ) -> str:
        return (
            f"# 航班动态/历史报表\n\n"
            f"- 航班ID: `{flight_id}`\n"
            f"- 航班号: `{flight_number}`\n"
            f"- 时间窗口: `{window_start.isoformat()}` ~ `{window_end.isoformat()}`\n\n"
            "## 摘要\n"
            f"- 事件总数: {summary.get('total_events', 0)}\n"
            f"- 状态变更数: {summary.get('status_changes', 0)}\n"
            f"- 关键字段变更数: {summary.get('key_field_changes', 0)}\n"
        )

    @staticmethod
    def _fallback_journey_markdown(
        *,
        flight_id: str,
        flight_number: str,
        window_start: datetime,
        window_end: datetime,
        merged_count: int,
    ) -> str:
        return (
            f"# 航班事件经过\n\n"
            f"- 航班ID: `{flight_id}`\n"
            f"- 航班号: `{flight_number}`\n"
            f"- 时间窗口: `{window_start.isoformat()}` ~ `{window_end.isoformat()}`\n"
            f"- 合并事件数: {merged_count}\n"
        )
