"""SMTP email sending helpers."""

import asyncio
import os
import smtplib
from collections.abc import Sequence
from dataclasses import dataclass
from email.message import EmailMessage

from src.infrastructure.config.integration.app_config_integration import get_app_config_integration
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


def _to_bool(value: object, default: bool = False) -> bool:
    if value is None:
        return default
    if isinstance(value, bool):
        return value
    text = str(value).strip().lower()
    if not text:
        return default
    return text in {"1", "true", "yes", "on"}


def _to_int(value: object, default: int) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


@dataclass
class EmailDeliveryConfig:
    enabled: bool
    host: str
    port: int
    username: str | None
    password: str | None
    from_address: str
    use_tls: bool
    use_ssl: bool
    timeout_seconds: int


def load_email_delivery_config() -> EmailDeliveryConfig:
    cfg = get_app_config_integration().get_config() or {}
    email_cfg = (cfg.get("notifications") or {}).get("email") or {}

    enabled = _to_bool(os.getenv("SMTP_ENABLED", email_cfg.get("enabled", False)))
    host = str(os.getenv("SMTP_HOST", email_cfg.get("host", "")) or "").strip()
    port = _to_int(os.getenv("SMTP_PORT", email_cfg.get("port", 587)), 587)
    username = str(os.getenv("SMTP_USERNAME", email_cfg.get("username", "")) or "").strip() or None
    password = str(os.getenv("SMTP_PASSWORD", email_cfg.get("password", "")) or "").strip() or None
    from_address = str(
        os.getenv("SMTP_FROM_ADDRESS", email_cfg.get("from_address", "no-reply@localhost")) or "no-reply@localhost"
    ).strip()
    use_tls = _to_bool(os.getenv("SMTP_USE_TLS", email_cfg.get("use_tls", True)), default=True)
    use_ssl = _to_bool(os.getenv("SMTP_USE_SSL", email_cfg.get("use_ssl", False)))
    timeout_seconds = _to_int(os.getenv("SMTP_TIMEOUT_SECONDS", email_cfg.get("timeout_seconds", 10)), 10)

    return EmailDeliveryConfig(
        enabled=enabled,
        host=host,
        port=port,
        username=username,
        password=password,
        from_address=from_address,
        use_tls=use_tls,
        use_ssl=use_ssl,
        timeout_seconds=timeout_seconds,
    )


def _normalize_recipients(recipients: Sequence[str]) -> list[str]:
    normalized: list[str] = []
    for recipient in recipients:
        address = str(recipient or "").strip()
        if address:
            normalized.append(address)
    return normalized


def send_email(
    subject: str,
    body_text: str,
    recipients: Sequence[str],
    html_body: str | None = None,
    config: EmailDeliveryConfig | None = None,
) -> bool:
    recipients_list = _normalize_recipients(recipients)
    if not recipients_list:
        logger.warning("Email sending skipped: recipients are empty")
        return False

    mail_config = config or load_email_delivery_config()
    if not mail_config.enabled:
        logger.warning("Email sending skipped: SMTP is disabled")
        return False

    if not mail_config.host:
        logger.error("Email sending failed: SMTP host is empty")
        return False

    msg = EmailMessage()
    msg["Subject"] = subject
    msg["From"] = mail_config.from_address
    msg["To"] = ", ".join(recipients_list)
    msg.set_content(body_text)
    if html_body:
        msg.add_alternative(html_body, subtype="html")

    try:
        smtp_cls = smtplib.SMTP_SSL if mail_config.use_ssl else smtplib.SMTP
        with smtp_cls(mail_config.host, mail_config.port, timeout=mail_config.timeout_seconds) as server:
            if not mail_config.use_ssl and mail_config.use_tls:
                server.starttls()
            if mail_config.username:
                server.login(mail_config.username, mail_config.password or "")
            server.send_message(msg)

        logger.info("Email sent", recipients=recipients_list, subject=subject)
        return True
    except (smtplib.SMTPException, OSError) as exc:
        logger.error(f"Failed to send email: {exc}")
        return False


async def send_email_async(
    subject: str,
    body_text: str,
    recipients: Sequence[str],
    html_body: str | None = None,
    config: EmailDeliveryConfig | None = None,
) -> bool:
    return await asyncio.to_thread(
        send_email,
        subject,
        body_text,
        recipients,
        html_body,
        config,
    )
