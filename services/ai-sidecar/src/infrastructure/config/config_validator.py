from typing import Any

from .types import ConfigData


class ConfigValidator:
    def __init__(self):
        self._schemas: dict[str, Any] = {}

    def register_schema(self, name: str, schema: Any) -> None:
        self._schemas[name] = schema

    def validate_config(self, config_data: ConfigData, schema_name: str | None = None) -> bool:
        return True

    def validate_value(self, key: str, value: Any, schema_name: str | None = None) -> bool:
        return True
