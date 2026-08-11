"""
业务事项工具执行器

处理AI工具调用请求，路由到BusinessCaseService执行实际操作。
"""

from typing import Any

from src.domain.ports.service_interfaces import BusinessCaseServiceInterface

from .base import (
    BaseToolExecutor,
    ToolCategory,
    ToolExecutionError,
    ToolExecutionStatus,
)
from .business_case_inputs import BusinessCaseCreateInput
from .business_case_tools import BusinessCaseToolName


class BusinessCaseToolExecutor(BaseToolExecutor):
    """
    业务事项工具执行器

    将AI的工具调用请求转换为实际的BusinessCaseService操作。
    """

    def __init__(
        self, business_case_service: BusinessCaseServiceInterface = None, default_user: str = "system_ai_agent"
    ):
        super().__init__(default_user)
        self._service = business_case_service

    def _register_handlers(self) -> None:
        """注册工具处理器"""
        self._handlers = {
            BusinessCaseToolName.CREATE.value: self._handle_create_business_case,
            BusinessCaseToolName.LIST.value: self._handle_list_business_cases,
            BusinessCaseToolName.GET.value: self._handle_get_business_case,
            BusinessCaseToolName.UPDATE.value: self._handle_update_business_case,
        }

    def get_category(self) -> ToolCategory:
        """返回此执行器处理的工具类别"""
        return ToolCategory.BUSINESS_CASE

    def set_business_case_service(self, business_case_service: BusinessCaseServiceInterface) -> None:
        self._service = business_case_service

    async def _handle_create_business_case(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理创建业务事项请求"""
        self._ensure_service()
        case_data = self._validate_args(BusinessCaseCreateInput, args)
        result = await self._service.create_business_case(case_data=case_data, created_by=self.default_user)
        return self._success_response(
            data=result,
            message=f"业务事项创建成功: {result['case_id']}",
        )

    async def _handle_list_business_cases(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理获取业务事项列表请求"""
        self._ensure_service()
        result = await self._service.get_business_cases(
            flight_id=args.get("flight_id"), case_type=args.get("case_type"), status=args.get("status")
        )
        return self._success_response(
            data=result,
            total=len(result),
            message=f"获取到 {len(result)} 个业务事项",
        )

    async def _handle_get_business_case(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理获取业务事项详情请求"""
        self._ensure_service()
        case_id = self._require_arg(args, "case_id", "业务事项ID不能为空")
        result = await self._service.get_business_case_by_id(case_id)
        return self._success_response(
            data=result,
            message=f"获取业务事项成功: {case_id}",
        )

    async def _handle_update_business_case(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理更新业务事项请求"""
        return await self._update_case_with_args(args, "更新")

    async def _update_case_with_args(self, args: dict[str, Any], action: str) -> dict[str, Any]:
        """通用业务事项更新方法"""
        self._ensure_service()
        case_id = self._require_arg(args, "case_id", "业务事项ID不能为空")

        case = await self._service.get_business_case_by_id(case_id)
        if not case:
            raise ToolExecutionError(
                f"业务事项不存在: {case_id}",
                ToolExecutionStatus.NOT_FOUND,
            )
        case_data = BusinessCaseCreateInput(
            case_type=args.get("case_type", case["case_type"]),
            flight_id=case["flight_id"],
            description=args.get("description", case["description"]),
            context=args.get("context", case["context"]),
        )
        result = await self._service.update_business_case(
            case_id=case_id, case_data=case_data, updated_by=self.default_user
        )
        return self._success_response(
            data=result,
            message=f"业务事项{action}成功: {case_id}",
        )


# 导出类
__all__ = ["BusinessCaseToolExecutor"]
