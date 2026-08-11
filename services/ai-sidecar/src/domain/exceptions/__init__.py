"""异常模块初始化文件

使domain.exceptions成为一个Python包
"""

from .base import DomainException, DomainExceptionHandler, create_exception_handler
from .business import (
    BaggageClaimRuleException,
    BoardingRuleException,
    BusinessConstraintException,
    BusinessRuleException,
    CapacityRuleException,
    DependencyRuleException,
    DuplicateOperationException,
    FlightScheduleRuleException,
    FlightStatusRuleException,
    GateAssignmentRuleException,
    TimingRuleException,
)
from .infrastructure import (
    CacheException,
    ConfigurationException,
    ConnectionException,
    ConnectionPoolException,
    DatabaseException,
    ExternalServiceException,
    FileStorageException,
    InfrastructureException,
    MessageQueueException,
    SecurityException,
    TransactionException,
)
from .validation import (
    DateTimeValidationException,
    FlightNumberValidationException,
    PatternValidationException,
    RangeValidationException,
    RequiredFieldException,
    TypeValidationException,
    UniqueConstraintException,
    ValidationException,
    ValueObjectValidationException,
)

__all__ = [
    "BaggageClaimRuleException",
    "BoardingRuleException",
    "BusinessConstraintException",
    # Business exceptions
    "BusinessRuleException",
    "CacheException",
    "CapacityRuleException",
    "ConfigurationException",
    "ConnectionException",
    "ConnectionPoolException",
    "DatabaseException",
    "DateTimeValidationException",
    "DependencyRuleException",
    # Base exceptions
    "DomainException",
    "DomainExceptionHandler",
    "DuplicateOperationException",
    "ExternalServiceException",
    "FileStorageException",
    "FlightNumberValidationException",
    "FlightScheduleRuleException",
    "FlightStatusRuleException",
    "GateAssignmentRuleException",
    # Infrastructure exceptions
    "InfrastructureException",
    "MessageQueueException",
    "PatternValidationException",
    "RangeValidationException",
    "RequiredFieldException",
    "SecurityException",
    "TimingRuleException",
    "TransactionException",
    "TypeValidationException",
    "UniqueConstraintException",
    # Validation exceptions
    "ValidationException",
    "ValueObjectValidationException",
    "create_exception_handler",
]
