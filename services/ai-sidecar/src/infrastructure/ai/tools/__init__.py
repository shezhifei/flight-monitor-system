"""
AI工具 - 统一工具管理

提供与OpenAI function calling兼容的工具定义和执行器。
支持待办事项、航班查询等功能。
"""

# 基础类
from .advisor_tool_executor import AdvisorToolExecutor, SimpleKnowledgeBase

# 处置建议工具
from .advisor_tools import (
    ADVISOR_TOOL_DEFINITIONS,
    ADVISOR_TOOLS,
    AdvisorToolName,
    get_advisor_tools,
)
from .anomaly_tool_executor import AnomalyToolExecutor

# 异常工具
from .anomaly_tools import (
    ANOMALY_TOOL_DEFINITIONS,
    ANOMALY_TOOLS,
    AnomalyToolName,
    get_anomaly_tools,
)
from .base import (
    BaseToolDefinition,
    BaseToolExecutor,
    InvocationMode,
    OperationLevel,
    ToolCategory,
    ToolExecutionError,
    ToolExecutionResult,
    ToolExecutionStatus,
)
from .business_case_tool_executor import BusinessCaseToolExecutor

# 业务事项工具
from .business_case_tools import (
    BUSINESS_CASE_TOOLS,
    BusinessCaseToolName,
    get_business_case_tools,
)
from .dispatch_command_executor import DispatchCommandExecutor

# 调度操作工具 (EP-08)
from .dispatch_command_tools import (
    DISPATCH_COMMAND_DEFINITIONS,
    DISPATCH_COMMAND_TOOLS,
    DispatchCommandToolName,
    get_dispatch_command_tools,
)
from .dispatch_query_executor import DispatchQueryExecutor

# 派工单查询工具
from .dispatch_query_tools import (
    DISPATCH_QUERY_TOOL_DEFINITIONS,
    DISPATCH_QUERY_TOOLS,
    DispatchQueryToolName,
    get_dispatch_query_tools,
)
from .equipment_tool_executor import EquipmentToolExecutor

# 设备工具
from .equipment_tools import (
    EQUIPMENT_TOOL_DEFINITIONS,
    EQUIPMENT_TOOLS,
    EquipmentToolName,
    get_equipment_tools,
)
from .flight_executor import FlightToolExecutor

# 航班工具
from .flight_tools import (
    FLIGHT_TOOL_DEFINITIONS,
    FLIGHT_TOOLS,
    FlightToolName,
    get_flight_tools,
)
from .pending_actions import (
    MemoryPendingActionStore,
    PendingAction,
    PendingActionConflictError,
    PendingActionStatus,
    PendingActionStore,
    PendingActionStoreProtocol,
    PostgresPendingActionStore,
    get_pending_action_store,
    set_pending_action_store,
)
from .query_tool_executor import QueryToolExecutor  # pkg

# 查询工具
from .query_tools import (
    QUERY_TOOL_DEFINITIONS,
    QUERY_TOOLS,
    QueryToolName,
    get_query_tools,
)

# 注册表
from .registry import (
    ToolRegistry,
    get_tool_registry,
)
from .report_tool_executor import ReportToolExecutor

# 报告生成工具
from .report_tools import (
    REPORT_TOOL_DEFINITIONS,
    REPORT_TOOLS,
    ReportToolName,
    get_report_tools,
)
from .sql_query_executor import SQLQueryReadOnlyExecutor

# SQL 只读查询工具
from .sql_query_tools import (
    SQL_QUERY_TOOL_DEFINITIONS,
    SQL_QUERY_TOOLS,
    SQLQueryToolName,
    get_sql_query_tools,
)
from .stand_tool_executor import StandToolExecutor

# 机位工具
from .stand_tools import (
    STAND_TOOL_DEFINITIONS,
    STAND_TOOLS,
    StandToolName,
    get_stand_tools,
)
from .team_tool_executor import TeamToolExecutor

# 班组工具
from .team_tools import (
    TEAM_TOOL_DEFINITIONS,
    TEAM_TOOLS,
    TeamToolName,
    get_team_tools,
)
from .todo_tool_executor import TodoToolExecutor

# 待办事项工具
from .todo_tools import (
    TODO_TOOL_DEFINITIONS,
    TODO_TOOLS,
    TodoToolDefinition,
    get_todo_tools,
)

__all__ = [
    "ADVISOR_TOOLS",
    # 处置建议
    "ADVISOR_TOOL_DEFINITIONS",
    "ANOMALY_TOOLS",
    # 异常
    "ANOMALY_TOOL_DEFINITIONS",
    # 业务事项
    "BUSINESS_CASE_TOOLS",
    # 调度操作 (EP-08)
    "DISPATCH_COMMAND_DEFINITIONS",
    "DISPATCH_COMMAND_TOOLS",
    "DISPATCH_QUERY_TOOLS",
    # 派工单查询
    "DISPATCH_QUERY_TOOL_DEFINITIONS",
    "EQUIPMENT_TOOLS",
    # 设备
    "EQUIPMENT_TOOL_DEFINITIONS",
    # 航班
    "FLIGHT_TOOLS",
    "FLIGHT_TOOL_DEFINITIONS",
    "QUERY_TOOLS",
    # 查询
    "QUERY_TOOL_DEFINITIONS",
    "REPORT_TOOLS",
    # 报告生成
    "REPORT_TOOL_DEFINITIONS",
    "SQL_QUERY_TOOLS",
    "SQL_QUERY_TOOL_DEFINITIONS",
    "STAND_TOOLS",
    # 机位
    "STAND_TOOL_DEFINITIONS",
    "TEAM_TOOLS",
    # 班组
    "TEAM_TOOL_DEFINITIONS",
    # 待办事项
    "TODO_TOOLS",
    "TODO_TOOL_DEFINITIONS",
    "AdvisorToolExecutor",
    "AdvisorToolName",
    "AnomalyToolExecutor",
    "AnomalyToolName",
    "BaseToolDefinition",
    "BaseToolExecutor",
    "BusinessCaseToolExecutor",
    "BusinessCaseToolName",
    "DispatchCommandExecutor",
    "DispatchCommandToolName",
    "DispatchQueryExecutor",
    "DispatchQueryToolName",
    "EquipmentToolExecutor",
    "EquipmentToolName",
    "FlightToolExecutor",
    "FlightToolName",
    "InvocationMode",
    "MemoryPendingActionStore",
    "OperationLevel",
    "PendingAction",
    "PendingActionConflictError",
    "PendingActionStatus",
    "PendingActionStore",
    "PendingActionStoreProtocol",
    "PostgresPendingActionStore",
    "QueryToolExecutor",
    "QueryToolName",
    "ReportToolExecutor",
    "ReportToolName",
    "SQLQueryReadOnlyExecutor",
    "SQLQueryToolName",
    "SimpleKnowledgeBase",
    "StandToolExecutor",
    "StandToolName",
    "TeamToolExecutor",
    "TeamToolName",
    "TodoToolDefinition",
    "TodoToolExecutor",
    # 基础
    "ToolCategory",
    "ToolExecutionError",
    "ToolExecutionResult",
    "ToolExecutionStatus",
    # 注册表
    "ToolRegistry",
    "get_advisor_tools",
    "get_anomaly_tools",
    "get_business_case_tools",
    "get_dispatch_command_tools",
    "get_dispatch_query_tools",
    "get_equipment_tools",
    "get_flight_tools",
    "get_pending_action_store",
    "get_query_tools",
    "get_report_tools",
    "get_sql_query_tools",
    "get_stand_tools",
    "get_team_tools",
    "get_todo_tools",
    "get_tool_registry",
    "set_pending_action_store",
]
