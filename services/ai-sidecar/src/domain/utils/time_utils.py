"""时间工具模块"""

import logging
from dataclasses import dataclass
from datetime import UTC, datetime, tzinfo


def utc_now() -> datetime:
    """返回当前 UTC aware 时间。"""
    return datetime.now(UTC)


def now_in_tz(target_tz: tzinfo | None) -> datetime:
    """返回指定时区当前时间，默认回退 UTC。"""
    if target_tz is None:
        return utc_now()
    return datetime.now(target_tz)


def to_utc(value: datetime | None) -> datetime | None:
    """将时间标准化为 UTC aware。"""
    if value is None:
        return None

    if value.tzinfo is None:
        return value.replace(tzinfo=UTC)

    return value.astimezone(UTC)


def to_utc_iso_z(value: datetime | None) -> str | None:
    """输出 UTC ISO 8601 字符串，后缀统一为 Z。"""
    normalized = to_utc(value)
    if normalized is None:
        return None
    return normalized.isoformat().replace("+00:00", "Z")


@dataclass
class TimeCalculationResult:
    """时间计算结果"""

    is_valid: bool
    minutes: float = 0.0
    seconds: float = 0.0
    error: str | None = None

    def __post_init__(self):
        if self.is_valid and self.minutes == 0 and self.seconds == 0:
            self.minutes = self.seconds / 60


def safe_time_difference(
    time1: datetime | None, time2: datetime | None, default_minutes: float = 0.0
) -> TimeCalculationResult:
    """
    安全计算时间差

    Args:
        time1: 第一个时间
        time2: 第二个时间
        default_minutes: 无效时的默认值（分钟）

    Returns:
        TimeCalculationResult: 时间计算结果
    """
    try:
        # 检查空值
        if time1 is None:
            return TimeCalculationResult(is_valid=False, minutes=default_minutes, error="First time parameter is None")

        if time2 is None:
            return TimeCalculationResult(is_valid=False, minutes=default_minutes, error="Second time parameter is None")

        # 计算时间差
        time_diff = time1 - time2
        seconds = time_diff.total_seconds()
        minutes = seconds / 60

        return TimeCalculationResult(is_valid=True, minutes=minutes, seconds=seconds)

    except Exception as e:  # noqa: BLE001 - time calculation may fail in various ways
        logging.getLogger(__name__).error(f"Error calculating time difference: {e}")
        return TimeCalculationResult(is_valid=False, minutes=default_minutes, error=str(e))


def safe_time_comparison(time1: datetime | None, time2: datetime | None, default_result: bool = False) -> bool:
    """
    安全比较时间

    Args:
        time1: 第一个时间
        time2: 第二个时间
        default_result: 无效时的默认结果

    Returns:
        bool: time1是否大于time2
    """
    try:
        if time1 is None or time2 is None:
            logging.getLogger(__name__).warning(f"Time comparison with None value: time1={time1}, time2={time2}")
            return default_result

        return time1 > time2

    except Exception as e:  # noqa: BLE001 - time comparison may fail in various ways
        logging.getLogger(__name__).error(f"Error comparing times: {e}")
        return default_result


def format_time_diff(minutes: float) -> str:
    """
    格式化时间差为易读字符串

    Args:
        minutes: 时间差（分钟）

    Returns:
        str: 格式化后的字符串
    """
    if abs(minutes) < 1:
        seconds = int(minutes * 60)
        return f"{seconds}秒"
    elif abs(minutes) < 60:
        return f"{minutes:.1f}分钟"
    else:
        hours = minutes / 60
        return f"{hours:.1f}小时"
