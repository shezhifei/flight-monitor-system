"""领域异常基础模块

定义领域层的基础异常类和异常体系
"""

import json
import logging
import traceback
from abc import ABC
from enum import Enum
from typing import Any, Optional

from src.domain.utils.time_utils import utc_now
from src.shared.id_generator import generate_short_id


# 统一标准错误码常量
class ErrorCodes:
    """统一标准错误码定义"""

    # 验证错误
    VALIDATION_ERROR = "VALIDATION_ERROR"
    REQUIRED_FIELD_MISSING = "REQUIRED_FIELD_MISSING"
    INVALID_FORMAT = "INVALID_FORMAT"

    # 业务逻辑错误
    BUSINESS_RULE_VIOLATION = "BUSINESS_RULE_VIOLATION"
    RESOURCE_CONFLICT = "RESOURCE_CONFLICT"
    OPERATION_NOT_ALLOWED = "OPERATION_NOT_ALLOWED"

    # 认证授权错误
    UNAUTHORIZED = "UNAUTHORIZED"
    FORBIDDEN = "FORBIDDEN"
    TOKEN_EXPIRED = "TOKEN_EXPIRED"
    INVALID_TOKEN = "INVALID_TOKEN"

    # 资源错误
    RESOURCE_NOT_FOUND = "RESOURCE_NOT_FOUND"
    RESOURCE_ALREADY_EXISTS = "RESOURCE_ALREADY_EXISTS"

    # 系统错误
    INTERNAL_ERROR = "INTERNAL_ERROR"
    DATABASE_ERROR = "DATABASE_ERROR"
    EXTERNAL_SERVICE_ERROR = "EXTERNAL_SERVICE_ERROR"

    # 速率限制
    RATE_LIMIT_EXCEEDED = "RATE_LIMIT_EXCEEDED"


class ExceptionCategory(Enum):
    """异常分类枚举"""

    BUSINESS = "business"  # 业务异常
    SYSTEM = "system"  # 系统异常
    VALIDATION = "validation"  # 验证异常
    INFRASTRUCTURE = "infrastructure"  # 基础设施异常


class ExceptionSeverity(Enum):
    """异常严重程度枚举"""

    LOW = "low"  # 低严重程度
    MEDIUM = "medium"  # 中等严重程度
    HIGH = "high"  # 高严重程度
    CRITICAL = "critical"  # 关键严重程度


class DomainException(Exception, ABC):
    """Base class for domain exceptions."""

    def __init__(
        self,
        message: str,
        error_code: str | None = None,
        category: ExceptionCategory = ExceptionCategory.BUSINESS,
        severity: ExceptionSeverity = ExceptionSeverity.MEDIUM,
        details: dict[str, Any] | None = None,
        status_code: int = 500,
        context: dict[str, Any] | None = None,
        cause: Optional["DomainException"] = None,
    ):
        super().__init__(message)
        self.message = message
        self.error_code = error_code or self._generate_default_error_code()
        self.category = category
        self.severity = severity
        self.details = details or {}
        self.status_code = status_code
        self.context = context or {}
        self.timestamp = utc_now()
        self.traceback_info = self._capture_traceback_info()
        self.cause = cause
        self.exception_id = self._generate_exception_id()

        self._log_exception()

    @staticmethod
    def _capture_traceback_info() -> str | None:
        if not logging.getLogger(__name__).isEnabledFor(logging.DEBUG):
            return None
        tb = traceback.format_exc()
        if not tb or tb.strip() == "NoneType: None":
            return None
        return tb

    def _generate_default_error_code(self) -> str:
        """生成默认错误代码"""
        class_name = self.__class__.__name__
        if class_name.endswith("Exception"):
            class_name = class_name[:-9]
        return f"{class_name.upper()}_ERROR"

    def _generate_exception_id(self) -> str:
        """生成异常唯一标识"""
        return generate_short_id(8)

    def _log_exception(self):
        """记录异常日志"""
        logger = logging.getLogger(__name__)
        debug_enabled = logger.isEnabledFor(logging.DEBUG)

        log_data = {
            "exception_id": self.exception_id,
            "error_code": self.error_code,
            "category": self.category.value,
            "severity": self.severity.value,
            "message": self.message,
            "details": self.details,
            "context": self.context,
        }

        if self.cause:
            log_data["cause"] = {
                "exception_id": self.cause.exception_id,
                "error_code": self.cause.error_code,
                "message": self.cause.message,
            }

        if debug_enabled:
            log_message = json.dumps(log_data, ensure_ascii=False)
        else:
            log_message = (
                f"id={self.exception_id} code={self.error_code} "
                f"category={self.category.value} severity={self.severity.value} "
                f"message={self.message}"
            )

        if self.severity == ExceptionSeverity.CRITICAL:
            logger.critical(f"Critical exception: {log_message}")
        elif self.severity == ExceptionSeverity.HIGH:
            logger.error(f"High severity exception: {log_message}")
        elif self.severity == ExceptionSeverity.MEDIUM:
            logger.warning(f"Medium severity exception: {log_message}")
        else:
            logger.info(f"Low severity exception: {log_message}")

    def to_dict(self) -> dict[str, Any]:
        """转换为字典格式，便于序列化"""
        result = {
            "code": self.error_code,
            "message": self.message,
        }

        if self.details:
            result["details"] = self.details

        if isinstance(self, ValidationException) and "field" in self.details:
            field = self.details.get("field")
            if field:
                result["field_errors"] = {field: [self.message]}

        if logging.getLogger(__name__).isEnabledFor(logging.DEBUG):
            result.update(
                {
                    "exception_id": self.exception_id,
                    "category": self.category.value,
                    "severity": self.severity.value,
                    "context": self.context,
                    "status_code": self.status_code,
                    "type": self.__class__.__name__,
                }
            )

            if self.cause:
                result["cause"] = self.cause.to_dict()

        return result

    def with_cause(self, cause: "DomainException") -> "DomainException":
        """设置异常原因，用于链式异常处理"""
        self.cause = cause
        return self

    def with_context(self, **kwargs) -> "DomainException":
        """添加上下文信息"""
        self.context.update(kwargs)
        return self

    def with_details(self, **kwargs) -> "DomainException":
        """添加详细信息"""
        self.details.update(kwargs)
        return self

    def with_severity(self, severity: ExceptionSeverity) -> "DomainException":
        """设置严重程度"""
        self.severity = severity
        return self

    def get_root_cause(self) -> "DomainException":
        """获取根异常"""
        current = self
        while current.cause:
            current = current.cause
        return current

    def get_exception_chain(self) -> list["DomainException"]:
        """获取异常链"""
        chain = []
        current = self
        while current:
            chain.append(current)
            current = current.cause
        return chain

    def is_recoverable(self) -> bool:
        """判断异常是否可恢复"""
        return self.category in [ExceptionCategory.BUSINESS, ExceptionCategory.VALIDATION]

    def requires_immediate_attention(self) -> bool:
        """判断是否需要立即处理"""
        return self.severity in [ExceptionSeverity.HIGH, ExceptionSeverity.CRITICAL]

    def __str__(self):
        return f"[{self.error_code}] {self.message}"

    def __repr__(self):
        return f"{self.__class__.__name__}(error_code='{self.error_code}', message='{self.message}', category='{self.category.value}')"


class RepositoryException(DomainException):
    """仓库异常基类"""

    def __init__(
        self,
        message: str,
        error_code: str = ErrorCodes.DATABASE_ERROR,
        severity: ExceptionSeverity = ExceptionSeverity.HIGH,
    ):
        super().__init__(
            message=message,
            error_code=error_code,
            category=ExceptionCategory.INFRASTRUCTURE,
            severity=severity,
            status_code=500,
        )


class ValidationException(DomainException):
    """验证异常基类"""

    def __init__(
        self,
        message: str,
        field: str | None = None,
        value: Any | None = None,
        error_code: str = ErrorCodes.VALIDATION_ERROR,
        severity: ExceptionSeverity = ExceptionSeverity.MEDIUM,
    ):
        details = {"field": field, "value": value}
        super().__init__(
            message=message,
            error_code=error_code,
            category=ExceptionCategory.VALIDATION,
            severity=severity,
            details=details,
            status_code=422,  # 验证错误
        )


class BusinessRuleException(DomainException):
    """业务规则异常基类"""

    def __init__(
        self,
        message: str,
        rule_name: str | None = None,
        error_code: str = ErrorCodes.BUSINESS_RULE_VIOLATION,
        severity: ExceptionSeverity = ExceptionSeverity.MEDIUM,
    ):
        details = {"rule_name": rule_name}
        super().__init__(
            message=message,
            error_code=error_code,
            category=ExceptionCategory.BUSINESS,
            severity=severity,
            details=details,
            status_code=422,  # 业务规则错误也属于验证错误
        )


class SystemException(DomainException):
    """系统异常基类"""

    def __init__(
        self,
        message: str,
        component: str | None = None,
        error_code: str = ErrorCodes.INTERNAL_ERROR,
        severity: ExceptionSeverity = ExceptionSeverity.HIGH,
    ):
        details = {"component": component}
        super().__init__(
            message=message,
            error_code=error_code,
            category=ExceptionCategory.SYSTEM,
            severity=severity,
            details=details,
            status_code=500,  # 系统错误
        )


class EntityNotFoundException(DomainException):
    """实体未找到异常"""

    def __init__(
        self,
        entity_type: str,
        entity_id: str | None = None,
        error_code: str = ErrorCodes.RESOURCE_NOT_FOUND,
        severity: ExceptionSeverity = ExceptionSeverity.MEDIUM,
    ):
        message = f"{entity_type} 实体未找到"
        if entity_id:
            message += f": {entity_id}"

        details = {"entity_type": entity_type, "entity_id": entity_id}
        super().__init__(
            message=message,
            error_code=error_code,
            category=ExceptionCategory.BUSINESS,
            severity=severity,
            details=details,
            status_code=404,  # 资源未找到
        )


class ValueObjectValidationException(ValidationException):
    """值对象验证异常"""

    def __init__(
        self,
        value_object_name: str,
        message: str,
        field: str | None = None,
        severity: ExceptionSeverity = ExceptionSeverity.MEDIUM,
    ):
        full_message = f"{value_object_name} 验证失败: {message}"
        super().__init__(
            message=full_message, field=field, error_code="VALUE_OBJECT_VALIDATION_ERROR", severity=severity
        )


class AggregateRootException(DomainException):
    """聚合根异常"""

    def __init__(
        self, message: str, aggregate_name: str | None = None, severity: ExceptionSeverity = ExceptionSeverity.HIGH
    ):
        details = {"aggregate_name": aggregate_name}
        super().__init__(
            message=message,
            error_code="AGGREGATE_ROOT_ERROR",
            category=ExceptionCategory.BUSINESS,
            severity=severity,
            details=details,
        )


class DomainServiceException(DomainException):
    """领域服务异常"""

    def __init__(
        self, message: str, service_name: str | None = None, severity: ExceptionSeverity = ExceptionSeverity.HIGH
    ):
        details = {"service_name": service_name}
        super().__init__(
            message=message,
            error_code="DOMAIN_SERVICE_ERROR",
            category=ExceptionCategory.SYSTEM,
            severity=severity,
            details=details,
        )


class DomainExceptionHandler:
    """统一的领域异常处理器

    提供异常转换、上下文管理和响应标准化功能
    """

    def __init__(self):
        self.logger = logging.getLogger(__name__)
        self._handlers = {}  # 异常类型处理器映射
        self._category_handlers = {}  # 异常分类处理器映射
        self._severity_handlers = {}  # 异常严重程度处理器映射

    def register_handler(self, exception_type: type, handler_func):
        """注册异常类型处理函数"""
        self._handlers[exception_type] = handler_func

    def register_category_handler(self, category: ExceptionCategory, handler_func):
        """注册异常分类处理函数"""
        self._category_handlers[category] = handler_func

    def register_severity_handler(self, severity: ExceptionSeverity, handler_func):
        """注册异常严重程度处理函数"""
        self._severity_handlers[severity] = handler_func

    def handle_exception(self, exc: Exception, context: str = "", **kwargs) -> dict[str, Any]:
        """处理异常并返回标准化响应"""
        try:
            # 1. 尝试使用特定异常类型处理器
            for exc_type, handler in self._handlers.items():
                if isinstance(exc, exc_type):
                    return handler(exc, context, **kwargs)

            # 2. 如果是领域异常，尝试使用分类处理器
            if isinstance(exc, DomainException):
                category_handler = self._category_handlers.get(exc.category)
                if category_handler:
                    return category_handler(exc, context, **kwargs)

                # 3. 尝试使用严重程度处理器
                severity_handler = self._severity_handlers.get(exc.severity)
                if severity_handler:
                    return severity_handler(exc, context, **kwargs)

            # 4. 使用默认处理
            return self._handle_default_exception(exc, context, **kwargs)

        except Exception as handling_error:  # noqa: BLE001 - exception handler must never raise
            # 异常处理本身出错，记录并返回安全响应
            self.logger.critical(f"Exception handler failed: {handling_error}")
            return self._create_safe_response(exc, handling_error)

    def _handle_default_exception(self, exc: Exception, context: str, **kwargs) -> dict[str, Any]:
        """处理默认异常"""
        self.logger.error(f"未处理异常 [{context}]: {exc!s}")
        if self.logger.isEnabledFor(logging.DEBUG):
            self.logger.debug("异常堆栈: %s", traceback.format_exc())

        # 对于领域异常，返回详细信息；对于其他异常，只返回通用信息
        if isinstance(exc, DomainException):
            exc_dict = exc.to_dict()
            error_data = {
                "code": exc_dict.get("code", exc.error_code),
                "message": exc_dict.get("message", exc.message),
            }

            # 添加details如果存在
            if "details" in exc_dict:
                error_data["details"] = exc_dict["details"]

            # 添加field_errors如果存在
            if "field_errors" in exc_dict:
                error_data["field_errors"] = exc_dict["field_errors"]

            return {"success": False, "error": error_data}
        else:
            # 非领域异常，返回通用错误信息，不暴露内部细节
            return {
                "success": False,
                "error": {
                    "code": ErrorCodes.INTERNAL_ERROR,
                    "message": "系统内部错误",
                },
            }

    def _create_safe_response(self, original_exc: Exception, handling_error: Exception) -> dict[str, Any]:
        """创建安全的错误响应（当异常处理失败时）"""
        return {
            "success": False,
            "error": {
                "code": "EXCEPTION_HANDLER_ERROR",
                "category": "system",
                "severity": "critical",
                "message": "系统错误处理失败",
                "type": "handler_error",
                "timestamp": utc_now().isoformat(),
                "recoverable": False,
                "requires_immediate_attention": True,
                "handler_error": str(handling_error)
                if logging.getLogger(__name__).isEnabledFor(logging.DEBUG)
                else None,
            },
        }

    def convert_to_domain_exception(self, exc: Exception, context: str = "") -> DomainException:
        """将普通异常转换为领域异常"""
        if isinstance(exc, DomainException):
            return exc

        # 根据异常类型进行转换
        if isinstance(exc, ValueError):
            return ValidationException(
                message=f"参数验证错误: {exc!s}", severity=ExceptionSeverity.MEDIUM
            ).with_context(original_exception=type(exc).__name__, context=context)

        elif isinstance(exc, (KeyError, IndexError)):
            return EntityNotFoundException(
                entity_type="资源", entity_id=str(exc), severity=ExceptionSeverity.MEDIUM
            ).with_context(original_exception=type(exc).__name__, context=context)

        elif isinstance(exc, (ConnectionError, TimeoutError)):
            return SystemException(
                message=f"连接错误: {exc!s}", component="network", severity=ExceptionSeverity.HIGH
            ).with_context(original_exception=type(exc).__name__, context=context)

        elif isinstance(exc, PermissionError):
            return SystemException(
                message=f"权限错误: {exc!s}", component="security", severity=ExceptionSeverity.HIGH
            ).with_context(original_exception=type(exc).__name__, context=context)

        else:
            # 其他未知异常
            return SystemException(
                message=f"未知系统错误: {exc!s}", component="unknown", severity=ExceptionSeverity.CRITICAL
            ).with_context(original_exception=type(exc).__name__, context=context)

    def get_exception_statistics(self) -> dict[str, Any]:
        """获取异常统计信息"""
        return {
            "registered_type_handlers": len(self._handlers),
            "registered_category_handlers": len(self._category_handlers),
            "registered_severity_handlers": len(self._severity_handlers),
            "handler_types": list(self._handlers.keys()),
            "category_handlers": [cat.value for cat in self._category_handlers],
            "severity_handlers": [sev.value for sev in self._severity_handlers],
        }


def create_exception_handler() -> "DomainExceptionHandler":
    """Factory function to create a DomainExceptionHandler instance.

    Prefer dependency injection over module-level singletons per North Star §3.1.
    """
    return DomainExceptionHandler()
