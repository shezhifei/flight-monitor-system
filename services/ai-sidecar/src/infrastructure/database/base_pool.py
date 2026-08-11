"""Base connection pool with shared metrics/logging helpers."""

import logging

from .pool_models import DatabaseConfig, PoolConfig

logger = logging.getLogger(__name__)


class BaseConnectionPool:
    """Base class for database connection pools providing common helpers."""

    def __init__(self, config: DatabaseConfig, pool_config: PoolConfig | None = None) -> None:
        self.config = config
        self.pool_config = pool_config or PoolConfig()
        self._log_prefix = f"[{self.__class__.__name__}]"

    def _validate_pool_config(self, pool_config: PoolConfig | None) -> None:
        """Validate pool configuration values."""
        if pool_config is None:
            return
        if pool_config.min_connections < 0:
            raise ValueError("min_connections must be non-negative")
        if pool_config.max_connections < pool_config.min_connections:
            raise ValueError("max_connections must be >= min_connections")

    def _log_initialization_complete(self, pool_size: int) -> None:
        logger.info("%s Pool initialized with size %s", self._log_prefix, pool_size)

    def _log_initialization_error(self, error: BaseException) -> None:
        logger.error("%s Pool initialization error: %s", self._log_prefix, error)

    def _log_connection_checkout(self, duration: float) -> None:
        logger.debug("%s Connection checked out in %.4fs", self._log_prefix, duration)

    def _log_connection_checkin(self, duration: float) -> None:
        logger.debug("%s Connection checked in in %.4fs", self._log_prefix, duration)

    def _log_connection_wait(self, duration: float) -> None:
        logger.debug("%s Connection waited %.4fs", self._log_prefix, duration)

    def log_pool_hit(self) -> None:
        logger.debug("%s Pool hit", self._log_prefix)

    def log_timeout_error(self) -> None:
        logger.warning("%s Connection acquire timeout", self._log_prefix)

    def _log_transaction_commit(self) -> None:
        logger.debug("%s Transaction committed", self._log_prefix)

    def _log_transaction_rollback(self) -> None:
        logger.debug("%s Transaction rolled back", self._log_prefix)

    def _log_disposal_start(self) -> None:
        logger.debug("%s Disposing pool", self._log_prefix)

    def _log_disposal_complete(self) -> None:
        logger.debug("%s Pool disposed", self._log_prefix)

    def _update_connection_stats(
        self,
        query_count: int = 0,
        error_count: int = 0,
        lifetime: float = 0.0,
    ) -> None:
        """Hook for subclasses to aggregate per-connection statistics."""

    def get_metrics_dict(self) -> dict[str, object]:
        """Return generic pool metrics; subclasses may override."""
        return {}

    def is_active(self) -> bool:
        """Return whether the pool is currently active."""
        return False
