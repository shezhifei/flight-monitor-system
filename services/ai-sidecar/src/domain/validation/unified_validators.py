"""
统一验证规则库

定义跨语言的统一值对象验证规则，确保Python和Rust版本在验证逻辑上完全一致。
"""

import re
from dataclasses import dataclass
from datetime import datetime
from enum import Enum
from typing import Any, ClassVar

from src.domain.exceptions.validation import ValidationException as DomainValidationException


class ValidationErrorCode(Enum):
    """标准验证错误代码"""

    REQUIRED_FIELD = "REQUIRED_FIELD"
    INVALID_FORMAT = "INVALID_FORMAT"
    TOO_SHORT = "TOO_SHORT"
    TOO_LONG = "TOO_LONG"
    INVALID_LENGTH = "INVALID_LENGTH"
    INVALID_ULID_FORMAT = "INVALID_ULID_FORMAT"
    INVALID_FLIGHT_NUMBER = "INVALID_FLIGHT_NUMBER"
    INVALID_AIRPORT_CODE = "INVALID_AIRPORT_CODE"
    FORBIDDEN_SUFFIX = "FORBIDDEN_SUFFIX"
    BUSINESS_RULE_VIOLATION = "BUSINESS_RULE_VIOLATION"
    INVALID_STATE_TRANSITION = "INVALID_STATE_TRANSITION"
    DUPLICATE_VALUE = "DUPLICATE_VALUE"
    VALIDATION_SYSTEM_ERROR = "VALIDATION_SYSTEM_ERROR"


@dataclass(frozen=True)
class ValidationResult:
    """统一的验证结果结构"""

    is_valid: bool
    error_code: str | None = None
    error_message: str | None = None
    error_details: dict[str, Any] | None = None

    @classmethod
    def success(cls) -> "ValidationResult":
        return cls(is_valid=True)

    @classmethod
    def error(cls, code: str, message: str, details: dict[str, Any] | None = None) -> "ValidationResult":
        return cls(is_valid=False, error_code=code, error_message=message, error_details=details)

    def __str__(self) -> str:
        if self.is_valid:
            return "验证通过"
        return f"验证失败: {self.error_code} - {self.error_message}"


# 标准验证规则定义
ULID_RULES = {
    "format": r"^[0-9A-HJKMNP-TV-Z]{26}$",
    "exact_length": 26,
    "description": "必须为有效的ULID格式（26字符Crockford Base32）",
}

FLIGHT_NUMBER_RULES = {
    "format": r"^[A-Z0-9]{2,3}\d{1,4}[A-Z0-9]?$",
    "min_length": 3,
    "max_length": 8,
    "description": "航班号格式: 2-3位航空公司代码(字母数字) + 1-4数字航班号 + 可选后缀(字母/数字)",
}

AIRPORT_CODE_RULES = {
    "format": r"^[A-Z]{3}$",
    "exact_length": 3,
    "reserved_codes": {"XXX", "ZZZ", "AAA", "BBB"},
    "description": "机场代码必须为3字母IATA格式",
}

AIRCRAFT_TYPE_RULES = {
    "format": r"^[A-Z0-9\-]+$",
    "min_length": 2,
    "max_length": 10,
    "no_start_end_hyphen": True,
    "no_consecutive_hyphens": True,
    "description": "机型格式: 大写字母、数字和连字符",
}

STAND_NUMBER_RULES = {
    "format": r"^[A-Z]?\d{1,3}[A-Z]?$",
    "min_length": 1,
    "max_length": 5,
    "no_all_zeros": True,
    "no_same_adjacent_letters": True,
    "description": "机位号格式: 可选字母 + 1-3数字 + 可选字母",
}

GATE_NUMBER_RULES = {
    "format": r"^[A-Z]?\d{1,3}[A-Z]?$",
    "min_length": 1,
    "max_length": 5,
    "no_zero_digits": True,
    "no_same_adjacent_letters": True,
    "description": "登机口号格式: 可选字母 + 1-3数字 + 可选字母",
}

MISSION_TYPE_RULES = {
    "format": r"^[A-Z_]+$",
    "min_length": 1,
    "max_length": 10,
    "no_start_end_underscore": True,
    "no_consecutive_underscores": True,
    "description": "任务类型格式: 大写字母和下划线",
}


PROCESS_INSTANCE_ID_RULES = ULID_RULES.copy()  # ProcessInstanceId使用与FlightId相同的规则

USER_ID_RULES = ULID_RULES.copy()  # UserId使用与FlightId相同的规则

EMAIL_ADDRESS_RULES = {
    # 移除严格的正则表达式，只保留基本的字符检查
    "min_length": 1,
    "max_length": 254,
    "local_part_max_length": 64,
    "domain_max_length": 253,
    "description": "邮箱格式: 任意非空字符串",
}

PASSWORD_HASH_RULES = {
    "min_length": 60,
    "max_length": 128,
    "bcrypt_versions": ["2a", "2b", "2x", "2y"],
    "bcrypt_cost_min": 4,
    "bcrypt_cost_max": 31,
    "description": "密码哈希格式: bcrypt格式($2a$10$...)",
}


class UnifiedValidator:
    """统一验证器基类"""

    @staticmethod
    def validate_required_field(value: str, field_name: str) -> ValidationResult:
        """验证必填字段"""
        if not value or not value.strip():
            return ValidationResult.error(
                ValidationErrorCode.REQUIRED_FIELD.value,
                f"{field_name}不能为空或仅包含空白字符",
                {"field": field_name, "value": value},
            )
        return ValidationResult.success()

    @staticmethod
    def validate_length(value: str, rules: dict[str, Any], field_name: str) -> ValidationResult:
        """验证长度"""
        length = len(value)

        if "exact_length" in rules and length != rules["exact_length"]:
            return ValidationResult.error(
                ValidationErrorCode.INVALID_LENGTH.value,
                f"{field_name}长度必须为{rules['exact_length']}字符，当前长度: {length}",
                {
                    "field": field_name,
                    "value": value,
                    "required_length": rules["exact_length"],
                    "actual_length": length,
                },
            )

        if "min_length" in rules and length < rules["min_length"]:
            return ValidationResult.error(
                ValidationErrorCode.TOO_SHORT.value,
                f"{field_name}长度至少{rules['min_length']}字符，当前长度: {length}",
                {"field": field_name, "value": value, "min_length": rules["min_length"], "actual_length": length},
            )

        if "max_length" in rules and length > rules["max_length"]:
            return ValidationResult.error(
                ValidationErrorCode.TOO_LONG.value,
                f"{field_name}长度最多{rules['max_length']}字符，当前长度: {length}",
                {"field": field_name, "value": value, "max_length": rules["max_length"], "actual_length": length},
            )

        return ValidationResult.success()

    @staticmethod
    def validate_format(value: str, rules: dict[str, Any], field_name: str) -> ValidationResult:
        """验证格式"""
        if "format" in rules and not re.match(rules["format"], value):
            return ValidationResult.error(
                ValidationErrorCode.INVALID_FORMAT.value,
                f"{field_name}格式错误，{rules['description']}",
                {"field": field_name, "value": value, "pattern": rules["format"]},
            )
        return ValidationResult.success()

    @staticmethod
    def validate_business_rules(value: str, rules: dict[str, Any], field_name: str) -> ValidationResult:
        """验证业务规则"""
        # 检查保留/无效代码
        if "reserved_codes" in rules and value in rules["reserved_codes"]:
            return ValidationResult.error(
                ValidationErrorCode.BUSINESS_RULE_VIOLATION.value,
                f"{field_name}为保留或无效代码",
                {"field": field_name, "value": value, "reserved_codes": list(rules["reserved_codes"])},
            )

        # 检查禁止的后缀
        if "forbidden_suffixes" in rules:
            for suffix in rules["forbidden_suffixes"]:
                if value.endswith(suffix):
                    return ValidationResult.error(
                        ValidationErrorCode.FORBIDDEN_SUFFIX.value,
                        f"{field_name}不能以{suffix}结尾",
                        {"field": field_name, "value": value, "forbidden_suffix": suffix},
                    )

        # 检查特殊规则
        if rules.get("no_start_end_hyphen") and (value.startswith("-") or value.endswith("-")):
            return ValidationResult.error(
                ValidationErrorCode.INVALID_FORMAT.value,
                f"{field_name}不能以连字符开头或结尾",
                {"field": field_name, "value": value},
            )

        if rules.get("no_consecutive_hyphens") and "--" in value:
            return ValidationResult.error(
                ValidationErrorCode.INVALID_FORMAT.value,
                f"{field_name}不能包含连续的连字符",
                {"field": field_name, "value": value},
            )

        if rules.get("no_start_end_underscore") and (value.startswith("_") or value.endswith("_")):
            return ValidationResult.error(
                ValidationErrorCode.INVALID_FORMAT.value,
                f"{field_name}不能以下划线开头或结尾",
                {"field": field_name, "value": value},
            )

        if rules.get("no_consecutive_underscores") and "__" in value:
            return ValidationResult.error(
                ValidationErrorCode.INVALID_FORMAT.value,
                f"{field_name}不能包含连续的下划线",
                {"field": field_name, "value": value},
            )

        if rules.get("no_all_zeros"):
            digits = re.search(r"\d+", value)
            if digits and set(digits.group()) == {"0"}:
                return ValidationResult.error(
                    ValidationErrorCode.BUSINESS_RULE_VIOLATION.value,
                    f"{field_name}数字部分不能全为0",
                    {"field": field_name, "value": value},
                )

        if rules.get("no_zero_digits"):
            digits = re.search(r"\d+", value)
            if digits and (digits.group() == "0" or digits.group() == "00"):
                return ValidationResult.error(
                    ValidationErrorCode.BUSINESS_RULE_VIOLATION.value,
                    f"{field_name}数字部分不能为0",
                    {"field": field_name, "value": value},
                )

        if rules.get("no_same_adjacent_letters"):
            letters = re.findall(r"[A-Z]", value)
            if len(letters) == 2 and letters[0] == letters[1]:
                return ValidationResult.error(
                    ValidationErrorCode.BUSINESS_RULE_VIOLATION.value,
                    f"{field_name}前后字母不能相同",
                    {"field": field_name, "value": value},
                )

        if rules.get("no_start_end_special") and (
            value.startswith(".") or value.endswith(".") or value.startswith("-") or value.endswith("-")
        ):
            return ValidationResult.error(
                ValidationErrorCode.INVALID_FORMAT.value,
                f"{field_name}不能以点号或连字符开头或结尾",
                {"field": field_name, "value": value},
            )

        if rules.get("no_consecutive_dots") and ".." in value:
            return ValidationResult.error(
                ValidationErrorCode.INVALID_FORMAT.value,
                f"{field_name}域名不能包含连续的点号",
                {"field": field_name, "value": value},
            )

        return ValidationResult.success()

    @staticmethod
    def validate_with_rules(
        value: str,
        field_name: str,
        rules: dict[str, Any],
        include_business_rules: bool = True,
    ) -> ValidationResult:
        """按统一流程验证字段。"""
        result = UnifiedValidator.validate_required_field(value, field_name)
        if not result.is_valid:
            return result

        result = UnifiedValidator.validate_length(value, rules, field_name)
        if not result.is_valid:
            return result

        result = UnifiedValidator.validate_format(value, rules, field_name)
        if not result.is_valid:
            return result

        if include_business_rules:
            result = UnifiedValidator.validate_business_rules(value, rules, field_name)
            if not result.is_valid:
                return result

        return ValidationResult.success()


class FlightIdValidator:
    """FlightId验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        return UnifiedValidator.validate_with_rules(
            value,
            "航班ID",
            ULID_RULES,
            include_business_rules=False,
        )


class FlightNumberValidator:
    """FlightNumber验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        return UnifiedValidator.validate_with_rules(value, "航班号", FLIGHT_NUMBER_RULES)

    @staticmethod
    def extract_airline_code(flight_number: str) -> str | None:
        """提取航空公司代码"""
        match = re.match(r"^([A-Z]{2,3})", flight_number)
        return match.group(1) if match else None

    @staticmethod
    def extract_flight_code(flight_number: str) -> str | None:
        """提取航班号数字部分"""
        match = re.match(r"^[A-Z]{2,3}(\d{1,4})", flight_number)
        return match.group(1) if match else None


class AirportCodeValidator:
    """AirportCode验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        return UnifiedValidator.validate_with_rules(value, "机场代码", AIRPORT_CODE_RULES)

    @staticmethod
    def is_valid_iata_code(value: str) -> bool:
        """检查是否为有效的IATA代码（可以扩展为数据库验证）"""
        return bool(re.match(AIRPORT_CODE_RULES["format"], value))


class AircraftTypeValidator:
    """AircraftType验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        return UnifiedValidator.validate_with_rules(value, "机型", AIRCRAFT_TYPE_RULES)


class StandNumberValidator:
    """StandNumber验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        return UnifiedValidator.validate_with_rules(value, "机位号", STAND_NUMBER_RULES)


class GateNumberValidator:
    """GateNumber验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        return UnifiedValidator.validate_with_rules(value, "登机口号", GATE_NUMBER_RULES)


class MissionTypeValidator:
    """MissionType验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        """验证任务类型值。"""
        result = UnifiedValidator.validate_required_field(value, "任务类型")
        if not result.is_valid:
            return result

        if not re.match(r"^[A-Z/0-9_]+$", value):
            return ValidationResult.error(
                ValidationErrorCode.INVALID_FORMAT.value,
                "任务类型格式错误，任务类型格式: 大写字母、数字、斜杠和下划线",
                {"value": value},
            )

        return ValidationResult.success()

    @staticmethod
    def validate_time_sequence(early_time: datetime, late_time: datetime) -> bool:
        """验证时间序列（早于/晚于关系）。"""
        return early_time <= late_time


class ProcessInstanceIdValidator(FlightIdValidator):
    """ProcessInstanceId验证器（与FlightId使用相同规则）"""

    pass


class UserIdValidator:
    """UserId验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        return UnifiedValidator.validate_with_rules(
            value,
            "用户ID",
            USER_ID_RULES,
            include_business_rules=False,
        )


class EmailAddressValidator:
    """EmailAddress验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        result = UnifiedValidator.validate_required_field(value, "电子邮件地址")
        if not result.is_valid:
            return result

        result = UnifiedValidator.validate_length(value, EMAIL_ADDRESS_RULES, "电子邮件地址")
        if not result.is_valid:
            return result

        # 移除格式验证和特殊规则检查，允许任意字符串作为邮箱

        return ValidationResult.success()


class PasswordHashValidator:
    """PasswordHash验证器"""

    @staticmethod
    def validate(value: str) -> ValidationResult:
        result = UnifiedValidator.validate_required_field(value, "密码哈希")
        if not result.is_valid:
            return result

        result = UnifiedValidator.validate_length(value, PASSWORD_HASH_RULES, "密码哈希")
        if not result.is_valid:
            return result

        # bcrypt格式验证
        if not value.startswith("$2") or len(value.split("$")) < 4:
            return ValidationResult.error(
                ValidationErrorCode.INVALID_FORMAT.value,
                f"密码哈希格式无效，{PASSWORD_HASH_RULES['description']}",
                {"field": "密码哈希", "value": value},
            )

        # bcrypt版本验证
        bcrypt_parts = value.split("$")
        if len(bcrypt_parts) >= 2:
            version = bcrypt_parts[1]
            if version not in PASSWORD_HASH_RULES["bcrypt_versions"]:
                return ValidationResult.error(
                    ValidationErrorCode.INVALID_FORMAT.value,
                    f"密码哈希bcrypt版本无效({version})，支持的版本: {PASSWORD_HASH_RULES['bcrypt_versions']}",
                    {"field": "密码哈希", "value": value, "version": version},
                )

        # cost因子验证
        if len(bcrypt_parts) >= 3:
            try:
                cost = int(bcrypt_parts[2])
                if cost < PASSWORD_HASH_RULES["bcrypt_cost_min"] or cost > PASSWORD_HASH_RULES["bcrypt_cost_max"]:
                    return ValidationResult.error(
                        ValidationErrorCode.INVALID_FORMAT.value,
                        f"密码哈希cost因子无效({cost})，应在{PASSWORD_HASH_RULES['bcrypt_cost_min']}-{PASSWORD_HASH_RULES['bcrypt_cost_max']}之间",
                        {"field": "密码哈希", "value": value, "cost": cost},
                    )
            except ValueError:
                return ValidationResult.error(
                    ValidationErrorCode.INVALID_FORMAT.value,
                    "密码哈希cost因子格式无效",
                    {"field": "密码哈希", "value": value},
                )

        return ValidationResult.success()


class UnifiedFlightValidator:
    """统一航班验证器"""

    @staticmethod
    def validate_flight_number(flight_number: str) -> bool:
        """验证航班号"""
        result = FlightNumberValidator.validate(flight_number)
        return result.is_valid

    @staticmethod
    def validate_airport_code(airport_code: str) -> bool:
        """验证机场代码"""
        result = AirportCodeValidator.validate(airport_code)
        return result.is_valid

    @staticmethod
    def validate_time_sequence(early_time: datetime, late_time: datetime) -> bool:
        """验证时间序列（早于/晚于关系）"""
        return early_time <= late_time


class StatusTransitionValidator:
    """状态转换验证器"""

    # 状态转换矩阵 - 定义哪些状态转换是允许的
    _ALLOWED_TRANSITIONS: ClassVar[dict[str, list[str]]] = {
        "SCHEDULED": ["CHECK_IN", "DELAYED", "CANCELLED"],
        "CHECK_IN": ["BOARDING", "CANCELLED"],
        "BOARDING": ["DEPARTED", "CANCELLED"],
        "DEPARTED": ["ARRIVED"],
        "ARRIVED": [],
        "CANCELLED": [],
        "DELAYED": ["SCHEDULED", "CANCELLED"],
    }

    @staticmethod
    def can_transition(from_status: str, to_status: str) -> bool:
        """检查是否可以从from_status转换到to_status"""
        allowed = StatusTransitionValidator._ALLOWED_TRANSITIONS.get(from_status, [])
        return to_status in allowed


class ValidationEngine:
    """统一验证引擎 - 值对象验证入口"""

    # 验证器映射表
    _VALIDATOR_MAP: ClassVar[dict[str, type[Any]]] = {
        "FlightId": FlightIdValidator,
        "FlightNumber": FlightNumberValidator,
        "AirportCode": AirportCodeValidator,
        "AircraftType": AircraftTypeValidator,
        "StandNumber": StandNumberValidator,
        "GateNumber": GateNumberValidator,
        "MissionType": MissionTypeValidator,
    }

    @classmethod
    def validate_value_object(cls, value_object: Any) -> ValidationResult:
        """验证值对象"""
        class_name = value_object.__class__.__name__
        validator = cls._VALIDATOR_MAP.get(class_name)

        if validator and hasattr(value_object, "value"):
            return validator.validate(value_object.value)

        return ValidationResult.success()

    @classmethod
    def validate_and_throw(cls, value_object: Any) -> None:
        """验证值对象，失败时抛出异常"""
        result = cls.validate_value_object(value_object)
        if not result.is_valid:
            raise DomainValidationException(f"验证失败: {result.error_message}", "validation")


def validate_and_throw(value_object: Any) -> None:
    """Validate a value object and raise on failure."""
    ValidationEngine.validate_and_throw(value_object)
