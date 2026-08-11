"""Redis 连接管理器

统一管理 Redis 连接，支持连接池和健康检查。
已迁移为 redis.asyncio 异步实现，废弃同步路径。
"""

import asyncio
import os
import threading
import time
import warnings
from dataclasses import dataclass
from typing import Any, Optional
from urllib.parse import quote

import redis
import redis.asyncio as redis_async
from redis.asyncio import ConnectionPool as AsyncConnectionPool

from src.infrastructure.common.exceptions import REDIS_EXCEPTIONS
from src.infrastructure.common.runtime_utils import get_runtime_holder
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


@dataclass
class RedisConfig:
    host: str = "localhost"
    port: int = 6379
    db: int = 0
    password: str | None = None
    ssl: bool = False
    max_connections: int = 200
    socket_timeout: int = 5
    socket_connect_timeout: int = 5
    retry_on_timeout: bool = True


class RedisConnectionManager:
    """Redis 连接管理器单例（异步实现，同步方法已废弃）"""

    _instance: Optional["RedisConnectionManager"] = None
    _instance_lock = threading.RLock()
    _pool: AsyncConnectionPool | None = None
    _client: redis_async.Redis | None = None
    _initialized: bool = False

    def __new__(cls, config: RedisConfig | None = None):
        with cls._instance_lock:
            if cls._instance is None:
                cls._instance = super().__new__(cls)
                cls._instance._initialized = False
                cls._instance._initialize_lock = threading.RLock()
            elif not hasattr(cls._instance, "_initialize_lock"):
                cls._instance._initialize_lock = threading.RLock()
            return cls._instance

    async def initialize(self, config: RedisConfig):
        if self._initialized:
            return

        await asyncio.to_thread(self._initialize_once, config)

    def _initialize_once(self, config: RedisConfig):
        with self._initialize_lock:
            if self._initialized:
                return

            pool_kwargs = {
                "host": config.host,
                "port": config.port,
                "db": config.db,
                "password": config.password if config.password else None,
                "max_connections": config.max_connections,
                "socket_timeout": config.socket_timeout,
                "socket_connect_timeout": config.socket_connect_timeout,
                "retry_on_timeout": config.retry_on_timeout,
                "decode_responses": True,
            }

            if config.ssl:
                ssl_connection_cls = getattr(redis_async, "SSLConnection", None)
                if ssl_connection_cls is None:
                    try:
                        from redis.asyncio.connection import SSLConnection

                        ssl_connection_cls = SSLConnection
                    except ImportError as exc:
                        logger.debug(f"Redis SSLConnection import failed: {exc}")
                        ssl_connection_cls = None

                if ssl_connection_cls is not None:
                    pool_kwargs["connection_class"] = ssl_connection_cls
                else:
                    logger.warning(
                        "Redis SSL requested but SSLConnection is unavailable; fallback to non-SSL connection class"
                    )

            pool = AsyncConnectionPool(**pool_kwargs)
            client = redis_async.Redis(connection_pool=pool)
            self._config = config
            self._pool = pool
            self._client = client
            self._initialized = True
            logger.info(f"Redis connection initialized: {config.host}:{config.port}")

    def initialize_sync(self, config: RedisConfig):
        """Deprecated: 使用 initialize() 异步版本代替。"""
        warnings.warn(
            "initialize_sync() is deprecated, use async initialize() instead",
            DeprecationWarning,
            stacklevel=2,
        )
        loop = self._get_event_loop()
        loop.run_until_complete(self.initialize(config))

    @property
    def client(self) -> redis_async.Redis:
        if not self._initialized:
            raise RuntimeError("Redis connection not initialized")
        return self._client

    @property
    def is_initialized(self) -> bool:
        return self._initialized

    async def health_check(self) -> bool:
        try:
            if not self._client:
                return False
            return await self._client.ping()
        except redis.ConnectionError:
            return False
        except REDIS_EXCEPTIONS as e:
            logger.warning(f"Redis health check failed: {e}")
            return False

    def health_check_sync(self) -> bool:
        """Deprecated: 使用 health_check() 异步版本代替。"""
        warnings.warn(
            "health_check_sync() is deprecated, use async health_check() instead",
            DeprecationWarning,
            stacklevel=2,
        )
        loop = self._get_event_loop()
        return loop.run_until_complete(self.health_check())

    async def close(self):
        if self._client:
            await self._client.close()
        self._initialized = False

    def close_sync(self):
        """Deprecated: 使用 close() 异步版本代替。"""
        warnings.warn(
            "close_sync() is deprecated, use async close() instead",
            DeprecationWarning,
            stacklevel=2,
        )
        loop = self._get_event_loop()
        loop.run_until_complete(self.close())

    @classmethod
    def reset_instance(cls):
        """重置单例实例（仅用于测试）"""
        with cls._instance_lock:
            if cls._instance:
                try:
                    loop = cls._get_event_loop()
                    loop.run_until_complete(cls._instance.close())
                except REDIS_EXCEPTIONS as e:
                    logger.debug("Failed to close Redis instance during reset: %s", e)
            cls._instance = None

    @staticmethod
    def _get_event_loop() -> asyncio.AbstractEventLoop:
        """获取或创建事件循环"""
        try:
            loop = asyncio.get_event_loop()
            if loop.is_closed():
                loop = asyncio.new_event_loop()
                asyncio.set_event_loop(loop)
        except RuntimeError:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
        return loop


# 全局访问函数
_redis_disabled: bool = False
_last_redis_config: RedisConfig | None = None
_last_recovery_attempt_at: float = 0.0
_recovery_cooldown_seconds: int = 10
_availability_cache_ttl_seconds: float = 1.0
_last_availability_checked_at: float = 0.0
_last_availability_result: bool = False
_consecutive_failure_count: int = 0
_last_failure_at: float = 0.0
_availability_lock = threading.Lock()
_manager_lock = threading.RLock()
_redis_manager: RedisConnectionManager | None = None
_async_redis_manager: Optional["AsyncRedisConnectionManager"] = None

# 后台恢复线程
_recovery_thread_running: bool = False
_recovery_thread: threading.Thread | None = None


@dataclass
class RedisRuntimeState:
    redis_disabled: bool = False
    last_redis_config: RedisConfig | None = None
    last_recovery_attempt_at: float = 0.0
    last_availability_checked_at: float = 0.0
    last_availability_result: bool = False
    consecutive_failure_count: int = 0
    last_failure_at: float = 0.0
    redis_manager: RedisConnectionManager | None = None
    async_redis_manager: Optional["AsyncRedisConnectionManager"] = None
    recovery_thread_running: bool = False
    recovery_thread: threading.Thread | None = None


_runtime_state_fallback = RedisRuntimeState()
_redis_infrastructure: Optional["RedisInfrastructure"] = None


def _load_globals_into_runtime_state(state: RedisRuntimeState) -> RedisRuntimeState:
    state.redis_disabled = _redis_disabled
    state.last_redis_config = _last_redis_config
    state.last_recovery_attempt_at = _last_recovery_attempt_at
    state.last_availability_checked_at = _last_availability_checked_at
    state.last_availability_result = _last_availability_result
    state.consecutive_failure_count = _consecutive_failure_count
    state.last_failure_at = _last_failure_at
    state.redis_manager = _redis_manager
    state.async_redis_manager = _async_redis_manager
    state.recovery_thread_running = _recovery_thread_running
    state.recovery_thread = _recovery_thread
    return state


def _sync_runtime_state_to_globals(state: RedisRuntimeState) -> None:
    global _redis_disabled
    global _last_redis_config
    global _last_recovery_attempt_at
    global _last_availability_checked_at
    global _last_availability_result
    global _consecutive_failure_count
    global _last_failure_at
    global _redis_manager
    global _async_redis_manager
    global _recovery_thread_running
    global _recovery_thread

    _redis_disabled = state.redis_disabled
    _last_redis_config = state.last_redis_config
    _last_recovery_attempt_at = state.last_recovery_attempt_at
    _last_availability_checked_at = state.last_availability_checked_at
    _last_availability_result = state.last_availability_result
    _consecutive_failure_count = state.consecutive_failure_count
    _last_failure_at = state.last_failure_at
    _redis_manager = state.redis_manager
    _async_redis_manager = state.async_redis_manager
    _recovery_thread_running = state.recovery_thread_running
    _recovery_thread = state.recovery_thread


def _get_runtime_state() -> RedisRuntimeState:
    runtime_holder = get_runtime_holder()
    if runtime_holder is not None:
        state = getattr(runtime_holder, "redis_runtime_state", None)
        if not isinstance(state, RedisRuntimeState):
            state = _load_globals_into_runtime_state(RedisRuntimeState())
            runtime_holder.redis_runtime_state = state
        _sync_runtime_state_to_globals(state)
        return state

    _load_globals_into_runtime_state(_runtime_state_fallback)
    return _runtime_state_fallback


def _persist_runtime_state(state: RedisRuntimeState) -> RedisRuntimeState:
    runtime_holder = get_runtime_holder()
    if runtime_holder is not None:
        runtime_holder.redis_runtime_state = state
    _sync_runtime_state_to_globals(state)
    return state


class RedisInfrastructure:
    """Compatibility facade for bootstrap/runtime Redis lifecycle wiring."""

    def initialize(self, config: RedisConfig) -> None:
        loop = RedisConnectionManager._get_event_loop()
        loop.run_until_complete(initialize_redis(config))

    def is_available(self) -> bool:
        return is_redis_available()

    def disable(self, reason: str = "") -> None:
        disable_redis(reason)


def configure_redis_infrastructure(infrastructure: Optional["RedisInfrastructure"]) -> None:
    """Store a bootstrap-visible Redis infrastructure facade for compatibility."""
    global _redis_infrastructure
    _redis_infrastructure = infrastructure


def get_redis_infrastructure() -> RedisInfrastructure:
    global _redis_infrastructure
    if _redis_infrastructure is None:
        _redis_infrastructure = RedisInfrastructure()
    return _redis_infrastructure


def _str_to_bool(value: str | None, default: bool = False) -> bool:
    if value is None:
        return default
    return str(value).strip().lower() in {"1", "true", "yes", "on"}


def _load_redis_config_from_env() -> RedisConfig:
    return RedisConfig(
        host=os.getenv("REDIS_HOST", "localhost"),
        port=int(os.getenv("REDIS_PORT", "6379")),
        db=int(os.getenv("REDIS_DB", "0")),
        password=os.getenv("REDIS_PASSWORD") or None,
        ssl=_str_to_bool(os.getenv("REDIS_SSL"), False),
        max_connections=int(os.getenv("REDIS_MAX_CONNECTIONS", "200")),
        socket_timeout=int(os.getenv("REDIS_SOCKET_TIMEOUT", "5")),
        socket_connect_timeout=int(os.getenv("REDIS_CONNECT_TIMEOUT", "5")),
        retry_on_timeout=_str_to_bool(os.getenv("REDIS_RETRY_ON_TIMEOUT"), True),
    )


def build_redis_url(config: RedisConfig) -> str:
    """Build a Redis URL from the effective runtime config."""
    scheme = "rediss" if config.ssl else "redis"
    host = str(config.host or "localhost").strip() or "localhost"
    port = int(config.port or 6379)
    db = int(config.db or 0)

    if config.password:
        password = quote(str(config.password), safe="")
        return f"{scheme}://:{password}@{host}:{port}/{db}"
    return f"{scheme}://{host}:{port}/{db}"


def get_effective_redis_url() -> str:
    """Return the currently effective Redis URL for runtime-managed clients."""
    return build_redis_url(_get_recovery_config())


def _get_recovery_config() -> RedisConfig:
    state = _get_runtime_state()
    return state.last_redis_config or _load_redis_config_from_env()


def _get_failure_disable_threshold() -> int:
    return max(1, int(os.getenv("REDIS_DISABLE_FAILURE_THRESHOLD", "3") or "3"))


def _get_failure_reset_window_seconds() -> float:
    return max(1.0, float(os.getenv("REDIS_FAILURE_RESET_SECONDS", "30") or "30"))


def _reset_failure_state(state: RedisRuntimeState | None = None) -> None:
    target_state = state or _get_runtime_state()
    target_state.consecutive_failure_count = 0
    target_state.last_failure_at = 0.0
    _persist_runtime_state(target_state)


def _can_retry_recovery(force: bool = False) -> bool:
    state = _get_runtime_state()
    if force:
        state.last_recovery_attempt_at = time.time()
        _persist_runtime_state(state)
        return True

    now = time.time()
    if now - state.last_recovery_attempt_at >= _recovery_cooldown_seconds:
        state.last_recovery_attempt_at = now
        _persist_runtime_state(state)
        return True
    return False


def _reset_availability_cache() -> None:
    state = _get_runtime_state()
    state.last_availability_checked_at = 0.0
    state.last_availability_result = False
    _persist_runtime_state(state)


def _set_availability_cache(result: bool) -> None:
    state = _get_runtime_state()
    state.last_availability_checked_at = time.time()
    state.last_availability_result = bool(result)
    _persist_runtime_state(state)


def _is_availability_cache_valid(now: float) -> bool:
    state = _get_runtime_state()
    return (now - state.last_availability_checked_at) < _availability_cache_ttl_seconds


def _start_background_recovery():
    """启动后台恢复线程，不阻塞主线程"""
    state = _get_runtime_state()
    if state.recovery_thread_running and state.recovery_thread and state.recovery_thread.is_alive():
        return

    def _background_recovery_worker():
        worker_state = _get_runtime_state()
        worker_state.recovery_thread_running = True
        _persist_runtime_state(worker_state)
        logger.info("[Redis] Starting background recovery...")

        max_attempts = 10
        if max_attempts <= 0:
            worker_state.recovery_thread_running = False
            _persist_runtime_state(worker_state)
            logger.error("[Redis] Background recovery disabled: max_attempts <= 0")
            return

        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)

        try:
            recovered = False
            for attempt in range(max_attempts):
                try:
                    config = _get_recovery_config()
                    manager = get_redis_manager()

                    if manager.is_initialized:
                        try:
                            if loop.run_until_complete(manager.health_check()):
                                worker_state.redis_disabled = False
                                _persist_runtime_state(worker_state)
                                _set_availability_cache(True)
                                _reset_failure_state(worker_state)
                                logger.info("[Redis] Background recovery succeeded!")
                                recovered = True
                                break
                        except REDIS_EXCEPTIONS as exc:
                            logger.debug(f"Redis background recovery health check failed: {exc}")
                            try:
                                loop.run_until_complete(manager.close())
                            except REDIS_EXCEPTIONS as close_exc:
                                logger.debug(f"Redis manager close during recovery failed: {close_exc}")

                    # 不重复打印初始化日志
                    if not manager.is_initialized:
                        loop.run_until_complete(manager.initialize(config))

                    if loop.run_until_complete(manager.health_check()):
                        worker_state.redis_disabled = False
                        _persist_runtime_state(worker_state)
                        _set_availability_cache(True)
                        _reset_failure_state(worker_state)
                        logger.info("[Redis] Background recovery succeeded!")
                        recovered = True
                        break

                    logger.warning(
                        f"[Redis] Background recovery attempt {attempt + 1}/{max_attempts} failed, retrying in 5s..."
                    )
                    time.sleep(5)
                except Exception as e:  # noqa: BLE001 - background recovery worker must not crash
                    logger.warning(f"[Redis] Background recovery error: {e}")
                    time.sleep(5)

            worker_state.recovery_thread_running = False
            _persist_runtime_state(worker_state)
            if not recovered:
                logger.error("[Redis] Background recovery failed after max attempts")
        finally:
            loop.close()

    state.recovery_thread = threading.Thread(target=_background_recovery_worker, daemon=True)
    _persist_runtime_state(state)
    state.recovery_thread.start()


def _attempt_sync_recovery(force: bool = False) -> bool:
    state = _get_runtime_state()

    if not _can_retry_recovery(force):
        return False

    config = _get_recovery_config()

    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)

    try:
        manager = get_redis_manager()

        # 如果已经初始化且健康，直接返回成功
        if manager.is_initialized:
            try:
                if loop.run_until_complete(manager.health_check()):
                    state.redis_disabled = False
                    _persist_runtime_state(state)
                    _set_availability_cache(True)
                    _reset_failure_state(state)
                    return True
            except REDIS_EXCEPTIONS as exc:
                logger.debug(f"Redis health check failed during sync recovery: {exc}")
            # 只有在不健康时才关闭重新初始化
            loop.run_until_complete(manager.close())

        loop.run_until_complete(manager.initialize(config))
        if loop.run_until_complete(manager.health_check()):
            state.redis_disabled = False
            _persist_runtime_state(state)
            _set_availability_cache(True)
            _reset_failure_state(state)
            return True

        state.redis_disabled = True
        _persist_runtime_state(state)
        _set_availability_cache(False)
        return False
    except Exception as e:  # noqa: BLE001 - recovery function must return bool, not raise
        state.redis_disabled = True
        _persist_runtime_state(state)
        _set_availability_cache(False)
        logger.warning(f"Redis recovery attempt failed: {e}")
        return False
    finally:
        loop.close()


def get_redis_client() -> redis_async.Redis:
    """获取异步 Redis 客户端"""
    state = _get_runtime_state()

    if state.redis_disabled:
        raise RuntimeError("Redis is required but currently disabled.")

    # 超级快速路径：直接返回已初始化的客户端，不做任何阻塞检查！
    if state.redis_manager is not None and state.redis_manager.is_initialized:
        return state.redis_manager.client

    # Redis 未初始化，直接报错
    raise RuntimeError("Redis not initialized.")


def get_redis_manager() -> RedisConnectionManager:
    with _manager_lock:
        state = _get_runtime_state()
        if state.redis_manager is None:
            state.redis_manager = RedisConnectionManager()
        elif RedisConnectionManager._instance is None:
            RedisConnectionManager._instance = state.redis_manager
        _persist_runtime_state(state)
        return state.redis_manager


async def initialize_redis(config: RedisConfig = None):
    with _manager_lock:
        state = _get_runtime_state()
        if state.redis_manager is None:
            state.redis_manager = RedisConnectionManager()

        if config is None:
            config = RedisConfig()

        state.last_redis_config = config
        await state.redis_manager.initialize(config)
        state.redis_disabled = False
        _persist_runtime_state(state)
        _set_availability_cache(True)
        _reset_failure_state(state)


def initialize_redis_from_env():
    config = _load_redis_config_from_env()
    loop = RedisConnectionManager._get_event_loop()
    loop.run_until_complete(initialize_redis(config))


def shutdown_redis():
    """关闭 Redis 连接"""
    with _manager_lock:
        state = _get_runtime_state()
        manager = state.redis_manager
        state.redis_manager = None

        loop = RedisConnectionManager._get_event_loop()
        if manager:
            loop.run_until_complete(manager.close())

        RedisConnectionManager.reset_instance()
        state.async_redis_manager = None
        AsyncRedisConnectionManager.reset_instance()

        state.redis_disabled = False
        state.recovery_thread_running = False
        state.recovery_thread = None
        _persist_runtime_state(state)
        _reset_availability_cache()
        _reset_failure_state(state)


def is_redis_available() -> bool:
    """检查 Redis 是否可用（使用事件循环桥接异步健康检查）"""
    state = _get_runtime_state()

    if state.redis_manager is None or not state.redis_manager.is_initialized:
        _set_availability_cache(False)
        return False

    if state.redis_disabled:
        _set_availability_cache(False)
        return False

    now = time.time()
    if _is_availability_cache_valid(now):
        return state.last_availability_result

    with _availability_lock:
        state = _get_runtime_state()
        now = time.time()
        if _is_availability_cache_valid(now):
            return state.last_availability_result
        try:
            loop = RedisConnectionManager._get_event_loop()
            healthy = loop.run_until_complete(state.redis_manager.health_check())
        except REDIS_EXCEPTIONS as exc:
            logger.debug(f"Redis availability health check failed: {exc}")
            healthy = False
        _set_availability_cache(healthy)
        if healthy:
            _reset_failure_state(state)
            return True

    record_redis_failure("availability health check failed")
    return False


def is_redis_disabled() -> bool:
    """检查 Redis 是否被禁用（降级模式）"""
    return _get_runtime_state().redis_disabled


def enable_redis(force_recovery: bool = False) -> bool:
    """启用 Redis；可选立即触发一次恢复。"""
    state = _get_runtime_state()
    state.redis_disabled = False
    _persist_runtime_state(state)
    _reset_availability_cache()
    _reset_failure_state(state)
    return _attempt_sync_recovery(force=True) if force_recovery else True


def record_redis_failure(reason: str = "") -> bool:
    """Record one runtime Redis failure and only disable after a threshold."""
    state = _get_runtime_state()
    if state.redis_disabled:
        return True

    threshold = _get_failure_disable_threshold()
    reset_window_seconds = _get_failure_reset_window_seconds()
    now = time.time()

    with _manager_lock:
        state = _get_runtime_state()
        if state.redis_disabled:
            return True

        if state.last_failure_at and (now - state.last_failure_at) > reset_window_seconds:
            state.consecutive_failure_count = 0

        state.last_failure_at = now
        state.consecutive_failure_count += 1
        failure_count = state.consecutive_failure_count
        _persist_runtime_state(state)

    if failure_count >= threshold:
        disable_redis(reason or f"failure threshold reached ({failure_count}/{threshold})")
        return True

    if reason:
        logger.warning(
            f"Redis operation failed; keep service enabled until threshold is reached "
            f"({failure_count}/{threshold}): {reason}"
        )
    else:
        logger.warning(
            f"Redis operation failed; keep service enabled until threshold is reached ({failure_count}/{threshold})"
        )
    return False


def disable_redis(reason: str = ""):
    """禁用 Redis（进入降级模式）"""
    with _manager_lock:
        state = _get_runtime_state()
        state.redis_disabled = True
        _set_availability_cache(False)
        if state.redis_manager and state.redis_manager.is_initialized:
            loop = RedisConnectionManager._get_event_loop()
            loop.run_until_complete(state.redis_manager.close())
        state.redis_manager = None
        state.async_redis_manager = None
        state.recovery_thread_running = False
        state.recovery_thread = None
        _persist_runtime_state(state)
        _reset_failure_state(state)
        RedisConnectionManager.reset_instance()
        AsyncRedisConnectionManager.reset_instance()

    if reason:
        logger.warning(f"Redis 已被禁用，系统将使用内存缓存模式: {reason}")
    else:
        logger.warning("Redis 已被禁用，系统将使用内存缓存模式")


def try_initialize_redis_from_env() -> bool:
    """
    初始化 Redis，失败时直接抛异常
    """
    try:
        loop = RedisConnectionManager._get_event_loop()
        config = _load_redis_config_from_env()
        loop.run_until_complete(initialize_redis(config))
        state = _get_runtime_state()
        # 使用 manager 自带的 health_check 而不是 is_redis_available（避免递归）
        if state.redis_manager and state.redis_manager.is_initialized:
            try:
                if loop.run_until_complete(state.redis_manager.health_check()):
                    state.redis_disabled = False
                    _persist_runtime_state(state)
                    _set_availability_cache(True)
                    _reset_failure_state(state)
                    return True
            except REDIS_EXCEPTIONS as exc:
                logger.error(f"Redis health check failed during env init: {exc}")
                raise RuntimeError("Redis is required but health check failed") from exc
        logger.error("Redis connection pool created but failed to ping.")
        raise RuntimeError("Redis is required but failed to ping")
    except Exception as e:
        logger.error(f"Redis initialization failed. System requires Redis. Error details: {e}")
        state = _get_runtime_state()
        state.redis_disabled = True
        _persist_runtime_state(state)
        _set_availability_cache(False)
        raise RuntimeError(f"Redis is required but initialization failed: {e}") from e


class AsyncRedisConnectionManager:
    """Async Redis 连接管理器单例"""

    _instance: Optional["AsyncRedisConnectionManager"] = None
    _instance_lock = threading.RLock()
    _client: Any | None = None  # redis.asyncio.Redis
    _initialized: bool = False

    def __new__(cls, config: RedisConfig | None = None):
        with cls._instance_lock:
            if cls._instance is None:
                cls._instance = super().__new__(cls)
                cls._instance._initialized = False
            return cls._instance

    async def initialize(self, config: RedisConfig):
        if self._initialized:
            return

        if _get_runtime_state().redis_disabled:
            return

        import redis.asyncio as redis_async

        self._config = config
        pool_kwargs = {
            "host": config.host,
            "port": config.port,
            "db": config.db,
            "password": config.password if config.password else None,
            "max_connections": config.max_connections,
            "socket_timeout": config.socket_timeout,
            "socket_connect_timeout": config.socket_connect_timeout,
            "retry_on_timeout": config.retry_on_timeout,
            "decode_responses": True,
        }

        if config.ssl:
            async_ssl_connection_cls = getattr(redis_async, "SSLConnection", None)
            if async_ssl_connection_cls is None:
                try:
                    from redis.asyncio.connection import SSLConnection

                    async_ssl_connection_cls = SSLConnection
                except ImportError as exc:
                    logger.debug(f"Async Redis SSLConnection import failed: {exc}")
                    async_ssl_connection_cls = None

            if async_ssl_connection_cls is not None:
                pool_kwargs["connection_class"] = async_ssl_connection_cls
            else:
                logger.warning(
                    "Async Redis SSL requested but SSLConnection is unavailable; fallback to non-SSL connection class"
                )

        pool = redis_async.ConnectionPool(**pool_kwargs)
        self._client = redis_async.Redis(connection_pool=pool)
        self._initialized = True
        logger.info(f"Async Redis connection initialized: {config.host}:{config.port}")

    @property
    def client(self) -> Any:
        if not self._initialized:
            # Lazy init if not initialized? No, async init needs await.
            # Expect explicit init or auto-init via factory if needed (but impossible to await in property)
            # User must call ensure_async_redis_initialized or similar.
            raise RuntimeError("Async Redis not initialized. Call initialize_async_redis() first.")
        return self._client

    async def close(self):
        if self._client:
            await self._client.close()
            self._initialized = False

    @classmethod
    def reset_instance(cls):
        cls._instance = None


# 全局 Async Redis Manager


async def get_async_redis_client() -> Any:
    state = _get_runtime_state()
    if state.redis_disabled:
        raise RuntimeError("Redis is required but currently disabled.")

    if state.async_redis_manager is None:
        state.async_redis_manager = AsyncRedisConnectionManager()
        _persist_runtime_state(state)

    if not state.async_redis_manager._initialized:
        try:
            await initialize_async_redis_from_env()
        except REDIS_EXCEPTIONS as e:
            state = _get_runtime_state()
            state.redis_disabled = True
            _persist_runtime_state(state)
            _set_availability_cache(False)
            logger.error(f"Async Redis initialization failed: {e}")
            raise RuntimeError("Redis is required but async init failed") from e

    try:
        return state.async_redis_manager.client
    except RuntimeError as e:
        raise RuntimeError(f"Failed to get async redis client: {e}") from e


async def initialize_async_redis(config: RedisConfig = None):
    state = _get_runtime_state()
    if state.async_redis_manager is None:
        state.async_redis_manager = AsyncRedisConnectionManager()

    if config is None:
        config = RedisConfig()  # Should ideally load defaults

    state.last_redis_config = config

    await state.async_redis_manager.initialize(config)
    state.redis_disabled = False
    _persist_runtime_state(state)
    _set_availability_cache(True)
    _reset_failure_state(state)


async def initialize_async_redis_from_env():
    if _get_runtime_state().redis_disabled:
        return

    config = _get_recovery_config()
    await initialize_async_redis(config)
