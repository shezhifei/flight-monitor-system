"""DI container for AI services.

Provides a lightweight dependency injection container for AI-sidecar services,
enabling explicit wiring of LLM clients, tool executors, graph runners, and
caching infrastructure without relying on import-time side-effect singletons.
"""

from __future__ import annotations

import threading
from dataclasses import dataclass, field
from typing import Any, TypeVar, cast

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

T = TypeVar("T")


@dataclass
class AiServiceContainer:
    """Lightweight DI container for AI-sidecar services.

    Usage::

        container = AiServiceContainer()
        container.register(LLMClient, OpenAiStreamingLlmClient(...))
        client = container.resolve(LLMClient)
    """

    _services: dict[str, Any] = field(default_factory=dict)
    _factories: dict[str, Any] = field(default_factory=dict)
    _singletons: dict[str, Any] = field(default_factory=dict)
    _lock: threading.RLock = field(default_factory=threading.RLock)

    def register(self, key: str, instance: Any, *, singleton: bool = True) -> None:
        """Register a service instance under a string key.

        Args:
            key: Service identifier (e.g., "llm_client", "tool_executor").
            instance: The service instance or factory callable.
            singleton: If True, cache the resolved instance; if False, call
                factory on each resolve.
        """
        with self._lock:
            if callable(instance) and not callable(instance):
                # It's a class, treat as factory
                self._factories[key] = instance
            elif callable(instance):
                self._factories[key] = instance
            else:
                self._services[key] = instance
                if singleton:
                    self._singletons[key] = instance

    def register_factory(self, key: str, factory: Any, *, singleton: bool = True) -> None:
        """Register a factory callable that produces service instances."""
        with self._lock:
            self._factories[key] = factory
            if not singleton and key in self._singletons:
                del self._singletons[key]

    def resolve(self, key: str, default: T | None = None) -> T:
        """Resolve a service by key.

        Returns cached singleton if available, otherwise calls factory.
        """
        with self._lock:
            # Check direct registration first
            if key in self._services:
                return self._services[key]

            # Check singleton cache
            if key in self._singletons:
                return self._singletons[key]

            # Try factory
            if key in self._factories:
                factory = self._factories[key]
                instance = factory()
                if instance is not None:
                    self._singletons[key] = instance
                    return instance

            return cast(T, default)

    def has(self, key: str) -> bool:
        """Check if a service is registered."""
        return key in self._services or key in self._factories or key in self._singletons

    def clear(self) -> None:
        """Clear all registrations (useful for testing)."""
        with self._lock:
            self._services.clear()
            self._factories.clear()
            self._singletons.clear()


# ---------------------------------------------------------------------------
# Global container instance
# ---------------------------------------------------------------------------

_global_container: AiServiceContainer | None = None
_container_lock = threading.RLock()


def get_ai_container() -> AiServiceContainer:
    """Get or create the global AI service container."""
    global _global_container
    with _container_lock:
        if _global_container is None:
            _global_container = AiServiceContainer()
        return _global_container


def reset_ai_container() -> None:
    """Reset the global container (for testing only)."""
    global _global_container
    with _container_lock:
        if _global_container is not None:
            _global_container.clear()
        _global_container = None


# ---------------------------------------------------------------------------
# Convenience registration functions
# ---------------------------------------------------------------------------


def register_llm_client(instance: Any) -> None:
    """Register the default LLM client."""
    get_ai_container().register("llm_client", instance)


def register_streaming_llm_client(instance: Any) -> None:
    """Register the streaming LLM client."""
    get_ai_container().register("streaming_llm_client", instance)


def register_tool_executor(instance: Any) -> None:
    """Register the tool executor."""
    get_ai_container().register("tool_executor", instance)


def register_graph_runner(instance: Any) -> None:
    """Register the graph runner."""
    get_ai_container().register("graph_runner", instance)


def resolve_llm_client(default: T | None = None) -> T:
    """Resolve the default LLM client."""
    return get_ai_container().resolve("llm_client", default)


def resolve_streaming_llm_client(default: T | None = None) -> T:
    """Resolve the streaming LLM client."""
    return get_ai_container().resolve("streaming_llm_client", default)


def resolve_tool_executor(default: T | None = None) -> T:
    """Resolve the tool executor."""
    return get_ai_container().resolve("tool_executor", default)


def resolve_graph_runner(default: T | None = None) -> T:
    """Resolve the graph runner."""
    return get_ai_container().resolve("graph_runner", default)


# ---------------------------------------------------------------------------
# AI capability extension registration
# ---------------------------------------------------------------------------


def register_capability_resolver(instance: Any) -> None:
    """Register the capability resolver."""
    get_ai_container().register("capability_resolver", instance)


def register_mcp_client_manager(instance: Any) -> None:
    """Register the MCP client manager."""
    get_ai_container().register("mcp_client_manager", instance)


def register_skill_loader(instance: Any) -> None:
    """Register the skill loader."""
    get_ai_container().register("skill_loader", instance)


def register_skill_instruction_composer(instance: Any) -> None:
    """Register the skill instruction composer."""
    get_ai_container().register("skill_instruction_composer", instance)


def register_context_budget_planner(instance: Any) -> None:
    """Register the context budget planner."""
    get_ai_container().register("context_budget_planner", instance)


def register_cache_manager(instance: Any) -> None:
    """Register the cache manager."""
    get_ai_container().register("cache_manager", instance)


def register_tool_registry_snapshot_builder(instance: Any) -> None:
    """Register the tool registry snapshot builder."""
    get_ai_container().register("tool_registry_snapshot_builder", instance)


def register_model_catalog_repo(instance: Any) -> None:
    """Register the model catalog repository."""
    get_ai_container().register("model_catalog_repo", instance)


def register_mcp_repo(instance: Any) -> None:
    """Register the MCP repository."""
    get_ai_container().register("mcp_repo", instance)


def register_skill_repo(instance: Any) -> None:
    """Register the skill repository."""
    get_ai_container().register("skill_repo", instance)


def register_cache_metrics_repo(instance: Any) -> None:
    """Register the cache metrics repository."""
    get_ai_container().register("cache_metrics_repo", instance)


def resolve_capability_resolver(default: T | None = None) -> T:
    """Resolve the capability resolver."""
    return get_ai_container().resolve("capability_resolver", default)


def resolve_mcp_client_manager(default: T | None = None) -> T:
    """Resolve the MCP client manager."""
    return get_ai_container().resolve("mcp_client_manager", default)


def resolve_skill_loader(default: T | None = None) -> T:
    """Resolve the skill loader."""
    return get_ai_container().resolve("skill_loader", default)


def resolve_skill_instruction_composer(default: T | None = None) -> T:
    """Resolve the skill instruction composer."""
    return get_ai_container().resolve("skill_instruction_composer", default)


def resolve_context_budget_planner(default: T | None = None) -> T:
    """Resolve the context budget planner."""
    return get_ai_container().resolve("context_budget_planner", default)


def resolve_cache_manager(default: T | None = None) -> T:
    """Resolve the cache manager."""
    return get_ai_container().resolve("cache_manager", default)


def resolve_tool_registry_snapshot_builder(default: T | None = None) -> T:
    """Resolve the tool registry snapshot builder."""
    return get_ai_container().resolve("tool_registry_snapshot_builder", default)


def resolve_model_catalog_repo(default: T | None = None) -> T:
    """Resolve the model catalog repository."""
    return get_ai_container().resolve("model_catalog_repo", default)


def resolve_mcp_repo(default: T | None = None) -> T:
    """Resolve the MCP repository."""
    return get_ai_container().resolve("mcp_repo", default)


def resolve_skill_repo(default: T | None = None) -> T:
    """Resolve the skill repository."""
    return get_ai_container().resolve("skill_repo", default)


def resolve_cache_metrics_repo(default: T | None = None) -> T:
    """Resolve the cache metrics repository."""
    return get_ai_container().resolve("cache_metrics_repo", default)


def register_subagent_dispatcher(instance: Any) -> None:
    """Register the subagent dispatcher."""
    get_ai_container().register("subagent_dispatcher", instance)


def resolve_subagent_dispatcher(default: T | None = None) -> T:
    """Resolve the subagent dispatcher."""
    return get_ai_container().resolve("subagent_dispatcher", default)


# ---------------------------------------------------------------------------
# MQ control plane registration
# ---------------------------------------------------------------------------


def register_tool_mq_gate(instance: Any) -> None:
    """Register the :class:`ToolMqGate`."""
    get_ai_container().register("tool_mq_gate", instance)


def resolve_tool_mq_gate(default: T | None = None) -> T:
    """Resolve the :class:`ToolMqGate`."""
    return get_ai_container().resolve("tool_mq_gate", default)


def register_mq_event_publisher(instance: Any) -> None:
    """Register the :class:`AiRuntimeEventPublisher`."""
    get_ai_container().register("mq_event_publisher", instance)


def resolve_mq_event_publisher(default: T | None = None) -> T:
    """Resolve the :class:`AiRuntimeEventPublisher`."""
    return get_ai_container().resolve("mq_event_publisher", default)


def register_mq_command_poller(instance: Any) -> None:
    """Register the :class:`AiCommandPoller`."""
    get_ai_container().register("mq_command_poller", instance)


def resolve_mq_command_poller(default: T | None = None) -> T:
    """Resolve the :class:`AiCommandPoller`."""
    return get_ai_container().resolve("mq_command_poller", default)


# ---------------------------------------------------------------------------
# P1b: Prometheus metrics registry registration
# ---------------------------------------------------------------------------


def register_prometheus_registry(instance: Any) -> None:
    """Register the Prometheus ``CollectorRegistry`` used by ``/metrics``."""
    get_ai_container().register("prometheus_registry", instance)


def resolve_prometheus_registry(default: T | None = None) -> T:
    """Resolve the Prometheus ``CollectorRegistry`` used by ``/metrics``."""
    return get_ai_container().resolve("prometheus_registry", default)
