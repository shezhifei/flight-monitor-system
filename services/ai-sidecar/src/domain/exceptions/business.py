"""业务规则异常模块。

低频规则异常收敛到通用 RuleViolationException，并保留兼容入口。
"""

from .base import BusinessRuleException, DomainException, ErrorCodes


class RuleViolationException(BusinessRuleException):
    """通用业务规则异常。"""

    def __init__(self, message: str, rule_name: str, error_code: str = ErrorCodes.BUSINESS_RULE_VIOLATION):
        super().__init__(
            message=message,
            rule_name=rule_name,
            error_code=error_code,
        )


def FlightStatusRuleException(flight_number: str, current_status: str, attempted_action: str) -> BusinessRuleException:
    message = f"航班 {flight_number} 当前状态 '{current_status}' 不允许执行操作 '{attempted_action}'"
    return RuleViolationException(message, "FlightStatusRule", "FLIGHT_STATUS_RULE_VIOLATION")


def FlightScheduleRuleException(flight_number: str, message: str) -> BusinessRuleException:
    return RuleViolationException(
        f"航班 {flight_number} 时间安排规则违反: {message}",
        "FlightScheduleRule",
        "FLIGHT_SCHEDULE_RULE_VIOLATION",
    )


def BoardingRuleException(flight_number: str, message: str) -> BusinessRuleException:
    return RuleViolationException(
        f"航班 {flight_number} 登机规则违反: {message}",
        "BoardingRule",
        "BOARDING_RULE_VIOLATION",
    )


def GateAssignmentRuleException(gate: str, flight_number: str, message: str) -> BusinessRuleException:
    return RuleViolationException(
        f"登机口 {gate} 分配给航班 {flight_number} 时违反规则: {message}",
        "GateAssignmentRule",
        "GATE_ASSIGNMENT_RULE_VIOLATION",
    )


def BaggageClaimRuleException(flight_number: str, message: str) -> BusinessRuleException:
    return RuleViolationException(
        f"航班 {flight_number} 行李提取规则违反: {message}",
        "BaggageClaimRule",
        "BAGGAGE_CLAIM_RULE_VIOLATION",
    )


def CapacityRuleException(resource_type: str, resource_id: str, capacity: int, requested: int) -> BusinessRuleException:
    return RuleViolationException(
        f"{resource_type} {resource_id} 容量不足: 容量 {capacity}，请求 {requested}",
        "CapacityRule",
        "CAPACITY_RULE_VIOLATION",
    )


def TimingRuleException(operation: str, message: str) -> BusinessRuleException:
    return RuleViolationException(
        f"操作 {operation} 时间规则违反: {message}",
        "TimingRule",
        "TIMING_RULE_VIOLATION",
    )


def DependencyRuleException(dependency: str, dependent: str, message: str) -> BusinessRuleException:
    return RuleViolationException(
        f"依赖关系违反: {dependency} 依赖于 {dependent}，{message}",
        "DependencyRule",
        "DEPENDENCY_RULE_VIOLATION",
    )


def DuplicateOperationException(operation: str, identifier: str) -> BusinessRuleException:
    return RuleViolationException(
        f"操作 '{operation}' 不能重复执行，标识符: {identifier}",
        "DuplicateOperationRule",
        "DUPLICATE_OPERATION",
    )


def BusinessConstraintException(
    constraint_name: str,
    entity_type: str,
    entity_id: str,
    message: str,
) -> BusinessRuleException:
    return RuleViolationException(
        f"业务约束 '{constraint_name}' 违反: {entity_type} {entity_id}，{message}",
        constraint_name,
        "BUSINESS_CONSTRAINT_VIOLATION",
    )


class FlightNotFoundException(DomainException):
    """航班未找到异常。"""

    def __init__(self, flight_id: str, message: str | None = None):
        self.flight_id = flight_id
        details = {"flight_id": flight_id}
        super().__init__(
            message=message or f"Flight with ID {flight_id} not found",
            error_code=ErrorCodes.RESOURCE_NOT_FOUND,
            details=details,
            status_code=404,
        )
