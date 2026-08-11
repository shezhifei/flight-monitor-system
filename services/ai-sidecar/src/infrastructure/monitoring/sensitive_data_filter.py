"""敏感数据过滤器模块

提供过滤日志中敏感信息的功能，防止密码、密钥等敏感数据泄露到日志文件中
"""

import re
from typing import Any, ClassVar


class SensitiveDataFilter:
    """敏感数据过滤器，用于过滤日志中的敏感信息

    重构说明：已从 logging.Filter 子类降级为普通工具类，
    脱敏功能通过 structlog processor 集成。
    """

    # 定义敏感字段的正则表达式模式（仅用于非键值对的通用敏感信息）
    SENSITIVE_PATTERNS: ClassVar[list[str]] = [
        # 信用卡号 (简单模式)
        r"\b(?:\d{4}[-\s]?){3}\d{4}\b",
        # 邮箱地址
        r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b",
        # 手机号码 (中国)
        r"\b1[3-9]\d{9}\b",
        # 身份证号 (简单模式)
        r"\b\d{17}[\dXx]\b",
        # IP地址
        r"\b(?:\d{1,3}\.){3}\d{1,3}\b",
        # 社会安全号
        r"\b\d{3}-?\d{2}-?\d{4}\b",
        # 护照号码
        r"\b[PCDS]\d{7,8}\b",
        # 银行卡号
        r"\b(?:\d{4}[-\s]?){3,4}\d{4,7}\b",
        # 统一社会信用代码
        r"\b[0-9A-HJ-NPQRTUWXY]{2}\d{6}[0-9A-HJ-NPQRTUWXY]{10}\b",
    ]

    # 敏感字段名列表
    SENSITIVE_FIELD_NAMES: ClassVar[set[str]] = {
        "password",
        "pwd",
        "secret",
        "key",
        "token",
        "api_key",
        "api-token",
        "auth",
        "authorization",
        "bearer",
        "sessionid",
        "session_id",
        "access_token",
        "refresh_token",
        "client_secret",
        "private_key",
        "public_key",
        "jwt",
        "cookie",
        "x-api-key",
        "x-auth-token",
        "session_token",
        "credential",
        "phone",
        "email",
        "ssn",
    }

    def __init__(self):
        # 编译正则表达式以提高性能
        self.compiled_patterns = [re.compile(pattern) for pattern in self.SENSITIVE_PATTERNS]

    def sanitize_data(self, data: Any) -> Any:
        """对数据进行脱敏处理"""
        if isinstance(data, str):
            return self._sanitize_string(data)
        elif isinstance(data, dict):
            return self.sanitize_dict(data)
        elif isinstance(data, list):
            return self.sanitize_list(data)
        else:
            return data

    def sanitize_dict(self, data: dict[str, Any]) -> dict[str, Any]:
        """对字典中的敏感数据进行脱敏"""
        if not isinstance(data, dict):
            return data

        sanitized = {}
        for key, value in data.items():
            if self._is_sensitive_field(key):
                sanitized[key] = self._mask_sensitive_value(value)
            elif isinstance(value, dict):
                sanitized[key] = self.sanitize_dict(value)
            elif isinstance(value, list):
                sanitized[key] = self.sanitize_list(value)
            elif isinstance(value, str):
                sanitized[key] = self._sanitize_string(value)
            else:
                sanitized[key] = value
        return sanitized

    def sanitize_list(self, data: list[Any]) -> list[Any]:
        """对列表中的敏感数据进行脱敏"""
        if not isinstance(data, list):
            return data

        sanitized = []
        for item in data:
            if isinstance(item, dict):
                sanitized.append(self.sanitize_dict(item))
            elif isinstance(item, list):
                sanitized.append(self.sanitize_list(item))
            elif isinstance(item, str):
                sanitized.append(self._sanitize_string(item))
            else:
                sanitized.append(item)
        return sanitized

    def _is_sensitive_field(self, field_name: str) -> bool:
        """判断字段名是否为敏感字段"""
        field_lower = field_name.lower()
        if field_lower in self.SENSITIVE_FIELD_NAMES:
            return True
        return any(pattern.search(field_name) for pattern in self.compiled_patterns)

    def _mask_sensitive_value(self, value: Any) -> str:
        """对敏感值进行掩码处理"""
        if value is None:
            return "****"
        if isinstance(value, (str, int, float)):
            str_val = str(value)
            if len(str_val) <= 4:
                return "****"
            else:
                return f"{str_val[:2]}****{str_val[-2:]}" if len(str_val) > 4 else "****"
        else:
            return "****"

    def _sanitize_string(self, text: str) -> str:
        """对字符串中的敏感信息进行脱敏"""
        if not isinstance(text, str):
            return text

        sensitive_keywords = [
            "password",
            "pwd",
            "secret",
            "key",
            "token",
            "api_key",
            "api-key",
            "auth",
            "authorization",
            "bearer",
            "sessionid",
            "session_id",
            "access_token",
            "refresh_token",
            "client_secret",
            "private_key",
            "public_key",
            "jwt",
            "cookie",
            "x-api-key",
            "x-auth-token",
            "session_token",
            "密码",
            "密钥",
            "令牌",
            "口令",
            "秘钥",
        ]

        for keyword in sensitive_keywords:
            pattern1 = rf'({keyword})\s*:\s*([^\s,;\'"]+)'
            text = re.sub(
                pattern1, lambda m: f"{m.group(1)}: {self._mask_sensitive_value(m.group(2))}", text, flags=re.IGNORECASE
            )

            pattern2 = rf'({keyword})\s*=\s*([^\s,;\'"]+)'
            text = re.sub(
                pattern2, lambda m: f"{m.group(1)}={self._mask_sensitive_value(m.group(2))}", text, flags=re.IGNORECASE
            )

            pattern3 = rf'\b({keyword})\s+([^\s,;\'"]+)'
            text = re.sub(
                pattern3, lambda m: f"{m.group(1)} {self._mask_sensitive_value(m.group(2))}", text, flags=re.IGNORECASE
            )

            pattern4 = rf'({keyword})\s+(is|contains|has|value|equals?)\s+([^\s,;\'"]+)'
            text = re.sub(
                pattern4,
                lambda m: f"{m.group(1)} {m.group(2)} {self._mask_sensitive_value(m.group(3))}",
                text,
                flags=re.IGNORECASE,
            )

        for pattern in self.compiled_patterns:
            text = pattern.sub(self._replace_match, text)

        return text

    def _replace_match(self, match: re.Match) -> str:
        """替换匹配到的敏感信息"""
        full_match = match.group(0)
        if len(match.groups()) >= 2:
            key = match.group(1)
            value = match.group(2)

            if len(value) <= 4:
                sanitized_value = "****"
            else:
                clean_value = value.strip("'\"")
                sanitized_value = f"{clean_value[:2]}****{clean_value[-2:]}" if len(clean_value) > 4 else "****"

            if ": " in full_match or "=" in full_match:
                return f"{key}{full_match[match.end(1) : match.start(2)]}{sanitized_value}"
            else:
                return f"{key} {sanitized_value}"
        else:
            matched_text = match.group(0)
            if len(matched_text) <= 4:
                return "****"
            else:
                return f"{matched_text[:2]}****{matched_text[-2:]}"


_filter_instance: SensitiveDataFilter | None = None


def configure_sensitive_data_filter(instance: SensitiveDataFilter) -> None:
    """由 installer / 组合根调用，注入过滤器实例。"""
    global _filter_instance
    _filter_instance = instance


def get_sensitive_data_filter() -> SensitiveDataFilter:
    """获取敏感数据过滤器实例（惰性创建兜底）"""
    global _filter_instance
    if _filter_instance is None:
        _filter_instance = SensitiveDataFilter()
    return _filter_instance


def sanitize_log_message(message: str | dict | list | Any) -> str | dict | list | Any:
    """对外提供的脱敏函数，可以直接对消息进行脱敏"""
    return get_sensitive_data_filter().sanitize_data(message)
