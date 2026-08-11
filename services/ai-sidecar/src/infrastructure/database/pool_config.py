"""数据库连接池配置模块

提供连接池的配置参数和优化建议。
"""

import os
from dataclasses import dataclass


@dataclass
class DatabasePoolConfig:
    """数据库连接池配置"""

    # 基础连接配置
    min_connections: int = 2
    max_connections: int = 10

    # 超时配置（秒）
    connection_timeout: float = 30.0
    command_timeout: float = 60.0
    idle_timeout: float = 300.0  # 5分钟
    max_lifetime: float = 1800.0  # 30分钟

    # 健康检查配置
    health_check_interval: float = 60.0  # 1分钟
    health_check_timeout: float = 5.0

    # 重试配置
    max_retries: int = 3
    retry_delay: float = 1.0

    # 性能优化配置
    prepare_threshold: int = 5  # 预编译语句阈值
    prefetch_rows: int = 100  # 预取行数

    @classmethod
    def from_env(cls) -> "DatabasePoolConfig":
        """从环境变量加载配置"""
        return cls(
            min_connections=int(os.getenv("DB_POOL_MIN_CONNECTIONS", "2")),
            max_connections=int(os.getenv("DB_POOL_MAX_CONNECTIONS", "10")),
            connection_timeout=float(os.getenv("DB_POOL_CONNECTION_TIMEOUT", "30.0")),
            command_timeout=float(os.getenv("DB_POOL_COMMAND_TIMEOUT", "60.0")),
            idle_timeout=float(os.getenv("DB_POOL_IDLE_TIMEOUT", "300.0")),
            max_lifetime=float(os.getenv("DB_POOL_MAX_LIFETIME", "1800.0")),
            health_check_interval=float(os.getenv("DB_POOL_HEALTH_CHECK_INTERVAL", "60.0")),
            health_check_timeout=float(os.getenv("DB_POOL_HEALTH_CHECK_TIMEOUT", "5.0")),
            max_retries=int(os.getenv("DB_POOL_MAX_RETRIES", "3")),
            retry_delay=float(os.getenv("DB_POOL_RETRY_DELAY", "1.0")),
            prepare_threshold=int(os.getenv("DB_POOL_PREPARE_THRESHOLD", "5")),
            prefetch_rows=int(os.getenv("DB_POOL_PREFETCH_ROWS", "100")),
        )

    def validate(self) -> None:
        """验证配置参数"""
        if self.min_connections < 0:
            raise ValueError("min_connections must be non-negative")
        if self.max_connections < 1:
            raise ValueError("max_connections must be at least 1")
        if self.min_connections > self.max_connections:
            raise ValueError("min_connections must not exceed max_connections")
        if self.connection_timeout <= 0:
            raise ValueError("connection_timeout must be positive")
        if self.command_timeout <= 0:
            raise ValueError("command_timeout must be positive")
        if self.idle_timeout <= 0:
            raise ValueError("idle_timeout must be positive")
        if self.max_lifetime <= 0:
            raise ValueError("max_lifetime must be positive")
        if self.max_retries < 0:
            raise ValueError("max_retries must be non-negative")


@dataclass
class RedisPoolConfig:
    """Redis 连接池配置"""

    # 连接池配置
    max_connections: int = 200
    min_idle_connections: int = 10

    # 超时配置（秒）
    socket_timeout: float = 5.0
    socket_connect_timeout: float = 5.0
    retry_on_timeout: bool = True

    # 健康检查配置
    health_check_interval: float = 30.0

    # 连接复用配置
    max_idle_time: float = 300.0  # 5分钟

    @classmethod
    def from_env(cls) -> "RedisPoolConfig":
        """从环境变量加载配置"""
        return cls(
            max_connections=int(os.getenv("REDIS_MAX_CONNECTIONS", "200")),
            min_idle_connections=int(os.getenv("REDIS_MIN_IDLE_CONNECTIONS", "10")),
            socket_timeout=float(os.getenv("REDIS_SOCKET_TIMEOUT", "5.0")),
            socket_connect_timeout=float(os.getenv("REDIS_CONNECT_TIMEOUT", "5.0")),
            retry_on_timeout=os.getenv("REDIS_RETRY_ON_TIMEOUT", "true").lower() == "true",
            health_check_interval=float(os.getenv("REDIS_HEALTH_CHECK_INTERVAL", "30.0")),
            max_idle_time=float(os.getenv("REDIS_MAX_IDLE_TIME", "300.0")),
        )

    def validate(self) -> None:
        """验证配置参数"""
        if self.max_connections < 1:
            raise ValueError("max_connections must be at least 1")
        if self.min_idle_connections < 0:
            raise ValueError("min_idle_connections must be non-negative")
        if self.min_idle_connections > self.max_connections:
            raise ValueError("min_idle_connections must not exceed max_connections")
        if self.socket_timeout <= 0:
            raise ValueError("socket_timeout must be positive")
        if self.socket_connect_timeout <= 0:
            raise ValueError("socket_connect_timeout must be positive")


def get_optimal_pool_size(cpu_count: int | None = None) -> int:
    """
    计算最优连接池大小

    基于 CPU 核心数和并发需求计算。
    公式: min(max_connections, cpu_count * 2 + effective_io_concurrency)

    Args:
        cpu_count: CPU 核心数，默认自动检测

    Returns:
        推荐的连接池大小
    """
    if cpu_count is None:
        cpu_count = os.cpu_count() or 4

    # PostgreSQL 的 effective_io_concurrency 通常为 2-4
    effective_io_concurrency = 2

    optimal_size = cpu_count * 2 + effective_io_concurrency

    # 限制在合理范围内
    return max(2, min(optimal_size, 20))


def get_recommended_config(
    workload_type: str = "mixed",
    expected_concurrency: int = 10,
) -> DatabasePoolConfig:
    """
    获取推荐的连接池配置

    Args:
        workload_type: 工作负载类型 ("read_heavy", "write_heavy", "mixed")
        expected_concurrency: 预期并发数

    Returns:
        推荐的数据库连接池配置
    """
    cpu_count = os.cpu_count() or 4

    if workload_type == "read_heavy":
        # 读密集型：更多的连接，更长的空闲超时
        return DatabasePoolConfig(
            min_connections=max(2, cpu_count),
            max_connections=min(expected_concurrency, cpu_count * 3),
            idle_timeout=600.0,  # 10分钟
            max_lifetime=3600.0,  # 1小时
            prefetch_rows=200,
        )
    elif workload_type == "write_heavy":
        # 写密集型：较少的连接，较短的超时
        return DatabasePoolConfig(
            min_connections=2,
            max_connections=min(expected_concurrency, cpu_count * 2),
            idle_timeout=180.0,  # 3分钟
            max_lifetime=900.0,  # 15分钟
            command_timeout=120.0,
        )
    else:
        # 混合型：平衡配置
        return DatabasePoolConfig(
            min_connections=max(2, cpu_count // 2),
            max_connections=min(expected_concurrency, cpu_count * 2 + 2),
            idle_timeout=300.0,
            max_lifetime=1800.0,
        )
