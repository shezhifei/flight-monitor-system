"""数据库连接管理

提供PostgreSQL数据库连接的抽象和管理，支持配置外部化
"""

import threading
from abc import ABC, abstractmethod
from contextlib import contextmanager
from typing import Any

import psycopg
from psycopg.rows import dict_row

from src.infrastructure.logging.core import get_logger

from .encoding_handler import EncodingHandler
from .pool_models import DatabaseConfig

logger = get_logger(__name__)


class DatabaseConnectionInterface(ABC):
    """数据库连接接口"""

    @abstractmethod
    def connect(self) -> Any:
        """建立数据库连接"""

    @abstractmethod
    def close(self) -> None:
        """关闭数据库连接"""

    @abstractmethod
    def execute_query(self, query: str, params: tuple | None = None) -> Any:
        """执行查询"""

    @abstractmethod
    def execute_update(self, query: str, params: tuple | None = None) -> int:
        """执行更新操作"""

    @abstractmethod
    def get_config(self) -> DatabaseConfig:
        """获取数据库配置"""


class PostgreSQLDatabaseConnection(DatabaseConnectionInterface):
    """PostgreSQL数据库连接实现"""

    def __init__(self, config: DatabaseConfig | None = None):
        """
        初始化PostgreSQL数据库连接

        Args:
            config: 数据库配置
        """
        self.config = config or DatabaseConfig()
        self._local = threading.local()
        self._connection_stats = {"connections_created": 0, "queries_executed": 0, "updates_executed": 0}
        self._lock = threading.RLock()

    def dispose(self) -> None:
        """释放资源"""
        self.close()

    def connect(self) -> psycopg.Connection:
        """建立数据库连接"""
        with self._lock:
            if not hasattr(self._local, "connection") or (
                hasattr(self._local, "connection")
                and hasattr(self._local.connection, "closed")
                and self._local.connection.closed
            ):
                try:
                    # 使用 EncodingHandler 准备安全的连接参数
                    connection_params = EncodingHandler.prepare_connection_params(self.config)

                    self._local.connection = psycopg.connect(**connection_params)

                    # 添加SSL配置
                    if self.config.enable_ssl:
                        self._local.connection.set_session(sslmode="require")

                    # 配置连接
                    self._configure_connection(self._local.connection)

                    self._connection_stats["connections_created"] += 1

                except psycopg.Error as e:
                    logger.error(f"Failed to create PostgreSQL connection: {e}")
                    raise ConnectionError(f"Database connection failed: {e}") from e

            return self._local.connection

    def close(self) -> None:
        """关闭数据库连接"""
        with self._lock:
            if hasattr(self._local, "connection") and self._local.connection:
                try:
                    if not self._local.connection.closed:
                        self._local.connection.close()
                except psycopg.Error as e:
                    logger.warning(f"Error closing PostgreSQL connection: {e}")
                finally:
                    # 确保无论如何都移除引用
                    if hasattr(self._local, "connection"):
                        delattr(self._local, "connection")

    def execute_query(self, query: str, params: tuple | None = None) -> Any:
        """执行查询"""
        conn = self.connect()
        try:
            # 使用RealDictCursor获取字典形式的结果
            with conn.cursor(row_factory=dict_row) as cursor:
                if params:
                    cursor.execute(query, params)
                else:
                    cursor.execute(query)

                result = cursor.fetchall()
                # 转换为字典列表
                result_list = [dict(row) for row in result]

                self._connection_stats["queries_executed"] += 1
                return result_list

        except psycopg.Error as e:
            logger.error(f"Query execution failed: {e}")
            raise

    def execute_update(self, query: str, params: tuple | None = None) -> int:
        """执行更新操作"""
        conn = self.connect()
        try:
            with conn.cursor() as cursor:
                if params:
                    cursor.execute(query, params)
                else:
                    cursor.execute(query)

                conn.commit()
                rowcount = cursor.rowcount
                self._connection_stats["updates_executed"] += 1
                return rowcount

        except psycopg.Error as e:
            conn.rollback()
            logger.error(f"Update execution failed: {e}")
            raise

    def get_config(self) -> DatabaseConfig:
        """获取数据库配置"""
        return self.config

    def get_stats(self) -> dict[str, Any]:
        """获取连接统计信息"""
        return self._connection_stats.copy()

    def _configure_connection(self, conn: psycopg.Connection) -> None:
        """配置数据库连接"""
        try:
            # 设置隔离级别为READ COMMITTED（PostgreSQL默认）
            conn.isolation_level = psycopg.IsolationLevel.READ_COMMITTED

            # 设置自动提交为False，由我们手动控制事务
            conn.autocommit = False

            # PostgreSQL默认启用外键约束，无需额外配置

        except psycopg.Error as e:
            logger.warning(f"Error configuring PostgreSQL connection: {e}")
        except Exception as e:  # noqa: BLE001 - unexpected connection config errors must not crash
            logger.warning(f"Unexpected error configuring PostgreSQL connection: {e}")

    @contextmanager
    def transaction_context(self):
        """事务上下文管理器"""
        conn = self.connect()
        try:
            yield conn
            conn.commit()
        except Exception as exc:
            logger.warning("transaction_context rolled back: %s", exc)
            conn.rollback()
            raise


class DatabaseSessionFactory:
    """PostgreSQL数据库会话工厂"""

    def __init__(self, connection: DatabaseConnectionInterface):
        """
        初始化PostgreSQL数据库会话工厂

        Args:
            connection: PostgreSQL数据库连接接口
        """
        self.connection = connection
        self.logger = get_logger(__name__)

    @contextmanager
    def get_session(self):
        """获取PostgreSQL数据库会话"""
        conn = self.connection.connect()
        transaction_started = False
        try:
            # 确保连接处于正确的事务状态
            if not conn.autocommit:
                transaction_started = True
            yield conn

            # 只有在事务已启动的情况下才提交
            if transaction_started and hasattr(conn, "commit"):
                conn.commit()
        except Exception:
            # 回滚事务
            if hasattr(conn, "rollback"):
                conn.rollback()
            raise
        finally:
            if hasattr(conn, "close"):
                conn.close()


class AsyncDatabaseConnectionInterface(ABC):
    @abstractmethod
    async def connect(self):
        pass

    @abstractmethod
    def transaction_context(self):
        pass

    @abstractmethod
    def connection_context(self):
        pass

    @abstractmethod
    async def execute_query(self, query: str, params: tuple | None = None) -> list[dict[str, Any]]:
        pass

    @abstractmethod
    async def execute_update(self, query: str, params: tuple | None = None) -> int:
        pass


class AsyncPostgreSQLDatabaseConnection(AsyncDatabaseConnectionInterface):
    def __init__(self, config: "DatabaseConfig" = None):
        self.config = config

    async def connect(self):
        import psycopg

        # 使用 EncodingHandler 准备安全的连接参数，避免密码明文拼接
        connection_params = EncodingHandler.prepare_connection_params(self.config)
        return await psycopg.AsyncConnection.connect(**connection_params)

    def transaction_context(self):
        raise NotImplementedError("Use AsyncPooledDatabaseConnection")

    def connection_context(self):
        raise NotImplementedError("Use AsyncPooledDatabaseConnection")

    async def execute_query(self, query: str, params: tuple | None = None) -> list[dict[str, Any]]:
        import psycopg
        import psycopg.rows

        conn = await self.connect()
        try:
            async with conn.cursor(row_factory=psycopg.rows.dict_row) as cur:
                await cur.execute(query, params)
                return await cur.fetchall()
        finally:
            await conn.close()

    async def execute_update(self, query: str, params: tuple | None = None) -> int:
        conn = await self.connect()
        try:
            async with conn.cursor() as cur:
                await cur.execute(query, params)
                await conn.commit()
                return cur.rowcount
        finally:
            await conn.close()
