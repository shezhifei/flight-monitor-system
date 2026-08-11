"""Infrastructure-facing runtime providers."""

from __future__ import annotations

import logging
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

Provider = Callable[[], Any]

logger = logging.getLogger(__name__)


def _none_provider() -> Any:
    return None


@dataclass
class InfrastructureRuntimeProviderContext:
    container_provider: Provider = _none_provider
    config_manager_provider: Provider = _none_provider
    sse_hub_provider: Provider = _none_provider
    alert_service_provider: Provider = _none_provider
    metrics_service_provider: Provider = _none_provider
    session_manager_provider: Provider = _none_provider


class RuntimeProviderManager:
    _instance: RuntimeProviderManager | None = None

    def __init__(self):
        self._context = InfrastructureRuntimeProviderContext()

    @classmethod
    def get_instance(cls) -> RuntimeProviderManager:
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    def configure_providers(
        self,
        *,
        container_provider: Provider | None = None,
        config_manager_provider: Provider | None = None,
        sse_hub_provider: Provider | None = None,
        alert_service_provider: Provider | None = None,
        metrics_service_provider: Provider | None = None,
        session_manager_provider: Provider | None = None,
    ) -> None:
        if container_provider is not None:
            self._context.container_provider = container_provider or _none_provider
        if config_manager_provider is not None:
            self._context.config_manager_provider = config_manager_provider or _none_provider
        if sse_hub_provider is not None:
            self._context.sse_hub_provider = sse_hub_provider or _none_provider
        if alert_service_provider is not None:
            self._context.alert_service_provider = alert_service_provider or _none_provider
        if metrics_service_provider is not None:
            self._context.metrics_service_provider = metrics_service_provider or _none_provider
        if session_manager_provider is not None:
            self._context.session_manager_provider = session_manager_provider or _none_provider

    def get_container(self) -> Any:
        return self._safe_resolve(self._context.container_provider)

    def get_config_manager(self) -> Any:
        return self._safe_resolve(self._context.config_manager_provider)

    def get_sse_hub(self) -> Any:
        return self._safe_resolve(self._context.sse_hub_provider)

    def get_alert_service(self) -> Any:
        return self._safe_resolve(self._context.alert_service_provider)

    def get_metrics_service(self) -> Any:
        return self._safe_resolve(self._context.metrics_service_provider)

    def get_session_manager(self) -> Any:
        return self._safe_resolve(self._context.session_manager_provider)

    def reset(self) -> None:
        self._context = InfrastructureRuntimeProviderContext()

    def _safe_resolve(self, provider: Provider) -> Any:
        try:
            return provider()
        except Exception as exc:  # noqa: BLE001 - provider resolution failures return None instead of propagating
            logger.warning("runtime provider resolution failed: %s", exc)
            return None


def configure_infrastructure_runtime_providers(
    *,
    container_provider: Provider | None = None,
    config_manager_provider: Provider | None = None,
    sse_hub_provider: Provider | None = None,
    alert_service_provider: Provider | None = None,
    metrics_service_provider: Provider | None = None,
    session_manager_provider: Provider | None = None,
) -> None:
    manager = RuntimeProviderManager.get_instance()
    manager.configure_providers(
        container_provider=container_provider,
        config_manager_provider=config_manager_provider,
        sse_hub_provider=sse_hub_provider,
        alert_service_provider=alert_service_provider,
        metrics_service_provider=metrics_service_provider,
        session_manager_provider=session_manager_provider,
    )


def get_runtime_container() -> Any:
    return RuntimeProviderManager.get_instance().get_container()


def get_runtime_config_manager() -> Any:
    return RuntimeProviderManager.get_instance().get_config_manager()


def get_runtime_sse_hub() -> Any:
    return RuntimeProviderManager.get_instance().get_sse_hub()


def get_runtime_alert_service() -> Any:
    return RuntimeProviderManager.get_instance().get_alert_service()


def get_runtime_metrics_service() -> Any:
    return RuntimeProviderManager.get_instance().get_metrics_service()


def get_runtime_session_manager() -> Any:
    return RuntimeProviderManager.get_instance().get_session_manager()


def reset_infrastructure_runtime_providers() -> None:
    RuntimeProviderManager.get_instance().reset()
