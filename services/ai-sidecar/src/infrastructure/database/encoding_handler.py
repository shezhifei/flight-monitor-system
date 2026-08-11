"""Connection parameter encoding utilities."""

from typing import Any

from .pool_models import DatabaseConfig


class EncodingHandler:
    """Prepares safe psycopg connection parameters from configuration."""

    @staticmethod
    def prepare_connection_params(config: DatabaseConfig) -> dict[str, Any]:
        """Return a dictionary of connection parameters for psycopg."""
        params: dict[str, Any] = {
            "host": config.host,
            "port": config.port,
            "dbname": config.database,
            "user": config.user,
            "password": config.password,
            "connect_timeout": config.connect_timeout,
            "application_name": config.application_name,
        }
        if config.enable_ssl and config.ssl_mode:
            params["sslmode"] = config.ssl_mode
        return params
