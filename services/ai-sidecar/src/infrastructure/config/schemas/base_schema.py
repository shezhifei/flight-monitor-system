from typing import Any


class BaseSchema:
    def validate(self, data: dict[str, Any]) -> bool:
        return True

    def get_errors(self) -> list[str]:
        return []
