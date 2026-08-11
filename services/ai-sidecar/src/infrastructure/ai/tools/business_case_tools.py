"""业务事项 AI 工具定义。"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class BusinessCaseToolName(StrEnum):
    CREATE = "create_business_case"
    LIST = "list_business_cases"
    GET = "get_business_case"
    # 保留枚举值仅用于兼容历史配置，不再对 AI 暴露。
    CLOSE = "close_business_case"
    UPDATE = "update_business_case"


BUSINESS_CASE_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=BusinessCaseToolName.CREATE.value,
        description="创建一个新的航班业务事项，用于触发业务工作流和通知。",
        parameters={
            "case_type": {
                "type": "string",
                "description": "业务事项类型代码（从 business_case_types 表中获取）",
                "minLength": 1,
                "maxLength": 50,
            },
            "flight_id": {
                "type": "string",
                "description": "关联的航班ID",
                "minLength": 1,
                "maxLength": 36,
            },
            "description": {
                "type": "string",
                "description": "业务事项描述",
                "minLength": 1,
                "maxLength": 500,
            },
            "context": {
                "type": "object",
                "description": "业务事项上下文数据，包含与业务事项相关的额外信息",
                "additionalProperties": True,
            },
        },
        required_params=["case_type", "flight_id", "description"],
        category=ToolCategory.BUSINESS_CASE,
        operation_level=OperationLevel.ASSISTED_WRITE,
        side_effect=True,
    ),
    BaseToolDefinition(
        name=BusinessCaseToolName.LIST.value,
        description="获取业务事项列表，可以按航班ID、类型和状态筛选。",
        parameters={
            "flight_id": {
                "type": "string",
                "description": "航班ID，用于筛选特定航班的业务事项",
                "minLength": 1,
                "maxLength": 36,
            },
            "case_type": {
                "type": "string",
                "description": "业务事项类型，用于筛选特定类型的业务事项",
                "minLength": 1,
                "maxLength": 50,
            },
            "status": {
                "type": "string",
                "description": "业务事项状态，用于筛选特定状态的业务事项",
                "minLength": 1,
                "maxLength": 20,
            },
        },
        required_params=[],
        category=ToolCategory.BUSINESS_CASE,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=BusinessCaseToolName.GET.value,
        description="根据ID获取业务事项详情。",
        parameters={
            "case_id": {
                "type": "string",
                "description": "业务事项ID",
                "minLength": 1,
                "maxLength": 36,
            },
        },
        required_params=["case_id"],
        category=ToolCategory.BUSINESS_CASE,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=BusinessCaseToolName.UPDATE.value,
        description="更新业务事项的信息。",
        parameters={
            "case_id": {
                "type": "string",
                "description": "业务事项ID",
                "minLength": 1,
                "maxLength": 36,
            },
            "case_type": {
                "type": "string",
                "description": "业务事项类型",
                "minLength": 1,
                "maxLength": 50,
            },
            "description": {
                "type": "string",
                "description": "业务事项描述",
                "minLength": 1,
                "maxLength": 500,
            },
            "context": {
                "type": "object",
                "description": "业务事项上下文数据",
                "additionalProperties": True,
            },
            "status": {
                "type": "string",
                "description": "业务事项状态",
                "minLength": 1,
                "maxLength": 20,
            },
            "stand": {
                "type": "string",
                "description": "机位",
                "maxLength": 10,
            },
            "gate": {
                "type": "string",
                "description": "登机口",
                "maxLength": 10,
            },
        },
        required_params=["case_id"],
        category=ToolCategory.BUSINESS_CASE,
        operation_level=OperationLevel.ASSISTED_WRITE,
        side_effect=True,
    ),
]


BUSINESS_CASE_TOOLS: list[dict[str, Any]] = build_openai_tools(BUSINESS_CASE_TOOL_DEFINITIONS)


def get_business_case_tools() -> list[BaseToolDefinition]:
    return BUSINESS_CASE_TOOL_DEFINITIONS


__all__ = [
    "BUSINESS_CASE_TOOLS",
    "BUSINESS_CASE_TOOL_DEFINITIONS",
    "BusinessCaseToolName",
    "get_business_case_tools",
]
