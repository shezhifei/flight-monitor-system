"""
智能运行监控服务

提供航班保障流程阶段推断和预警功能。
所有阈值通过配置文件可配置。
"""

import asyncio
import json
from datetime import datetime
from enum import StrEnum
from pathlib import Path
from typing import Any

import yaml

from src.domain.utils.time_utils import now_in_tz, utc_now
from src.infrastructure.common.exceptions import JSON_EXCEPTIONS, LLM_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class AlertLevel(StrEnum):
    """预警级别"""

    INFO = "info"
    WARNING = "warning"
    CRITICAL = "critical"


class SmartMonitor:
    """智能运行监控服务"""

    def __init__(
        self,
        config_path: str = "config/smart_monitor_config.yaml",
        flight_service=None,
        sse_hub=None,
        ai_entity=None,
        update_log_query_service=None,
    ):
        self._config_path = Path(config_path)
        self._flight_service = flight_service
        self._sse_hub = sse_hub
        self._ai_entity = ai_entity
        self._update_log_query_service = update_log_query_service
        self._config: dict[str, Any] = {}
        self._stages: dict[str, dict[str, Any]] = {}
        self._alert_rules: list[dict[str, Any]] = []
        self._running = False
        self._task: asyncio.Task | None = None
        self._processed_gate_change_logs: set[str] = set()
        self._processed_gate_change_order: list[str] = []
        self._processed_gate_change_limit = 2000

        self._load_config()

    def _load_config(self) -> None:
        """加载配置文件"""
        if not self._config_path.exists():
            logger.warning(f"配置文件不存在: {self._config_path}，使用默认配置")
            self._config = self._get_default_config()
        else:
            with open(self._config_path, encoding="utf-8") as f:
                full_config = yaml.safe_load(f)
                self._config = full_config.get("smart_monitor", {})

        # 构建阶段索引
        self._stages = {stage["id"]: stage for stage in self._config.get("process_stages", [])}
        self._alert_rules = self._config.get("alert_rules", [])

        logger.info(f"已加载 {len(self._stages)} 个保障阶段, {len(self._alert_rules)} 条预警规则")

    def _get_default_config(self) -> dict[str, Any]:
        """返回默认配置"""
        return {
            "enabled": True,
            "scan_interval_seconds": 60,
            "process_stages": [
                {
                    "id": "wheel_chocks",
                    "name": "上轮挡",
                    "time_field": "wheel_chocks_time",
                    "standard_duration_min": 5,
                    "warning_threshold_min": 10,
                },
                {
                    "id": "door_open",
                    "name": "开舱门",
                    "time_field": "cabin_door_open_time",
                    "standard_duration_min": 3,
                    "warning_threshold_min": 8,
                },
                {
                    "id": "deboarding",
                    "name": "下客",
                    "time_field": "deboarding_complete_time",
                    "standard_duration_min": 15,
                    "warning_threshold_min": 25,
                },
                {
                    "id": "cleaning",
                    "name": "清洁",
                    "time_field": "cleaning_end_time",
                    "standard_duration_min": 20,
                    "warning_threshold_min": 30,
                },
                {
                    "id": "loading",
                    "name": "装载",
                    "time_field": "loading_complete_time",
                    "standard_duration_min": 25,
                    "warning_threshold_min": 40,
                },
                {
                    "id": "boarding",
                    "name": "登机",
                    "time_field": "end_boarding_time",
                    "standard_duration_min": 30,
                    "warning_threshold_min": 45,
                },
                {
                    "id": "door_close",
                    "name": "关舱门",
                    "time_field": "cabin_door_close_time",
                    "standard_duration_min": 5,
                    "warning_threshold_min": 10,
                },
            ],
            "alert_rules": [],
        }

    def reload_config(self) -> None:
        """重新加载配置（支持热更新）"""
        was_running = self._running or (self._task is not None and not self._task.done())

        if was_running:
            self.stop_periodic_scan()

        # 清理运行时状态，避免热更新后规则与上下文残留
        self._processed_gate_change_logs.clear()
        self._processed_gate_change_order.clear()
        self._stages.clear()
        self._alert_rules.clear()

        self._load_config()

        if was_running:
            self.start_background_scan(interval_seconds=self._config.get("scan_interval_seconds", 60))

        logger.info(
            "配置已重新加载（"
            f"running_before={was_running}, rules={len(self._alert_rules)}, stages={len(self._stages)}"
            "）"
        )

    def set_services(self, flight_service=None, sse_hub=None, ai_entity=None, update_log_query_service=None):
        """注入服务依赖"""
        if flight_service:
            self._flight_service = flight_service
        if sse_hub:
            self._sse_hub = sse_hub
        if ai_entity:
            self._ai_entity = ai_entity
        if update_log_query_service:
            self._update_log_query_service = update_log_query_service

    async def _publish_alert_event(self, alert: dict[str, Any]) -> None:
        """发布智能监控告警事件到 SSE。"""
        if not self._sse_hub:
            return

        payload = {
            "type": "alert",
            "data": alert,
        }

        try:
            if hasattr(self._sse_hub, "broadcast_to_topic"):
                await self._sse_hub.broadcast_to_topic("smart_monitor", payload)
                return

            if hasattr(self._sse_hub, "publish"):
                await self._sse_hub.publish("smart_monitor", payload)
                return

            logger.warning("SSE hub missing publish interface for smart monitor alerts")
        except Exception as e:  # noqa: BLE001 - SSE publish fallback must catch all failures
            logger.warning(f"智能监控 SSE 推送失败: {e}")

    def infer_process_stage(self, flight) -> dict[str, Any]:
        """根据航班时间节点推断当前保障阶段"""
        stages = self._config.get("process_stages", [])

        # 倒序检查（从最后阶段开始）
        completed_stages = []
        current_stage = None

        for stage in stages:
            time_field = stage.get("time_field")
            time_value = getattr(flight, time_field, None) if time_field else None

            if time_value:
                completed_stages.append(
                    {
                        "id": stage["id"],
                        "name": stage["name"],
                        "completed_at": time_value.isoformat() if hasattr(time_value, "isoformat") else str(time_value),
                    }
                )
            elif not current_stage:
                # 第一个未完成的阶段就是当前阶段
                current_stage = stage

        # 计算当前阶段耗时
        elapsed_minutes = None
        if current_stage and completed_stages:
            last_completed = completed_stages[-1]
            last_time_str = last_completed.get("completed_at")
            if last_time_str:
                try:
                    last_time = datetime.fromisoformat(last_time_str)
                    elapsed_minutes = (utc_now() - last_time).total_seconds() / 60
                except (ValueError, TypeError) as e:
                    logger.debug(f"无法解析时间: {e}")

        # 确定下一阶段
        next_stage = None
        if current_stage:
            current_idx = next((i for i, s in enumerate(stages) if s["id"] == current_stage["id"]), -1)
            if current_idx >= 0 and current_idx < len(stages) - 1:
                next_stage = stages[current_idx + 1]

        return {
            "completed_stages": completed_stages,
            "current_stage": {
                "id": current_stage["id"] if current_stage else None,
                "name": current_stage["name"] if current_stage else "等待进港",
                "standard_duration_min": current_stage.get("standard_duration_min") if current_stage else None,
                "warning_threshold_min": current_stage.get("warning_threshold_min") if current_stage else None,
                "elapsed_minutes": round(elapsed_minutes, 1) if elapsed_minutes else None,
            },
            "next_stage": {
                "id": next_stage["id"] if next_stage else None,
                "name": next_stage["name"] if next_stage else "已完成",
            }
            if next_stage
            else None,
            "is_complete": current_stage is None and len(completed_stages) == len(stages),
        }

    @staticmethod
    def _extract_flight_number(flight) -> str:
        """提取可读的航班号字符串。"""
        number = getattr(flight, "flight_number", None)
        if hasattr(number, "value"):
            return str(number.value)
        if number:
            return str(number)

        outbound_leg = getattr(flight, "outbound_leg", None)
        if outbound_leg is not None:
            outbound_no = getattr(outbound_leg, "flight_no", None)
            if outbound_no:
                return str(outbound_no)

        inbound_leg = getattr(flight, "inbound_leg", None)
        if inbound_leg is not None:
            inbound_no = getattr(inbound_leg, "flight_no", None)
            if inbound_no:
                return str(inbound_no)

        return "未知"

    @staticmethod
    def _normalize_datetime(value: Any) -> datetime | None:
        if value is None:
            return None
        if isinstance(value, datetime):
            return value
        if isinstance(value, str):
            text = value.strip()
            if text.endswith("Z"):
                text = text[:-1] + "+00:00"
            try:
                return datetime.fromisoformat(text)
            except ValueError:
                return None
        return None

    def _mark_gate_change_processed(self, log_id: str) -> None:
        if not log_id:
            return
        if log_id in self._processed_gate_change_logs:
            return

        self._processed_gate_change_logs.add(log_id)
        self._processed_gate_change_order.append(log_id)

        if len(self._processed_gate_change_order) > self._processed_gate_change_limit:
            removed_id = self._processed_gate_change_order.pop(0)
            self._processed_gate_change_logs.discard(removed_id)

    @staticmethod
    def _safe_format(template: str, context: dict[str, Any]) -> str:
        try:
            return str(template or "").format(**context)
        except Exception as exc:  # noqa: BLE001 - template format fallback must catch all formatting errors
            logger.warning("smart_monitor_alert_template_format_failed", exc_info=exc)
            return str(template or "")

    async def _detect_gate_change_urgent_alert(self, flight, rule: dict[str, Any]) -> dict[str, Any] | None:
        """检测紧急登机口变更预警。"""
        flight_id_raw = getattr(flight, "flight_id", "")
        flight_id = str(getattr(flight_id_raw, "value", flight_id_raw) or "")
        if not flight_id:
            return None

        departure_time = getattr(flight, "estimated_departure", None) or getattr(flight, "scheduled_departure", None)
        departure_at = self._normalize_datetime(departure_time)
        if not departure_at:
            return None

        now = now_in_tz(departure_at.tzinfo) if departure_at.tzinfo else utc_now()
        minutes_to_departure = (departure_at - now).total_seconds() / 60
        threshold_minutes = float(rule.get("condition_minutes_to_departure", 30) or 30)
        if minutes_to_departure < 0 or minutes_to_departure > threshold_minutes:
            return None

        if self._update_log_query_service is None:
            from src.infrastructure.logging.update_log_query_service import UpdateLogQueryService

            self._update_log_query_service = UpdateLogQueryService()

        change_window_minutes = int(rule.get("condition_change_window_minutes", 120) or 120)
        history = await self._update_log_query_service.get_update_history(
            entity_type="flight",
            entity_id=flight_id,
            page=1,
            page_size=30,
        )

        for entry in history:
            log_id = str(entry.get("id") or "")
            if log_id and log_id in self._processed_gate_change_logs:
                continue

            changes = entry.get("changes") or {}
            if isinstance(changes, str):
                try:
                    changes = json.loads(changes)
                except JSON_EXCEPTIONS as exc:
                    logger.warning("gate_change_history_json_parse_failed", exc_info=exc)
                    changes = {}

            if not isinstance(changes, dict):
                continue

            changed_fields = changes.get("fields") or []
            old_values = changes.get("old") or {}
            new_values = changes.get("new") or {}
            old_gate = old_values.get("gate")
            new_gate = new_values.get("gate")

            if "gate" not in changed_fields and old_gate == new_gate:
                continue
            if not new_gate or old_gate == new_gate:
                continue

            changed_at = self._normalize_datetime(entry.get("created_at") or entry.get("timestamp"))
            if changed_at:
                compare_now = now_in_tz(changed_at.tzinfo) if changed_at.tzinfo else utc_now()
                age_minutes = (compare_now - changed_at).total_seconds() / 60
                if age_minutes > change_window_minutes:
                    continue

            self._mark_gate_change_processed(log_id)
            return {
                "rule_id": "gate_change_urgent",
                "level": rule.get("level", "critical"),
                "old_gate": old_gate or "未知",
                "new_gate": new_gate,
                "minutes": max(0, round(minutes_to_departure)),
                "message": self._safe_format(
                    rule.get("message", ""),
                    {
                        "flight_number": self._extract_flight_number(flight),
                        "old_gate": old_gate or "未知",
                        "new_gate": new_gate,
                        "minutes": max(0, round(minutes_to_departure)),
                    },
                ),
            }

        return None

    async def check_alerts(self, flight) -> list[dict[str, Any]]:
        """检查航班是否触发预警规则"""
        alerts = []
        stage_info = self.infer_process_stage(flight)
        current = stage_info.get("current_stage", {})
        flight_number = self._extract_flight_number(flight)

        for rule in self._alert_rules:
            if not rule.get("enabled", True):
                continue

            alert = None
            rule_id = rule.get("id")

            # 检查环节超时
            if rule_id == "stage_timeout":
                elapsed = current.get("elapsed_minutes")
                threshold = current.get("warning_threshold_min")
                if elapsed and threshold and elapsed > threshold:
                    alert = {
                        "rule_id": rule_id,
                        "level": rule.get("level", "warning"),
                        "message": self._safe_format(
                            rule.get("message", ""),
                            {
                                "flight_number": flight_number,
                                "stage_name": current.get("name", "未知"),
                                "elapsed": round(elapsed),
                            },
                        ),
                    }

            # 检查紧急登机口变更
            elif rule_id == "gate_change_urgent":
                alert = await self._detect_gate_change_urgent_alert(flight, rule)

            # 检查大面积延误
            elif rule_id == "delay_major":
                delay_threshold = rule.get("condition_delay_minutes", 60)
                est_dep = getattr(flight, "estimated_departure", None)
                sch_dep = getattr(flight, "scheduled_departure", None)
                if est_dep and sch_dep:
                    delay_minutes = (est_dep - sch_dep).total_seconds() / 60
                    if delay_minutes > delay_threshold:
                        alert = {
                            "rule_id": rule_id,
                            "level": rule.get("level", "warning"),
                            "message": self._safe_format(
                                rule.get("message", ""),
                                {
                                    "flight_number": flight_number,
                                    "delay_minutes": round(delay_minutes),
                                },
                            ),
                            "delay_minutes": round(delay_minutes),
                        }

            # 如果触发预警，生成AI简报（如配置）
            if alert and rule.get("ai_generate_brief") and self._ai_entity:
                try:
                    prompt_template = rule.get("prompt_template", "")
                    if prompt_template:
                        prompt_context = {
                            "flight_number": flight_number,
                            "stage_name": current.get("name", "未知"),
                            "elapsed": current.get("elapsed_minutes", 0),
                            "delay_minutes": alert.get("delay_minutes", 0),
                            "delay_reason": getattr(flight, "flight_remarks", None) or "未说明",
                            "old_gate": alert.get("old_gate", "未知"),
                            "new_gate": alert.get("new_gate", "未知"),
                            "minutes": alert.get("minutes", 0),
                        }
                        prompt = prompt_template.format(**prompt_context)
                        response = await self._ai_entity.execute_task(prompt)
                        alert["ai_brief"] = response.content if hasattr(response, "content") else str(response)
                except LLM_EXCEPTIONS as e:
                    logger.warning(f"AI简报生成失败: {e}")

            if alert:
                alert["flight_id"] = str(getattr(flight, "flight_id", ""))
                alert["timestamp"] = utc_now().isoformat()
                alerts.append(alert)

        return alerts

    async def scan_all_flights(self) -> dict[str, list[dict[str, Any]]]:
        """扫描所有活跃航班并检查预警"""
        if not self._flight_service:
            logger.warning("未配置航班服务，无法扫描")
            return {}

        all_alerts = {}

        try:
            flights = (
                await self._flight_service.get_active_flights()
                if hasattr(self._flight_service, "get_active_flights")
                else []
            )

            if not flights:
                return all_alerts

            semaphore = asyncio.Semaphore(10)

            async def check_one(flight):
                flight_obj = flight.get_flight() if hasattr(flight, "get_flight") else flight
                flight_id = str(getattr(flight_obj, "flight_id", ""))
                try:
                    async with semaphore:
                        alerts = await self.check_alerts(flight_obj)
                    return flight_id, alerts
                except Exception as e:  # noqa: BLE001 - per-flight alert check must catch all failures
                    logger.error(f"检查航班 {flight_id} 预警失败: {e}")
                    return flight_id, []

            results = await asyncio.gather(*(check_one(f) for f in flights))

            for flight_id, alerts in results:
                if alerts:
                    all_alerts[flight_id] = alerts
                    if self._sse_hub:
                        for alert in alerts:
                            await self._publish_alert_event(alert)
        except Exception as e:  # noqa: BLE001 - top-level scan handler must catch all failures
            logger.error(f"扫描航班失败: {e}")

        return all_alerts

    async def start_periodic_scan(self, interval_seconds: int | None = None) -> None:
        """启动定时扫描任务"""
        if self._running:
            logger.warning("定时扫描已在运行中")
            return

        interval = interval_seconds or self._config.get("scan_interval_seconds", 60)
        self._running = True

        logger.info(f"启动智能监控定时扫描，间隔 {interval} 秒")

        try:
            while self._running:
                try:
                    await self.scan_all_flights()
                except Exception as e:  # noqa: BLE001 - periodic scan loop must catch all failures to keep running
                    logger.error(f"定时扫描出错: {e}")

                await asyncio.sleep(interval)
        finally:
            self._running = False
            current_task = asyncio.current_task()
            if self._task is current_task:
                self._task = None

    def start_background_scan(self, interval_seconds: int | None = None) -> asyncio.Task | None:
        """以后台任务方式启动定时扫描。"""
        if self._task and not self._task.done():
            logger.warning("智能监控后台扫描任务已存在")
            return self._task

        self._task = asyncio.create_task(self.start_periodic_scan(interval_seconds=interval_seconds))
        return self._task

    def stop_periodic_scan(self) -> None:
        """停止定时扫描"""
        self._running = False
        if self._task:
            self._task.cancel()
            self._task = None
        logger.info("智能监控定时扫描已停止")

    def get_status(self) -> dict[str, Any]:
        """获取监控服务状态"""
        return {
            "enabled": self._config.get("enabled", False),
            "running": self._running,
            "stages_count": len(self._stages),
            "rules_count": len(self._alert_rules),
            "scan_interval": self._config.get("scan_interval_seconds", 60),
        }


__all__ = ["AlertLevel", "SmartMonitor"]
