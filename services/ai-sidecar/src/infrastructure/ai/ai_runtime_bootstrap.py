"""AI sidecar 运行时装配（生产 DI bootstrap）。

把 v2 能力栈的各组件显式地构建并注册进 :class:`AiServiceContainer`，让
``runtime_service.get_runtime_service`` 与 ``management_routes`` 在生产启动时拿到真实的
依赖（config store / repos / cache / skill / mcp / capability resolver），而不是退化到
``None`` / 503。

设计原则（与项目北星一致）：

* **显式 wiring，无导入期副作用单例**：所有构建发生在显式调用的 bootstrap 函数内，
  不在模块导入时执行。
* **分层不被绕过**：本模块只负责"组合根"职责——构建 adapter、注入 container。它不
  实现任何业务逻辑，也不触碰 Rust 控制面边界。
* **安全降级**：``bootstrap_ai_runtime_from_env`` 在缺少数据库 / Redis 配置或连接失败
  时记录告警并安全降级（不抛出），使 sidecar 仍能以只读 / 受限能力启动。
* **MCP 命令边界**：``McpClientManager`` 的 allowlist 唯一来源是
  ``AI_MCP_COMMAND_ALLOWLIST_JSON``（见 ``mcp/command_allowlist.py``），bootstrap 不引入
  任何动态旁路。
"""

from __future__ import annotations

import os
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

import redis.asyncio as redis

from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS, REDIS_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


# ---------------------------------------------------------------------------
# Component wiring
# ---------------------------------------------------------------------------


def build_and_register_runtime(
    *,
    db_pool: Any,
    redis_client: Any = None,
    config_store: Any = None,
    skill_roots: list[str] | None = None,
) -> None:
    """构建并注册 v2 能力栈的全部组件到全局 AI container。

    幂等性：可多次调用（例如测试），后注册的实例覆盖先前的。调用结束会把
    ``runtime_service`` 的模块级缓存重置为 ``None``，以便下次 ``get_runtime_service``
    使用最新注册的依赖重建。

    Args:
        db_pool: asyncpg 连接池，驱动全部 v2 repo 与（缺省时）config store。
        redis_client: 可选 async redis 客户端，驱动缓存读写路径。
        config_store: 可选的 config store；缺省用 ``AsyncpgAIConfigStore(db_pool)``。
        skill_roots: 可选的 skill 文件根目录白名单（传给 SkillLoader）。
    """
    from src.infrastructure.ai.agent_skills.instruction_composer import SkillInstructionComposer
    from src.infrastructure.ai.agent_skills.skill_loader import SkillLoader
    from src.infrastructure.ai.ai_container import (
        get_ai_container,
        register_cache_manager,
        register_cache_metrics_repo,
        register_capability_resolver,
        register_context_budget_planner,
        register_mcp_client_manager,
        register_mcp_repo,
        register_model_catalog_repo,
        register_skill_instruction_composer,
        register_skill_loader,
        register_skill_repo,
    )
    from src.infrastructure.ai.asyncpg_config_store import AsyncpgAIConfigStore
    from src.infrastructure.ai.cache_manager import AiCacheManager
    from src.infrastructure.ai.cache_metrics_repository import PostgresCacheMetricsRepository
    from src.infrastructure.ai.capability_resolver import CapabilityResolver
    from src.infrastructure.ai.context_budget_planner import ContextBudgetPlanner
    from src.infrastructure.ai.mcp.client_manager import McpClientManager
    from src.infrastructure.ai.mcp.command_allowlist import get_command_allowlist
    from src.infrastructure.ai.mcp_repository import PostgresMcpRepository
    from src.infrastructure.ai.model_catalog_repository import PostgresModelCatalogRepository
    from src.infrastructure.ai.skill_repository import PostgresSkillRepository
    from src.infrastructure.ai.tools.read_only_tools import READ_ONLY_TOOL_SCHEMAS

    container = get_ai_container()

    # --- config store (policy source of truth) ---
    if config_store is None:
        config_store = AsyncpgAIConfigStore(db_pool)
    container.register("config_store", config_store)

    # --- repositories (separate asset tables) ---
    mcp_repo = PostgresMcpRepository(db_pool)
    skill_repo = PostgresSkillRepository(db_pool)
    model_catalog_repo = PostgresModelCatalogRepository(db_pool)
    cache_metrics_repo = PostgresCacheMetricsRepository(db_pool)

    register_mcp_repo(mcp_repo)
    register_skill_repo(skill_repo)
    register_model_catalog_repo(model_catalog_repo)
    register_cache_metrics_repo(cache_metrics_repo)

    # --- cache manager (redis-backed; degrades to no-op without redis) ---
    cache_manager = AiCacheManager(
        redis_client=redis_client,
        cache_metrics_repo=cache_metrics_repo,
    )
    register_cache_manager(cache_manager)

    # --- context budget planner ---
    register_context_budget_planner(ContextBudgetPlanner())

    # --- skill loader + instruction composer ---
    resolved_skill_roots = skill_roots
    if resolved_skill_roots is None:
        raw_roots = os.environ.get("AI_SKILL_ALLOWED_ROOTS", "").strip()
        resolved_skill_roots = [r for r in raw_roots.split(os.pathsep) if r] if raw_roots else []
    skill_loader = SkillLoader(allowed_roots=resolved_skill_roots, skill_repo=skill_repo)
    register_skill_loader(skill_loader)
    register_skill_instruction_composer(SkillInstructionComposer(skill_loader=skill_loader, skill_repo=skill_repo))

    # --- MCP client manager (allowlist sourced ONLY from env JSON) ---
    register_mcp_client_manager(
        McpClientManager(
            mcp_repo=mcp_repo,
            command_allowlist=get_command_allowlist(),
            cache_manager=cache_manager,
        )
    )

    # --- capability resolver (immutable per-run snapshot generator) ---
    register_capability_resolver(
        CapabilityResolver(
            config_store=config_store,
            model_catalog_repo=model_catalog_repo,
            mcp_repo=mcp_repo,
            skill_repo=skill_repo,
            builtin_tools=list(READ_ONLY_TOOL_SCHEMAS),
        )
    )

    # Reset the cached default runtime service so it picks up freshly registered deps.
    try:
        import src.infrastructure.ai.runtime_service as runtime_service_module

        runtime_service_module._default_runtime_service = None
    except Exception as exc:  # pragma: no cover - defensive  # noqa: BLE001 - defensive import reset
        logger.warning("Could not reset default runtime service: %s", exc)

    logger.info("AI runtime DI bootstrap complete (v2 capability stack registered)")


def _register_mq_components(mq_components: Any) -> None:
    """Register the Wave 2.5 MQ control-plane singletons in the AI container.

    Pre-builds a :class:`ToolExecutor` that has the gate wired so the
    LLMStreamRunner path in ``stream_run_with_tools`` uses it.
    """
    from src.infrastructure.ai.ai_container import (
        get_ai_container,
        register_mq_command_poller,
        register_mq_event_publisher,
        register_prometheus_registry,
        register_tool_executor,
        register_tool_mq_gate,
    )
    from src.infrastructure.ai.messaging.mq_runtime_bootstrap import set_mq_runtime_components
    from src.infrastructure.ai.tools.tool_executor import ToolExecutor

    register_tool_mq_gate(mq_components.gate)
    register_mq_event_publisher(mq_components.publisher)
    register_mq_command_poller(mq_components.poller)
    set_mq_runtime_components(mq_components)

    from src.infrastructure.ai.monitoring.prometheus_exporter import REGISTRY

    register_prometheus_registry(REGISTRY)

    cont = get_ai_container()
    tool_executor = ToolExecutor(
        mcp_client_manager=cont.resolve("mcp_client_manager", None),
        mcp_repo=cont.resolve("mcp_repo", None),
        subagent_dispatcher=None,
        cache_manager=cont.resolve("cache_manager", None),
        mq_gate=mq_components.gate,
    )
    register_tool_executor(tool_executor)

    try:
        import src.infrastructure.ai.runtime_service as runtime_service_module

        runtime_service_module._default_runtime_service = None
    except Exception as exc:  # noqa: BLE001
        logger.warning("Could not reset default runtime service after MQ wiring: %s", exc)


# ---------------------------------------------------------------------------
# Environment-driven resource construction
# ---------------------------------------------------------------------------


def _resolve_database_dsn() -> str | None:
    """Resolve a Postgres DSN from environment variables."""
    for key in ("AI_DATABASE_URL", "DATABASE_URL", "POSTGRES_DSN"):
        value = os.environ.get(key, "").strip()
        if value:
            return value

    host = os.environ.get("POSTGRES_HOST", "").strip()
    if not host:
        return None
    port = os.environ.get("POSTGRES_PORT", "5432").strip() or "5432"
    user = os.environ.get("POSTGRES_USER", "postgres").strip() or "postgres"
    password = os.environ.get("POSTGRES_PASSWORD", "").strip()
    database = os.environ.get("POSTGRES_DB", "postgres").strip() or "postgres"
    auth = f"{user}:{password}" if password else user
    return f"postgresql://{auth}@{host}:{port}/{database}"


async def create_asyncpg_pool_from_env() -> Any | None:
    """Create an asyncpg pool from environment configuration, or ``None``.

    Returns ``None`` (with a warning) when no DSN is configured or asyncpg is
    unavailable, so callers can degrade gracefully.
    """
    dsn = _resolve_database_dsn()
    if not dsn:
        logger.warning(
            "No database DSN configured (AI_DATABASE_URL / DATABASE_URL / POSTGRES_*); "
            "AI runtime will start without DB-backed config."
        )
        return None

    try:
        import asyncpg
    except Exception as exc:  # pragma: no cover - depends on optional dep  # noqa: BLE001 - optional dep import guard
        logger.warning("asyncpg unavailable, cannot create pool: %s", exc)
        return None

    min_size = int(os.environ.get("AI_DB_POOL_MIN_SIZE", "1"))
    max_size = int(os.environ.get("AI_DB_POOL_MAX_SIZE", "8"))
    try:
        pool = await asyncpg.create_pool(dsn=dsn, min_size=min_size, max_size=max_size)
        logger.info("asyncpg pool created (min=%s, max=%s)", min_size, max_size)
        return pool
    except POSTGRES_EXCEPTIONS as exc:
        logger.error("Failed to create asyncpg pool: %s", exc)
        return None


async def _create_redis_client_from_env() -> Any | None:
    """Create an async redis client from ``REDIS_URL``, or ``None``."""
    redis_url = os.environ.get("REDIS_URL", "").strip()
    if not redis_url:
        return None
    try:
        client = redis.from_url(redis_url, decode_responses=True)
        return client
    except REDIS_EXCEPTIONS as exc:
        logger.warning("Could not create redis client: %s", exc)
        return None


async def bootstrap_ai_runtime_from_env() -> bool:
    """Bootstrap the AI runtime DI graph from environment configuration.

    Safe to call on FastAPI startup: any failure is logged and swallowed so the
    sidecar can still start in a degraded (no DB-backed capability) mode.

    Returns:
        ``True`` if the full DI graph was registered; ``False`` if it degraded.
    """
    try:
        pool = await create_asyncpg_pool_from_env()
        if pool is None:
            logger.warning("AI runtime bootstrap degraded: no database pool available")
            return False

        redis_client = await _create_redis_client_from_env()
        build_and_register_runtime(db_pool=pool, redis_client=redis_client)

        # Wave 2.5: build the MQ control plane (publisher + poller + gate).
        # Degrades closed (None) when the mq-gateway URL or DB pool is
        # missing. The runtime service falls back to legacy behavior in
        # that mode (no MQ publishes, no gate).
        try:
            from src.infrastructure.ai.messaging.mq_runtime_bootstrap import (
                build_mq_runtime_components,
            )

            mq_components = await build_mq_runtime_components(db_pool=pool)
            if mq_components.is_wired:
                _register_mq_components(mq_components)
                # The poller drain loop is started from the entrypoint
                # lifespan (startup_event) so we own the lifecycle here.
        except Exception as exc:  # noqa: BLE001 - bootstrap must not crash on MQ wiring failure
            logger.warning("MQ runtime components bootstrap failed (degraded): %s", exc)

        return True
    except Exception as exc:  # pragma: no cover - defensive top-level guard  # noqa: BLE001 - top-level bootstrap guard
        logger.error("AI runtime bootstrap failed, continuing degraded: %s", exc)
        return False


async def start_mq_poller_loop() -> Any | None:
    """Start the MQ poller drain loop as a background task.

    Returns the :class:`asyncio.Task` (or ``None`` when no poller is
    wired). Mirrors :meth:`MqRuntimeComponents.start_poller_loop`.
    """
    from src.infrastructure.ai.messaging.mq_runtime_bootstrap import (
        get_mq_runtime_components,
    )

    components = get_mq_runtime_components()
    if components is None:
        return None
    return components.start_poller_loop()


async def stop_mq_poller_loop() -> None:
    """Stop the MQ poller drain loop and tear down the publisher client."""
    from src.infrastructure.ai.messaging.mq_runtime_bootstrap import (
        get_mq_runtime_components,
    )

    components = get_mq_runtime_components()
    if components is None:
        return
    await components.aclose()


@asynccontextmanager
async def ai_runtime_lifespan(app: Any = None) -> AsyncIterator[None]:
    """FastAPI / standalone worker lifespan for the AI runtime.

    On startup this boots the AI runtime from environment variables
    (``bootstrap_ai_runtime_from_env``) and starts the MQ command
    consumer loop when the control plane is wired. On shutdown it stops
    the consumer loop and closes the MQ publisher client.

    The context manager is intentionally fail-open: bootstrap or poller
    startup failures are logged and the host process still starts, so a
    sidecar without MQ configuration or DB connectivity can continue to
    serve its HTTP health endpoint.

    Args:
        app: Optional FastAPI application instance (ignored; present only
            to match the ASGI lifespan signature).

    Yields:
        Control to the application / worker after startup is complete.
    """
    try:
        await bootstrap_ai_runtime_from_env()
    except Exception as exc:  # noqa: BLE001 - lifespan must not crash the host
        logger.error("ai_runtime_lifespan_bootstrap_failed: %s", exc)

    task = await start_mq_poller_loop()
    if task is not None:
        logger.info("ai_runtime_lifespan_poller_started")
    else:
        logger.info("ai_runtime_lifespan_poller_skipped")

    try:
        yield
    finally:
        logger.info("ai_runtime_lifespan_shutdown")
        await stop_mq_poller_loop()


__all__ = [
    "ai_runtime_lifespan",
    "bootstrap_ai_runtime_from_env",
    "build_and_register_runtime",
    "create_asyncpg_pool_from_env",
    "start_mq_poller_loop",
    "stop_mq_poller_loop",
]
