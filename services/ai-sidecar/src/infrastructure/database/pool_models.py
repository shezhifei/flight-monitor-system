"""Database configuration models shared by database connection modules."""

from dataclasses import dataclass


@dataclass
class DatabaseConfig:
    """PostgreSQL database connection configuration."""

    host: str = "localhost"
    port: int = 5432
    database: str = ""
    user: str = ""
    password: str = ""
    enable_ssl: bool = False
    ssl_mode: str | None = None
    connect_timeout: int = 30
    application_name: str = "ai-sidecar"


@dataclass
class PoolConfig:
    """Connection pool configuration."""

    min_connections: int = 2
    max_connections: int = 10
    connection_timeout: int | None = 30
    command_timeout: float | None = None
    idle_timeout: float = 300.0
    max_lifetime: float = 1800.0
    health_check_interval: float = 60.0
    max_retries: int = 3
    retry_delay: float = 1.0
