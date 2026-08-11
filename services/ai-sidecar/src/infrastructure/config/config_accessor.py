import copy
from typing import Any

from .types import ConfigData


class ConfigAccessor:
    def __init__(self, data: ConfigData):
        self._data: ConfigData = data

    def get(self, path: str, default: Any = None) -> Any:
        keys = path.split(".")
        current = self._data
        for key in keys:
            if isinstance(current, dict) and key in current:
                current = current[key]
            else:
                return default
        return current

    def get_str(self, path: str, default: str = "") -> str:
        value = self.get(path, default)
        return str(value) if value is not None else default

    def get_int(self, path: str, default: int = 0) -> int:
        value = self.get(path, default)
        try:
            return int(value)
        except (TypeError, ValueError):
            return default

    def get_float(self, path: str, default: float = 0.0) -> float:
        value = self.get(path, default)
        try:
            return float(value)
        except (TypeError, ValueError):
            return default

    def get_bool(self, path: str, default: bool = False) -> bool:
        value = self.get(path, default)
        if isinstance(value, bool):
            return value
        if isinstance(value, str):
            return value.lower() in ("true", "1", "yes")
        return default

    def get_list(self, path: str, default: list | None = None) -> list:
        value = self.get(path, default or [])
        return value if isinstance(value, list) else (default or [])

    def get_dict(self, path: str, default: dict | None = None) -> dict:
        value = self.get(path, default or {})
        return value if isinstance(value, dict) else (default or {})

    def set(self, path: str, value: Any) -> None:
        keys = path.split(".")
        current = self._data
        for key in keys[:-1]:
            if key not in current or not isinstance(current[key], dict):
                current[key] = {}
            current = current[key]
        current[keys[-1]] = value

    def has(self, path: str) -> bool:
        keys = path.split(".")
        current = self._data
        for key in keys:
            if isinstance(current, dict) and key in current:
                current = current[key]
            else:
                return False
        return True

    def keys(self, path: str = "") -> list[str]:
        if not path:
            return list(self._data.keys())
        value = self.get(path)
        if isinstance(value, dict):
            return list(value.keys())
        return []

    def to_dict(self) -> ConfigData:
        return copy.deepcopy(self._data)
