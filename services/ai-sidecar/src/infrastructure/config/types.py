from typing import Any

ConfigData = dict[str, Any]


class ConfigSource:
    def __init__(self, name: str = "", data: ConfigData | None = None):
        self.name = name
        self.data = data or {}


class ConfigChangeListener:
    def on_config_changed(self, key: str, old_value: Any, new_value: Any) -> None:
        pass
