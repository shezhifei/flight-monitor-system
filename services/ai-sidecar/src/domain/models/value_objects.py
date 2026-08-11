"""值对象模块

定义领域中的值对象，这些对象没有唯一标识，其相等性基于属性值
使用统一的验证规则库确保Python和Rust版本验证逻辑一致
"""

import re
from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from typing import Any, ClassVar, Generic, TypeVar

from src.domain.exceptions import base
from src.domain.validation.unified_validators import (
    AircraftTypeValidator,
    AirportCodeValidator,
    EmailAddressValidator,
    FlightIdValidator,
    FlightNumberValidator,
    GateNumberValidator,
    MissionTypeValidator,
    PasswordHashValidator,
    ProcessInstanceIdValidator,
    StandNumberValidator,
    UserIdValidator,
)
from src.shared.id_generator import generate_id

T = TypeVar("T")


class FlightType(Enum):
    """航班类型枚举 (使用数字存储)

    用于单个航段的 `flight_type` 字段。
    """

    DOMESTIC = (0, "国内")
    INTL = (1, "国际")
    REGION = (2, "地区")

    def __init__(self, code: int, label: str):
        self.code = code
        self.label = label

    @classmethod
    def _get_code_map(cls):
        if not hasattr(cls, "_code_map_cache"):
            cls._code_map_cache = {item.code: item for item in cls}
        return cls._code_map_cache

    @classmethod
    def _get_label_map(cls):
        if not hasattr(cls, "_label_map_cache"):
            cls._label_map_cache = {item.label: item for item in cls}
        return cls._label_map_cache

    @classmethod
    def _get_str_map(cls):
        if not hasattr(cls, "_str_map_cache"):
            cls._str_map_cache = {
                **cls._get_label_map(),
                "domestic": cls.DOMESTIC,
                "0": cls.DOMESTIC,
                "intl": cls.INTL,
                "international": cls.INTL,
                "1": cls.INTL,
                "region": cls.REGION,
                "2": cls.REGION,
            }
        return cls._str_map_cache

    @classmethod
    def from_code(cls, code: int) -> "FlightType":
        if (ft := cls._get_code_map().get(code)) is not None:
            return ft
        raise ValueError(f"无效的航班类型代码: {code}")

    @classmethod
    def from_label(cls, label: str) -> "FlightType":
        if (ft := cls._get_label_map().get(label)) is not None:
            return ft
        raise ValueError(f"无效的航班类型标签: {label}")

    @classmethod
    def from_str(cls, value: str) -> "FlightType":
        if (ft := cls._get_str_map().get(value)) is not None:
            return ft
        raise ValueError(f"无效的航班类型字符串: {value}")

    @classmethod
    def from_any(cls, value) -> "FlightType | None":
        if value is None:
            return None
        if isinstance(value, cls):
            return value
        if isinstance(value, int):
            return cls._get_code_map().get(value)
        if isinstance(value, str):
            return cls._get_str_map().get(value)
        return None

    def __str__(self):
        return self.label


class FlightStatus(Enum):
    """航班状态枚举 (使用数字存储)"""

    SCHEDULED = (0, "计划中")
    PREV_DEPARTED = (1, "前站起飞")
    ARRIVED = (2, "到达本站")
    CHECK_IN_END = (3, "值机结束")
    BOARDING = (4, "登机")
    BOARDING_URGE = (5, "催促登机")
    BOARDING_END = (6, "结束登机")
    DEPARTED = (7, "已起飞")
    NEXT_ARRIVED = (8, "到下站")
    CANCELLED = (9, "取消")
    DELAYED = (10, "延误")

    def __init__(self, code: int, label: str):
        self.code = code
        self.label = label

    @classmethod
    def _get_code_map(cls):
        if not hasattr(cls, "_code_map_cache"):
            cls._code_map_cache = {item.code: item for item in cls}
        return cls._code_map_cache

    @classmethod
    def _get_label_map(cls):
        if not hasattr(cls, "_label_map_cache"):
            cls._label_map_cache = {item.label: item for item in cls}
        return cls._label_map_cache

    @classmethod
    def _get_name_map(cls):
        if not hasattr(cls, "_name_map_cache"):
            cls._name_map_cache = {item.name: item for item in cls}
        return cls._name_map_cache

    @classmethod
    def from_code(cls, code: int) -> "FlightStatus":
        if (fs := cls._get_code_map().get(code)) is not None:
            return fs
        raise ValueError(f"无效的航班状态代码: {code}")

    @classmethod
    def from_label(cls, label: str) -> "FlightStatus":
        if (fs := cls._get_label_map().get(label)) is not None:
            return fs
        raise ValueError(f"无效的航班状态标签: {label}")

    @classmethod
    def from_name(cls, name: str) -> "FlightStatus":
        if (fs := cls._get_name_map().get(name)) is not None:
            return fs
        raise ValueError(f"无效的航班状态名称: {name}")

    @classmethod
    def from_any(cls, value) -> "FlightStatus | None":
        if value is None:
            return None
        if isinstance(value, cls):
            return value
        if isinstance(value, int):
            return cls._get_code_map().get(value)
        if isinstance(value, str):
            normalized = value.strip()
            if not normalized:
                return None

            by_label_or_name = cls._get_label_map().get(normalized) or cls._get_name_map().get(normalized)
            if by_label_or_name is not None:
                return by_label_or_name

            by_upper_name = cls._get_name_map().get(normalized.upper())
            if by_upper_name is not None:
                return by_upper_name

            if normalized.isdigit():
                return cls._get_code_map().get(int(normalized))
        return None

    def __str__(self):
        return self.label


class BaseValueObject(ABC, Generic[T]):
    """值对象基类，提供通用验证逻辑"""

    @abstractmethod
    def _validate(self, value: T) -> None:
        """验证值的方法，子类必须实现"""
        pass

    @abstractmethod
    def _get_value_type(self) -> type:
        """获取值的类型，用于类型检查"""
        pass

    def __post_init__(self):
        """验证值对象的值"""
        value_type = self._get_value_type()
        if not isinstance(self.value, value_type):
            raise base.ValueObjectValidationException(
                self.__class__.__name__,
                f"值类型错误，期望 {value_type.__name__}，实际 {type(self.value).__name__}",
                "value",
            )
        self._validate(self.value)


class StringValueObject(BaseValueObject[str]):
    """字符串值对象基类。"""

    VALIDATOR: ClassVar[Any] = None
    OBJECT_NAME: ClassVar[str | None] = None

    def _get_value_type(self) -> type:
        return str

    def _validate(self, value: str) -> None:
        validator = self._get_validator()
        if validator is None:
            raise base.ValueObjectValidationException(self._get_object_name(), "未配置验证器", "value")
        self._raise_if_invalid(validator.validate(value), self._get_object_name())

    @classmethod
    def _get_validator(cls) -> Any:
        return cls.VALIDATOR

    @classmethod
    def _get_object_name(cls) -> str:
        return cls.OBJECT_NAME or cls.__name__

    @staticmethod
    def _raise_if_invalid(result, object_name: str) -> None:
        if not result.is_valid:
            raise base.ValueObjectValidationException(object_name, result.error_message or "验证失败", "value")


@dataclass(frozen=True)
class FlightId(StringValueObject):
    """航班ID值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = FlightIdValidator
    OBJECT_NAME: ClassVar[str] = "FlightId"

    @classmethod
    def generate(cls) -> "FlightId":
        """生成新的航班ID"""
        return cls(value=generate_id())


@dataclass(frozen=True)
class FlightNumber(StringValueObject):
    """航班号值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = FlightNumberValidator
    OBJECT_NAME: ClassVar[str] = "FlightNumber"

    def get_airline_code(self) -> str:
        match = re.match(r"^([A-Z]{2,3})", self.value)
        return match.group(1) if match else ""


@dataclass(frozen=True)
class AirportCode(StringValueObject):
    """机场代码值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = AirportCodeValidator
    OBJECT_NAME: ClassVar[str] = "AirportCode"


@dataclass(frozen=True)
class AircraftType(StringValueObject):
    """机型值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = AircraftTypeValidator
    OBJECT_NAME: ClassVar[str] = "AircraftType"


@dataclass(frozen=True)
class StandNumber(StringValueObject):
    """机位号值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = StandNumberValidator
    OBJECT_NAME: ClassVar[str] = "StandNumber"


@dataclass(frozen=True)
class GateNumber(StringValueObject):
    """登机口号值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = GateNumberValidator
    OBJECT_NAME: ClassVar[str] = "GateNumber"


@dataclass(frozen=True)
class MissionType(StringValueObject):
    """任务类型值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = MissionTypeValidator
    OBJECT_NAME: ClassVar[str] = "MissionType"

    def _validate(self, value: str) -> None:
        """验证任务类型值"""
        super()._validate(value)

        # 额外验证：检查是否为有效的任务性质代码
        try:
            from .mission_type_enum import MissionTypeEnum

            valid_codes = MissionTypeEnum.get_all_codes().keys()
            if value not in valid_codes:
                raise base.ValueObjectValidationException(
                    "MissionType", f"任务类型代码无效，支持的代码: {', '.join(valid_codes)}: {value}", "value"
                )
        except ImportError:
            # 如果枚举模块不可用，统一验证器已处理基本格式验证
            pass

    def to_numeric_value(self) -> int:
        """将任务类型转换为数字值存储到数据库"""
        try:
            from .mission_type_enum import MissionTypeEnum

            mission_enum = MissionTypeEnum.from_code(self.value)
            if mission_enum:
                return mission_enum.numeric_value
            # 如果找不到对应的枚举，抛出异常
            raise base.ValueObjectValidationException(
                "MissionType", f"无法找到对应的任务类型代码: {self.value}", "value"
            )
        except ImportError as exc:
            raise base.ValueObjectValidationException(
                "MissionType", f"任务类型枚举模块不可用，无法转换: {self.value}", "value"
            ) from exc

    @classmethod
    def from_numeric_value(cls, numeric_value: int) -> "MissionType":
        """从数据库的数字值创建MissionType对象"""
        try:
            from .mission_type_enum import MissionTypeEnum

            mission_enum = MissionTypeEnum.from_numeric_value(numeric_value)
            if mission_enum:
                return cls(value=mission_enum.code)
            # 如果找不到对应的枚举，抛出异常
            raise base.ValueObjectValidationException("MissionType", f"无法找到对应的数字值: {numeric_value}", "value")
        except ImportError as exc:
            raise base.ValueObjectValidationException(
                "MissionType", f"任务类型枚举模块不可用，无法从数字值创建: {numeric_value}", "value"
            ) from exc


@dataclass(frozen=True)
class ProcessInstanceId(StringValueObject):
    """流程实例ID值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = ProcessInstanceIdValidator
    OBJECT_NAME: ClassVar[str] = "ProcessInstanceId"


@dataclass(frozen=True)
class UserID(StringValueObject):
    """用户ID值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = UserIdValidator
    OBJECT_NAME: ClassVar[str] = "UserID"

    @classmethod
    def generate(cls) -> "UserID":
        """生成新的用户ID"""
        return cls(value=generate_id())


@dataclass(frozen=True)
class EmailAddress(StringValueObject):
    """电子邮件地址值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = EmailAddressValidator
    OBJECT_NAME: ClassVar[str] = "EmailAddress"


@dataclass(frozen=True)
class PasswordHash(StringValueObject):
    """密码哈希值对象"""

    value: str
    VALIDATOR: ClassVar[Any] = PasswordHashValidator
    OBJECT_NAME: ClassVar[str] = "PasswordHash"
