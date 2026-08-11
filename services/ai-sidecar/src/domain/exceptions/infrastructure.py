"""Infrastructure-layer domain exceptions."""


class InfrastructureException(Exception):
    def __init__(self, message: str = "", *, code: str = "INFRASTRUCTURE_ERROR"):
        self.code = code
        super().__init__(message)


class DatabaseException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="DATABASE_ERROR")


class ConnectionException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="CONNECTION_ERROR")


class ConnectionPoolException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="CONNECTION_POOL_ERROR")


class CacheException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="CACHE_ERROR")


class TransactionException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="TRANSACTION_ERROR")


class MessageQueueException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="MESSAGE_QUEUE_ERROR")


class ExternalServiceException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="EXTERNAL_SERVICE_ERROR")


class ConfigurationException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="CONFIGURATION_ERROR")


class FileStorageException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="FILE_STORAGE_ERROR")


class SecurityException(InfrastructureException):
    def __init__(self, message: str = ""):
        super().__init__(message, code="SECURITY_ERROR")
