"""任务性质枚举。

根据表 2 定义的任务性质枚举，数据库内统一使用数值存储。
"""

from enum import Enum
from typing import Optional


class MissionTypeEnum(Enum):
    """任务性质枚举"""

    # 数值 代码  任务性质
    A_V = (1, "A/V", "航线熟练飞行")
    B_F = (2, "B/F", "播种飞行")
    B_W = (3, "B/W", "专机飞行")
    C_B = (4, "C/B", "旅客加班")
    D_M = (5, "D/M", "展示飞行")
    D_Y = (6, "D/Y", "带飞飞行")
    F_J = (7, "F/J", "校验飞行")
    H_G = (8, "H/G", "货运包机")
    H_Y = (9, "H/Y", "货运加班")
    J_B = (10, "J/B", "按专机保障的定期航班")
    K_L = (11, "K/L", "本场训练飞行")
    L_W = (12, "L/W", "旅客包机")
    N_M = (13, "N/M", "调机飞行")
    R_Z = (14, "R/Z", "试航飞行")
    S_F = (15, "S/F", "试飞飞行")
    U_H = (16, "U/H", "公务飞行")
    VIP = (17, "VIP", "要客飞行")
    X_L = (18, "X/L", "训练飞行")
    O_F = (19, "O/F", "急救飞行")
    W_Z = (20, "W/Z", "正班飞行")
    Z_P = (21, "Z/P", "补班飞行")
    Z_F = (22, "Z/F", "执法飞行")
    Y_Z = (23, "Y/Z", "验证飞行")
    W_A = (24, "W/A", "转场飞行")
    S_Q = (25, "S/Q", "视察飞行（含巡线飞行）")
    H_F = (26, "H/F", "航摄飞行")
    X_X = (27, "X/X", "其他飞行")
    OVERFLIGHT = (28, "OVERFLIGHT", "临时飞越")
    # 29 空 空
    TECH_STOP = (31, "TECH_STOP", "技术经停")
    # 32 空
    # 33 空 空

    def __init__(self, numeric_value: int, code: str, description: str):
        self.numeric_value = numeric_value
        self.code = code
        self.description = description

    @classmethod
    def from_numeric_value(cls, numeric_value: int) -> Optional["MissionTypeEnum"]:
        """根据数字值获取枚举"""
        for item in cls:
            if item.numeric_value == numeric_value:
                return item
        return None

    @classmethod
    def from_code(cls, code: str) -> Optional["MissionTypeEnum"]:
        """根据代码获取枚举"""
        normalized = cls._normalize_code(code)
        if not normalized:
            return None
        for item in cls:
            if item.code == normalized:
                return item
        return None

    @classmethod
    def from_any(cls, value: object) -> Optional["MissionTypeEnum"]:
        """根据数值或代码获取枚举。"""
        if value is None:
            return None
        if isinstance(value, cls):
            return value
        if isinstance(value, int):
            return cls.from_numeric_value(value)
        if isinstance(value, str):
            normalized = value.strip()
            if not normalized:
                return None
            if normalized.isdigit():
                return cls.from_numeric_value(int(normalized))
            return cls.from_code(normalized)
        return None

    @classmethod
    def normalize_numeric_value(cls, value: object) -> int | None:
        """将输入标准化为数值任务类型。"""
        mission = cls.from_any(value)
        return mission.numeric_value if mission else None

    @staticmethod
    def _normalize_code(code: object) -> str:
        text = str(code or "").strip()
        if not text:
            return ""
        normalized = text.upper().replace("／", "/").replace("TECH STOP", "TECH_STOP")
        return normalized

    @classmethod
    def get_all_codes(cls) -> dict[str, str]:
        """获取所有代码和描述的映射"""
        return {item.code: item.description for item in cls}

    def __str__(self):
        return f"{self.code} - {self.description}"

    def __repr__(self):
        return f"MissionTypeEnum.{self.name}({self.numeric_value}, '{self.code}', '{self.description}')"
