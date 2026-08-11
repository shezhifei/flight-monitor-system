"""验证异常模块。

收敛低频异常类型到通用字段验证异常，并统一 error_code 机制。
"""

from typing import Any

from .base import (
    ErrorCodes,
)
from .base import (
    ValidationException as BaseValidationException,
)
from .base import (
    ValueObjectValidationException as BaseValueObjectValidationException,
)


class ValidationException(BaseValidationException):
    """兼容导出：统一使用基础验证异常实现。"""

    def __init__(
        self,
        message: str,
        field: str | None = None,
        value: Any | None = None,
        error_code: str = ErrorCodes.VALIDATION_ERROR,
    ):
        super().__init__(
            message=message,
            field=field,
            value=value,
            error_code=error_code,
        )


ValueObjectValidationException = BaseValueObjectValidationException


class FieldValidationException(ValidationException):
    """通用字段验证异常。"""

    def __init__(
        self,
        *,
        field: str,
        message: str,
        value: Any | None = None,
        error_code: str = ErrorCodes.VALIDATION_ERROR,
    ):
        super().__init__(
            message=message,
            field=field,
            value=value,
            error_code=error_code,
        )


def FlightNumberValidationException(flight_number: str, message: str) -> ValidationException:
    return FieldValidationException(
        field="flight_number",
        value=flight_number,
        message=f"航班号验证失败: {message}",
        error_code="FLIGHT_NUMBER_VALIDATION_ERROR",
    )


def DateTimeValidationException(datetime_value: str, message: str) -> ValidationException:
    return FieldValidationException(
        field="datetime",
        value=datetime_value,
        message=f"日期时间验证失败: {message}",
        error_code="DATETIME_VALIDATION_ERROR",
    )


def RequiredFieldException(field_name: str) -> ValidationException:
    return FieldValidationException(
        field=field_name,
        message=f"字段 '{field_name}' 是必需的",
        error_code=ErrorCodes.REQUIRED_FIELD_MISSING,
    )


def TypeValidationException(
    field_name: str,
    expected_type: str,
    actual_type: str,
    value: Any = None,
) -> ValidationException:
    return FieldValidationException(
        field=field_name,
        value=value,
        message=f"字段 '{field_name}' 类型错误，期望 {expected_type}，实际 {actual_type}",
        error_code="TYPE_VALIDATION_ERROR",
    )


def RangeValidationException(
    field_name: str,
    value: Any,
    min_value: Any = None,
    max_value: Any = None,
) -> ValidationException:
    if min_value is not None and max_value is not None:
        message = f"字段 '{field_name}' 值 {value} 超出范围 [{min_value}, {max_value}]"
    elif min_value is not None:
        message = f"字段 '{field_name}' 值 {value} 小于最小值 {min_value}"
    elif max_value is not None:
        message = f"字段 '{field_name}' 值 {value} 大于最大值 {max_value}"
    else:
        message = f"字段 '{field_name}' 值 {value} 不在有效范围内"

    return FieldValidationException(
        field=field_name,
        value=value,
        message=message,
        error_code="RANGE_VALIDATION_ERROR",
    )


def PatternValidationException(field_name: str, value: str, pattern: str) -> ValidationException:
    return FieldValidationException(
        field=field_name,
        value=value,
        message=f"字段 '{field_name}' 值 '{value}' 不符合模式 '{pattern}'",
        error_code="PATTERN_VALIDATION_ERROR",
    )


def UniqueConstraintException(field_name: str, value: Any, entity_type: str = "Entity") -> ValidationException:
    return FieldValidationException(
        field=field_name,
        value=value,
        message=f"{entity_type} 中字段 '{field_name}' 值 '{value}' 已存在，必须唯一",
        error_code="UNIQUE_CONSTRAINT_ERROR",
    )
