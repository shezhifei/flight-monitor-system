"""
告警服务

提供系统降级、异常等事件的告警通知
支持多种告警渠道：日志、Webhook、邮件等
"""

import asyncio
import os
import threading
from collections import deque
from collections.abc import Callable
from enum import Enum
from typing import Any, Optional

import httpx

from src.domain.utils.time_utils import to_utc_iso_z, utc_now
from src.infrastructure.ai.security.url_guard import (
    UnsafeUrlError,
    redact_url_for_log,
    validate_external_http_url,
)
from src.infrastructure.config.integration.app_config_integration import get_app_config_integration
from src.infrastructure.logging.core import get_logger
from src.infrastructure.notifications.email_sender import load_email_delivery_config, send_email, send_email_async

logger = get_logger(__name__)
HTTP_EXCEPTIONS = (httpx.HTTPError, OSError, RuntimeError)


def _alert_service_provider() -> Optional["AlertService"]:
    return None


def configure_alert_service_provider(provider: Callable[[], Optional["AlertService"]] | None) -> None:
    global _alert_service_provider
    _alert_service_provider = provider or (lambda: None)


class AlertLevel(Enum):
    """告警级别"""

    INFO = "info"
    WARNING = "warning"
    ERROR = "error"
    CRITICAL = "critical"


class AlertChannel(Enum):
    """告警渠道"""

    LOG = "log"
    WEBHOOK = "webhook"
    EMAIL = "email"


class AlertService:
    """
    告警服务

    管理系统告警，支持多种告警级别和渠道
    """

    def __init__(self):
        self._webhook_url: str | None = None
        self._alert_history: deque = deque(maxlen=1000)
        self._max_history = 1000
        self._email_recipients: list[str] = self._load_email_recipients()
        self._webhook_async_client: httpx.AsyncClient | None = None
        self._webhook_sync_client: httpx.Client | None = None
        self._webhook_client_lock = asyncio.Lock()
        self._webhook_sync_lock = threading.Lock()
        self._background_tasks: set[asyncio.Task[Any]] = set()

    def configure_webhook(self, webhook_url: str):
        """配置Webhook告警地址"""
        try:
            self._webhook_url = validate_external_http_url(webhook_url, purpose="alert webhook URL")
        except UnsafeUrlError as exc:
            self._webhook_url = None
            logger.warning("[ALERT] Webhook rejected: %s", exc)
            raise ValueError(str(exc)) from exc
        logger.info("[ALERT] Webhook configured: %s", redact_url_for_log(self._webhook_url))

    def configure_email_recipients(self, recipients: list[str]):
        """配置告警邮件接收人列表"""
        self._email_recipients = [str(r).strip() for r in (recipients or []) if str(r).strip()]
        logger.info("[ALERT] Email recipients configured", recipients=self._email_recipients)

    def send_alert(
        self,
        title: str,
        message: str,
        level: AlertLevel = AlertLevel.WARNING,
        channels: list[AlertChannel] | None = None,
        metadata: dict[str, Any] | None = None,
    ):
        """
        发送告警

        Args:
            title: 告警标题
            message: 告警消息
            level: 告警级别
            channels: 告警渠道列表，默认仅日志
            metadata: 额外元数据
        """
        if channels is None:
            channels = [AlertChannel.LOG]

        alert_data = {
            "timestamp": utc_now().isoformat(),
            "title": title,
            "message": message,
            "level": level.value,
            "metadata": metadata or {},
        }

        # 记录到历史
        self._alert_history.append(alert_data)

        # 根据渠道发送
        for channel in channels:
            if channel == AlertChannel.LOG:
                self._send_log_alert(title, message, level)
            elif channel == AlertChannel.WEBHOOK:
                self._send_webhook_alert(alert_data)
            elif channel == AlertChannel.EMAIL:
                self._send_email_alert(title, message, level)

    async def send_alert_async(
        self,
        title: str,
        message: str,
        level: AlertLevel = AlertLevel.WARNING,
        channels: list[AlertChannel] | None = None,
        metadata: dict[str, Any] | None = None,
    ):
        if channels is None:
            channels = [AlertChannel.LOG]

        alert_data = {
            "timestamp": utc_now().isoformat(),
            "title": title,
            "message": message,
            "level": level.value,
            "metadata": metadata or {},
        }

        self._alert_history.append(alert_data)

        for channel in channels:
            if channel == AlertChannel.LOG:
                self._send_log_alert(title, message, level)
            elif channel == AlertChannel.WEBHOOK:
                await self._send_webhook_alert_async(alert_data)
            elif channel == AlertChannel.EMAIL:
                await self._send_email_alert_async(title, message, level)

    def _send_log_alert(self, title: str, message: str, level: AlertLevel):
        """发送日志告警"""
        log_message = f"[ALERT][{level.value.upper()}] {title}: {message}"

        if level == AlertLevel.INFO:
            logger.info(log_message)
        elif level == AlertLevel.WARNING:
            logger.warning(log_message)
        elif level == AlertLevel.ERROR:
            logger.error(log_message)
        elif level == AlertLevel.CRITICAL:
            logger.critical(log_message)

    def _send_webhook_alert(self, alert_data: dict[str, Any]):
        """发送Webhook告警"""
        if not self._webhook_url:
            logger.debug("[ALERT] Webhook not configured, skipping webhook alert")
            return

        try:
            loop = asyncio.get_running_loop()
            task = loop.create_task(self._send_webhook_alert_async(alert_data))
            self._background_tasks.add(task)
            task.add_done_callback(self._background_tasks.discard)
            return
        except RuntimeError:
            pass

        # 同步路径降级：在后台线程发送，避免阻塞当前调用线程。
        def _send_in_background() -> None:
            try:
                client = self._get_sync_webhook_client()
                if client is None:
                    return
                response = client.post(self._webhook_url, json=alert_data)
                if response.status_code == 200:
                    logger.debug("[ALERT] Webhook alert sent successfully")
                else:
                    logger.warning(f"[ALERT] Webhook alert failed: {response.status_code}")
            except HTTP_EXCEPTIONS as exc:
                logger.error(f"[ALERT] Failed to send webhook alert: {exc}")

        threading.Thread(target=_send_in_background, daemon=True).start()

    async def _send_webhook_alert_async(self, alert_data: dict[str, Any]) -> None:
        if not self._webhook_url:
            logger.debug("[ALERT] Webhook not configured, skipping webhook alert")
            return

        try:
            client = await self._get_async_webhook_client()
            response = await client.post(self._webhook_url, json=alert_data)
            if response.status_code == 200:
                logger.debug("[ALERT] Webhook alert sent successfully")
            else:
                logger.warning(f"[ALERT] Webhook alert failed: {response.status_code}")
        except HTTP_EXCEPTIONS as e:
            logger.error(f"[ALERT] Failed to send webhook alert: {e}")

    async def _get_async_webhook_client(self) -> httpx.AsyncClient:
        if self._webhook_async_client is not None:
            return self._webhook_async_client

        async with self._webhook_client_lock:
            if self._webhook_async_client is None:
                self._webhook_async_client = httpx.AsyncClient(timeout=5.0)
            return self._webhook_async_client

    def _get_sync_webhook_client(self) -> httpx.Client | None:
        with self._webhook_sync_lock:
            if self._webhook_sync_client is None:
                try:
                    self._webhook_sync_client = httpx.Client(timeout=5.0)
                except HTTP_EXCEPTIONS as exc:
                    logger.error(f"[ALERT] Failed to initialize sync webhook client: {exc}")
                    return None
            return self._webhook_sync_client

    def _send_email_alert(self, title: str, message: str, level: AlertLevel):
        """发送邮件告警"""
        email_config = load_email_delivery_config()
        if not email_config.enabled:
            logger.debug("[ALERT] SMTP disabled, skipping email alert")
            return

        recipients = self._load_email_recipients()
        if not recipients:
            logger.warning("[ALERT] Email channel enabled but no recipients configured")
            return

        subject = f"[Flight Monitor][{level.value.upper()}] {title}"
        alert_time = to_utc_iso_z(utc_now())
        body = f"告警时间: {alert_time}\n告警级别: {level.value}\n告警标题: {title}\n告警内容: {message}\n"

        sent = send_email(subject=subject, body_text=body, recipients=recipients, config=email_config)
        if not sent:
            logger.error(f"[ALERT] Failed to send email alert: {title}")

    async def _send_email_alert_async(self, title: str, message: str, level: AlertLevel) -> None:
        email_config = load_email_delivery_config()
        if not email_config.enabled:
            logger.debug("[ALERT] SMTP disabled, skipping email alert")
            return

        recipients = self._load_email_recipients()
        if not recipients:
            logger.warning("[ALERT] Email channel enabled but no recipients configured")
            return

        subject = f"[Flight Monitor][{level.value.upper()}] {title}"
        alert_time = to_utc_iso_z(utc_now())
        body = f"告警时间: {alert_time}\n告警级别: {level.value}\n告警标题: {title}\n告警内容: {message}\n"

        sent = await send_email_async(subject=subject, body_text=body, recipients=recipients, config=email_config)
        if not sent:
            logger.error(f"[ALERT] Failed to send email alert: {title}")

    def _load_email_recipients(self) -> list[str]:
        env_recipients = os.getenv("ALERT_EMAIL_TO", "").strip()
        if env_recipients:
            recipients = [item.strip() for item in env_recipients.split(",") if item.strip()]
            if recipients:
                self._email_recipients = recipients
                return recipients

        cached_recipients = getattr(self, "_email_recipients", [])
        if cached_recipients:
            return list(cached_recipients)

        try:
            cfg = get_app_config_integration().get_config() or {}
            email_cfg = (cfg.get("alerts") or {}).get("email") or {}
            raw = email_cfg.get("recipients", [])
            if isinstance(raw, str):
                recipients = [item.strip() for item in raw.split(",") if item.strip()]
            elif isinstance(raw, list):
                recipients = [str(item).strip() for item in raw if str(item).strip()]
            else:
                recipients = []

            self._email_recipients = recipients
            return list(self._email_recipients)
        except Exception as e:  # noqa: BLE001 - config loading may fail in various ways
            logger.warning(f"[ALERT] Failed to load email recipients: {e}")
            return []

    def send_redis_fallback_alert(self, duration_seconds: float | None = None):
        """
        发送Redis降级告警

        Args:
            duration_seconds: 已降级时长（秒）
        """
        title = "Redis降级告警"

        if duration_seconds:
            message = f"Redis不可用，系统已降级为内存模式 {duration_seconds:.1f} 秒"
        else:
            message = "Redis不可用，系统已降级为内存模式"

        self.send_alert(
            title=title,
            message=message,
            level=AlertLevel.WARNING,
            metadata={"type": "redis_fallback", "duration_seconds": duration_seconds},
        )

    def send_redis_recovery_alert(self, duration_seconds: float):
        """
        发送Redis恢复告警

        Args:
            duration_seconds: 降级持续时间（秒）
        """
        self.send_alert(
            title="Redis恢复通知",
            message=f"Redis已恢复，降级模式持续 {duration_seconds:.1f} 秒",
            level=AlertLevel.INFO,
            metadata={"type": "redis_recovery", "duration_seconds": duration_seconds},
        )

    def get_alert_history(self, limit: int = 100, level: AlertLevel | None = None) -> list[dict[str, Any]]:
        """
        获取告警历史

        Args:
            limit: 返回数量限制
            level: 按级别筛选

        Returns:
            告警历史列表
        """
        history = self._alert_history

        if level:
            history = [h for h in history if h["level"] == level.value]

        return history[-limit:]


def get_alert_service() -> AlertService:
    """获取显式装配的告警服务实例。"""
    service = _alert_service_provider()
    if service is None:
        raise RuntimeError("Alert service is not configured")
    return service


async def send_alert_async(
    *,
    title: str,
    message: str,
    level: AlertLevel = AlertLevel.WARNING,
    channels: list[AlertChannel] | None = None,
    metadata: dict[str, Any] | None = None,
) -> None:
    """Backward-compatible module-level async alert entrypoint."""
    service = get_alert_service()
    await service.send_alert_async(
        title=title,
        message=message,
        level=level,
        channels=channels,
        metadata=metadata,
    )


def send_alert(
    *,
    title: str,
    message: str,
    level: AlertLevel = AlertLevel.WARNING,
    channels: list[AlertChannel] | None = None,
    metadata: dict[str, Any] | None = None,
) -> None:
    """Backward-compatible module-level sync alert entrypoint."""
    service = get_alert_service()
    service.send_alert(
        title=title,
        message=message,
        level=level,
        channels=channels,
        metadata=metadata,
    )
