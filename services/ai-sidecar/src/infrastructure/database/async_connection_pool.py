"""
异步数据库连接池实现

使用psycopg 3官方提供的AsyncConnectionPool,提供与原有自研连接池兼容的接口
继承BaseConnectionPool基类,复用公共功能

优化特性:
- 自动连接健康检查
- 连接生命周期管理
- 性能指标收集
- 自适应连接池大小调整
"""

import asyncio
import inspect
import time
from collections.abc import AsyncGenerator
from contextlib import AbstractAsyncContextManager, asynccontextmanager
from typing import Any, NoReturn

import psycopg
from psycopg.rows import dict_row
from psycopg_pool import AsyncConnectionPool

from src.infrastructure.logging.core import get_logger

from .base_pool import BaseConnectionPool
from .encoding_handler import EncodingHandler
from .pool_models import DatabaseConfig, PoolConfig

logger = get_logger(__name__)


class AsyncPooledConnection:
    """
    异步连接池连接包装器

    提供与原有自研连接池兼容的接口,包含性能监控功能
    """

    def __init__(
        self,
        connection: psycopg.AsyncConnection[tuple[Any, ...]],
        pool: AsyncConnectionPool[psycopg.AsyncConnection[tuple[Any, ...]]],
        owner_pool: "AsyncPooledDatabaseConnection" | None = None,
        transaction_timeout_seconds: int | None = None,
    ):
        """
        初始化连接包装器

        Args:
            connection: psycopg异步连接
            pool: 连接池实例
        """
        self._connection = connection
        self._pool = pool
        self._owner_pool = owner_pool
        self._transaction_manager = AsyncConnectionTransactionManager(
            self,
            owner_pool=owner_pool,
            timeout_seconds=transaction_timeout_seconds,
        )
        self._closed = False
        self._created_at = time.time()
        self._last_used_at = time.time()
        self._query_count = 0
        self._error_count = 0

    def _record_and_reraise(self, exc: BaseException) -> NoReturn:
        """统一记录查询失败指标后抛出原异常。"""
        self._error_count += 1
        logger.exception("Database operation failed")
        raise exc

    @property
    def connection(self) -> psycopg.AsyncConnection[tuple[Any, ...]]:
        """获取底层psycopg连接"""
        return self._connection

    @property
    def transaction_manager(self) -> "AsyncConnectionTransactionManager":
        """获取事务管理器"""
        return self._transaction_manager

    def cursor(self, **kwargs: Any) -> Any:
        """
        获取游标,自动继承 row_factory

        Args:
            **kwargs: 游标参数

        Returns:
            游标对象
        """
        if "row_factory" not in kwargs:
            kwargs["row_factory"] = dict_row
        return self._connection.cursor(**kwargs)

    async def execute(self, query: str, params: tuple[Any, ...] | None = None) -> int:
        """
        执行SQL查询

        Args:
            query: SQL查询语句
            params: 查询参数

        Returns:
            受影响行数
        """
        self._last_used_at = time.time()
        self._query_count += 1

        try:
            async with self._connection.cursor(row_factory=dict_row) as cursor:
                await cursor.execute(query, params)
                return cursor.rowcount
        except Exception as e:  # noqa: BLE001 - database operation failure metric and re-raise
            self._record_and_reraise(e)

    async def fetchall(self, query: str, params: tuple[Any, ...] | None = None) -> list[dict[str, Any]]:
        """
        执行查询并返回所有结果行

        Args:
            query: SQL查询语句
            params: 查询参数

        Returns:
            结果行列表
        """
        self._last_used_at = time.time()
        self._query_count += 1

        try:
            async with self._connection.cursor(row_factory=dict_row) as cursor:
                await cursor.execute(query, params)
                return await cursor.fetchall()
        except Exception as e:  # noqa: BLE001 - database operation failure metric and re-raise
            self._record_and_reraise(e)

    async def fetchrow(self, query: str, params: tuple[Any, ...] | None = None) -> dict[str, Any] | None:
        """
        执行查询并返回单行结果

        Args:
            query: SQL查询语句
            params: 查询参数

        Returns:
            单行结果或 None
        """
        self._last_used_at = time.time()
        self._query_count += 1

        try:
            async with self._connection.cursor(row_factory=dict_row) as cursor:
                await cursor.execute(query, params)
                return await cursor.fetchone()
        except Exception as e:  # noqa: BLE001 - database operation failure metric and re-raise
            self._record_and_reraise(e)

    async def close(self) -> None:
        """
        关闭连接(实际上是归还到池)
        """
        if not self._closed and self._pool:
            checkin_start = time.perf_counter()
            await self._pool.putconn(self._connection)
            if self._owner_pool is not None:
                self._owner_pool._log_connection_checkin(time.perf_counter() - checkin_start)
                # 更新连接池统计信息
                self._owner_pool._update_connection_stats(
                    query_count=self._query_count,
                    error_count=self._error_count,
                    lifetime=time.time() - self._created_at,
                )
            self._closed = True

    async def commit(self) -> None:
        """提交事务"""
        await self._connection.commit()

    async def rollback(self) -> None:
        """回滚事务"""
        await self._connection.rollback()

    def get_stats(self) -> dict[str, Any]:
        """获取连接统计信息"""
        return {
            "created_at": self._created_at,
            "last_used_at": self._last_used_at,
            "query_count": self._query_count,
            "error_count": self._error_count,
            "lifetime_seconds": time.time() - self._created_at,
            "idle_seconds": time.time() - self._last_used_at,
            "is_closed": self._closed,
        }

    async def __aenter__(self) -> "AsyncPooledConnection":
        """异步上下文管理器入口"""
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> None:
        """异步上下文管理器出口"""
        if exc_type is not None:
            try:
                await self.rollback()
            except Exception as e:  # noqa: BLE001 - rollback failure during cleanup must not mask the original exception
                logger.warning("Failed to rollback transaction: %s", e, exc_info=True)
        await self.close()


class AsyncConnectionTransactionManager:
    """
    异步事务管理器

    提供与原有自研事务管理器兼容的接口
    """

    def __init__(
        self,
        connection: AsyncPooledConnection,
        owner_pool: "AsyncPooledDatabaseConnection" | None = None,
        timeout_seconds: int | None = None,
    ):
        """
        初始化事务管理器

        Args:
            connection: 连接包装器
        """
        self._connection = connection
        self._owner_pool = owner_pool
        self._is_active = False
        self._timeout_seconds = max(1, int(timeout_seconds or 60))
        self._started_at: float | None = None

    def _is_timed_out(self) -> bool:
        if not self._is_active or self._started_at is None:
            return False
        return (time.time() - self._started_at) > self._timeout_seconds

    async def _rollback_if_needed(self) -> None:
        if not self._is_active:
            return
        try:
            await self._connection.execute("ROLLBACK")
        finally:
            self._is_active = False
            self._started_at = None
            if self._owner_pool is not None:
                self._owner_pool._log_transaction_rollback()

    @property
    def is_active(self) -> bool:
        """检查事务是否活跃"""
        return self._is_active

    async def begin(self) -> None:
        """开始事务"""
        if not self._is_active:
            await self._connection.execute("BEGIN")
            await self._connection.execute(f"SET LOCAL statement_timeout = {int(self._timeout_seconds * 1000)}")
            self._is_active = True
            self._started_at = time.time()

    async def commit(self) -> None:
        """提交事务"""
        if self._is_active:
            if self._is_timed_out():
                await self._rollback_if_needed()
                raise TimeoutError(f"Async transaction timed out after {self._timeout_seconds}s")
            await self._connection.execute("COMMIT")
            self._is_active = False
            self._started_at = None
            if self._owner_pool is not None:
                self._owner_pool._log_transaction_commit()

    async def rollback(self) -> None:
        """回滚事务"""
        if self._is_active:
            await self._connection.execute("ROLLBACK")
            self._is_active = False
            self._started_at = None
            if self._owner_pool is not None:
                self._owner_pool._log_transaction_rollback()

    async def __aenter__(self) -> "AsyncConnectionTransactionManager":
        """异步上下文管理器入口"""
        await self.begin()
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> None:
        """异步上下文管理器出口"""
        if exc_type:
            await self.rollback()
        else:
            await self.commit()


class AsyncPooledDatabaseConnection(BaseConnectionPool):
    """
    异步连接池实现

    使用psycopg 3官方AsyncConnectionPool,提供与原有自研连接池兼容的接口
    继承BaseConnectionPool基类,复用公共功能

    优化特性:
    - 自动连接健康检查
    - 连接生命周期管理
    - 性能指标收集
    - 自适应连接池大小调整
    """

    def __init__(self, config: DatabaseConfig, pool_config: PoolConfig | None = None) -> None:
        """
        初始化异步连接池

        Args:
            config: 数据库配置
            pool_config: 连接池配置
        """
        super().__init__(config, pool_config)

        # 验证连接池配置
        self._validate_pool_config(self.pool_config)

        # 使用psycopg官方的连接池
        self._pool: AsyncConnectionPool | None = None
        self._initialized = False
        self._init_lock = asyncio.Lock()  # 线程安全的初始化锁
        self._init_event = asyncio.Event()
        self._init_error: Exception | None = None
        self._initializing = False

        # 事务超时配置(优先读取 pool 配置中的 connection_timeout)
        self._transaction_timeout_seconds = max(
            1,
            int(getattr(self.pool_config, "connection_timeout", 30) or 30),
        )

        # 性能统计
        self._total_queries = 0
        self._total_errors = 0
        self._total_connections_created = 0
        self._total_connections_closed = 0
        self._total_connection_wait_time = 0.0
        self._stats_lock = asyncio.Lock()

        # 延迟初始化,在首次使用时初始化

    async def _initialize_pool(self) -> None:
        """
        初始化连接池(线程安全)

        Raises:
            psycopg.Error: 如果PostgreSQL连接错误
            Exception: 如果初始化失败
        """
        if self._initialized and self._pool:
            return

        should_initialize = False
        async with self._init_lock:
            # 双重检查,避免重复初始化
            if self._initialized and self._pool:
                return

            if self._initializing:
                should_initialize = False
            else:
                self._initializing = True
                self._init_event.clear()
                self._init_error = None
                should_initialize = True

        if not should_initialize:
            await self._init_event.wait()
            if self._init_error is not None:
                raise self._init_error
            return

        try:
            logger.info(f"{self._log_prefix} Initializing async PostgreSQL connection pool...")

            loop = asyncio.get_running_loop()
            if loop.__class__.__name__ == "ProactorEventLoop":
                raise RuntimeError(
                    "Psycopg async pool is incompatible with ProactorEventLoop on Windows. "
                    "Please use WindowsSelectorEventLoopPolicy in the host startup path before creating async database connections."
                )

            # 准备连接参数
            connection_params = EncodingHandler.prepare_connection_params(self.config)

            # 创建连接池
            # RuntimeWarning: opening the async pool AsyncConnectionPool in the constructor is deprecated
            # We use open=False and then explicitly await pool.open()
            _min = self.pool_config.min_connections if self.pool_config else 2
            _max = self.pool_config.max_connections if self.pool_config else 10
            pool_kwargs: dict[str, Any] = {
                "min_size": _min,
                "max_size": _max,
                "open": False,
                "kwargs": {
                    **connection_params,
                    "row_factory": dict_row,
                    "autocommit": True,
                },
            }
            try:
                signature = inspect.signature(AsyncConnectionPool.__init__)
                if "max_idle" in signature.parameters:
                    pool_kwargs["max_idle"] = _min
            except (TypeError, ValueError):
                # 某些实现可能无法反射签名,保守使用默认参数集合。
                pass

            self._pool = AsyncConnectionPool(**pool_kwargs)

            await self._pool.open()
            self._initialized = True

            # 记录初始化完成
            self._log_initialization_complete(self._pool.min_size)

        except psycopg.Error as e:
            logger.error(f"{self._log_prefix} PostgreSQL connection error during pool initialization: {e}")
            self._log_initialization_error(e)
            self._init_error = e
            raise
        except Exception as e:
            logger.error(f"{self._log_prefix} Failed to initialize async connection pool: {e}")
            self._log_initialization_error(e)
            self._init_error = e
            raise
        finally:
            async with self._init_lock:
                self._initializing = False
                self._init_event.set()

    async def _ensure_initialized(self) -> None:
        """
        确保连接池已初始化

        Raises:
            Exception: 如果初始化失败
        """
        if self._initialized and self._pool:
            return

        if self._initializing:
            await self._init_event.wait()
            if self._init_error is not None:
                raise self._init_error
            if self._initialized and self._pool:
                return

        await self._initialize_pool()

    async def get_connection(self, timeout: int | None = None) -> AsyncPooledConnection:
        """
        获取连接

        Args:
            timeout: 连接超时时间(秒)

        Returns:
            连接包装器对象

        Raises:
            Exception: 如果获取连接失败
        """
        await self._ensure_initialized()
        pool = self._pool
        if pool is None:
            raise RuntimeError("Connection pool is not initialized")
        wait_start = time.perf_counter()
        try:
            if timeout is not None:
                conn = await asyncio.wait_for(pool.getconn(), timeout=float(timeout))
            else:
                conn = await pool.getconn()
        except TimeoutError:
            self.log_timeout_error()
            raise
        wait_duration = time.perf_counter() - wait_start
        try:
            self.log_pool_hit()
            self._log_connection_wait(wait_duration)
            self._log_connection_checkout(wait_duration)

            # 更新统计信息
            self._total_connections_created += 1
            self._total_connection_wait_time += wait_duration

            return AsyncPooledConnection(
                conn,
                pool,
                owner_pool=self,
                transaction_timeout_seconds=self._transaction_timeout_seconds,
            )
        except Exception as exc:
            logger.warning("wrapping pooled connection failed; returning raw connection to pool", exc_info=exc)
            await pool.putconn(conn)
            raise

    async def return_connection(self, conn: AsyncPooledConnection) -> None:
        """
        归还连接

        Args:
            conn: 连接包装器对象
        """
        if self._pool:
            checkin_start = time.perf_counter()
            await self._pool.putconn(conn.connection)
            self._log_connection_checkin(time.perf_counter() - checkin_start)

    async def close(self) -> None:
        """
        关闭连接池
        """
        if self._pool:
            self._log_disposal_start()
            await self._pool.close()
            self._initialized = False
            self._pool = None
            self._init_error = None
            self._init_event = asyncio.Event()
            self._log_disposal_complete()

    def _update_connection_stats(
        self,
        query_count: int = 0,
        error_count: int = 0,
        lifetime: float = 0.0,
    ) -> None:
        """更新连接统计信息(非线程安全,需要在事件循环中调用)"""
        self._total_queries += query_count
        self._total_errors += error_count
        self._total_connections_closed += 1

    async def get_pool_stats(self) -> dict[str, Any]:
        """
        获取连接池统计信息

        Returns:
            包含统计信息的字典
        """
        pool_stats = {}
        if self._pool:
            pool_stats = {
                "min_size": self._pool.min_size,
                "max_size": self._pool.max_size,
                "idle_connections": getattr(self._pool, "idle_connections", None),
                "active_connections": getattr(self._pool, "active_connections", None),
            }

        return {
            **pool_stats,
            "total_queries": self._total_queries,
            "total_errors": self._total_errors,
            "total_connections_created": self._total_connections_created,
            "total_connections_closed": self._total_connections_closed,
            "error_rate": (self._total_errors / self._total_queries if self._total_queries > 0 else 0.0),
            "initialized": self._initialized,
        }

    @asynccontextmanager
    async def transaction_context(self, timeout: int | None = None) -> AsyncGenerator[AsyncPooledConnection, None]:
        """
        事务上下文管理器

        Args:
            timeout: 连接超时时间(秒)

        Yields:
            连接包装器对象(处于事务中)
        """
        await self._ensure_initialized()
        async with self.connection_context(timeout=timeout) as conn, conn.transaction_manager:
            yield conn

    @asynccontextmanager
    async def connection_context(self, timeout: int | None = None) -> AsyncGenerator[AsyncPooledConnection, None]:
        """
        连接上下文管理器

        Args:
            timeout: 连接超时时间(秒)

        Yields:
            连接包装器对象
        """
        await self._ensure_initialized()
        pool = self._pool
        if pool is None:
            raise RuntimeError("Connection pool is not initialized")
        # 获取连接
        wait_start = time.perf_counter()
        pooled_conn: AsyncPooledConnection | None = None
        try:
            if timeout is not None:
                conn = await asyncio.wait_for(pool.getconn(), timeout=float(timeout))
            else:
                conn = await pool.getconn()
        except TimeoutError:
            self.log_timeout_error()
            raise
        wait_duration = time.perf_counter() - wait_start
        try:
            # 创建连接包装器
            self.log_pool_hit()
            self._log_connection_wait(wait_duration)
            self._log_connection_checkout(wait_duration)
            pooled_conn = AsyncPooledConnection(
                conn,
                pool,
                owner_pool=self,
                transaction_timeout_seconds=self._transaction_timeout_seconds,
            )
            yield pooled_conn
        except Exception as e:
            logger.error("%s Error in connection context: %s", self._log_prefix, e)
            raise
        finally:
            # 通过包装器归还连接,避免双重归还
            if pooled_conn is not None and not pooled_conn._closed:
                await pooled_conn.close()

    @asynccontextmanager
    async def acquire(self, timeout: int | None = None) -> AsyncGenerator[AsyncPooledConnection, None]:
        """兼容 asyncpg 风格的连接获取接口。"""
        async with self.connection_context(timeout=timeout) as conn:
            yield conn

    def __del__(self) -> None:
        """
        析构函数,确保连接池关闭

        注意:不要在析构函数中使用asyncio.run(),因为可能在事件循环之外
        连接池会由Python垃圾回收自动处理
        """
        pass


class AsyncConnectionPoolManager:
    """
    异步连接池管理器

    提供对异步连接池的高级访问接口,简化连接池的使用
    """

    def __init__(self, config: DatabaseConfig, pool_config: PoolConfig | None = None) -> None:
        """
        初始化异步连接池管理器

        Args:
            config: 数据库配置
            pool_config: 连接池配置
        """
        self._pool = AsyncPooledDatabaseConnection(config, pool_config)

    async def get_connection(self, timeout: int | None = None) -> AsyncPooledConnection:
        """
        获取连接

        Args:
            timeout: 连接超时时间(秒)

        Returns:
            连接包装器对象
        """
        return await self._pool.get_connection(timeout)

    async def transaction(self, timeout: int | None = None) -> AbstractAsyncContextManager[AsyncPooledConnection]:
        """
        获取事务上下文管理器

        Args:
            timeout: 连接超时时间(秒)

        Returns:
            事务上下文管理器对象
        """
        return self._pool.transaction_context(timeout)

    async def connection_context(
        self, timeout: int | None = None
    ) -> AbstractAsyncContextManager[AsyncPooledConnection]:
        """
        获取连接上下文管理器

        Args:
            timeout: 连接超时时间(秒)

        Returns:
            连接上下文管理器对象
        """
        return self._pool.connection_context(timeout)

    def get_pooled_connection(self) -> AsyncPooledDatabaseConnection:
        """
        获取连接池实例

        Returns:
            连接池对象
        """
        return self._pool

    async def health_check(self) -> bool:
        """
        执行健康检查

        Returns:
            如果连接池健康则返回True
        """
        return self._pool.is_active()

    def get_metrics(self) -> dict[str, Any]:
        """
        获取连接池指标

        Returns:
            包含指标的字典
        """
        return self._pool.get_metrics_dict()

    async def get_pool_stats(self) -> dict[str, Any]:
        """
        获取连接池统计信息

        Returns:
            包含统计信息的字典
        """
        return await self._pool.get_pool_stats()

    async def close(self) -> None:
        """关闭连接池管理器"""
        await self._pool.close()

    async def __aenter__(self) -> "AsyncConnectionPoolManager":
        """异步上下文管理器入口"""
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> None:
        """异步上下文管理器出口"""
        await self.close()


async def create_async_connection_pool(
    config: DatabaseConfig,
    max_connections: int = 10,
    min_connections: int = 2,
    **kwargs: Any,
) -> AsyncPooledDatabaseConnection:
    """Create and initialize an async pooled connection."""
    pool_config = PoolConfig(max_connections=max_connections, min_connections=min_connections, **kwargs)
    pool = AsyncPooledDatabaseConnection(config, pool_config)
    await pool._ensure_initialized()
    return pool


async def create_async_connection_pool_manager(
    config: DatabaseConfig,
    max_connections: int = 10,
    min_connections: int = 2,
    **kwargs: Any,
) -> AsyncConnectionPoolManager:
    """Create and initialize an async connection pool manager."""
    pool_config = PoolConfig(max_connections=max_connections, min_connections=min_connections, **kwargs)
    manager = AsyncConnectionPoolManager(config, pool_config)
    await manager._pool._ensure_initialized()
    return manager
