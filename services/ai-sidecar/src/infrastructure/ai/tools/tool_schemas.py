from dataclasses import dataclass
from typing import Any


@dataclass
class ToolCallEvent:
    """Emitted when the LLM requests a tool call."""

    run_id: str
    tool_call_id: str
    tool_name: str
    arguments: dict[str, Any]

    def to_sse_data(self) -> dict[str, Any]:
        return {
            "run_id": self.run_id,
            "tool_call_id": self.tool_call_id,
            "tool_name": self.tool_name,
            "arguments": self.arguments,
        }


@dataclass
class ToolResultEvent:
    """Emitted after a tool has been executed."""

    run_id: str
    tool_call_id: str
    tool_name: str
    result: Any
    error: str | None = None

    def to_sse_data(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "run_id": self.run_id,
            "tool_call_id": self.tool_call_id,
            "tool_name": self.tool_name,
            "result": self.result,
        }
        if self.error:
            payload["error"] = self.error
        return payload
