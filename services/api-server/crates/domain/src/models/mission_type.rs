//! 任务性质枚举
//!
//! 对应 Python `src/domain/models/mission_type_enum.py`。
//! 数据库内统一使用数值存储。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 任务性质枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MissionTypeEntry {
    pub numeric_value: i32,
    pub code: &'static str,
    pub description: &'static str,
}

/// 全部任务性质定义 (常量表)
pub const MISSION_TYPES: &[MissionTypeEntry] = &[
    MissionTypeEntry {
        numeric_value: 1,
        code: "A/V",
        description: "航线熟练飞行",
    },
    MissionTypeEntry {
        numeric_value: 2,
        code: "B/F",
        description: "播种飞行",
    },
    MissionTypeEntry {
        numeric_value: 3,
        code: "B/W",
        description: "专机飞行",
    },
    MissionTypeEntry {
        numeric_value: 4,
        code: "C/B",
        description: "旅客加班",
    },
    MissionTypeEntry {
        numeric_value: 5,
        code: "D/M",
        description: "展示飞行",
    },
    MissionTypeEntry {
        numeric_value: 6,
        code: "D/Y",
        description: "带飞飞行",
    },
    MissionTypeEntry {
        numeric_value: 7,
        code: "F/J",
        description: "校验飞行",
    },
    MissionTypeEntry {
        numeric_value: 8,
        code: "H/G",
        description: "货运包机",
    },
    MissionTypeEntry {
        numeric_value: 9,
        code: "H/Y",
        description: "货运加班",
    },
    MissionTypeEntry {
        numeric_value: 10,
        code: "J/B",
        description: "按专机保障的定期航班",
    },
    MissionTypeEntry {
        numeric_value: 11,
        code: "K/L",
        description: "本场训练飞行",
    },
    MissionTypeEntry {
        numeric_value: 12,
        code: "L/W",
        description: "旅客包机",
    },
    MissionTypeEntry {
        numeric_value: 13,
        code: "N/M",
        description: "调机飞行",
    },
    MissionTypeEntry {
        numeric_value: 14,
        code: "R/Z",
        description: "试航飞行",
    },
    MissionTypeEntry {
        numeric_value: 15,
        code: "S/F",
        description: "试飞飞行",
    },
    MissionTypeEntry {
        numeric_value: 16,
        code: "U/H",
        description: "公务飞行",
    },
    MissionTypeEntry {
        numeric_value: 17,
        code: "VIP",
        description: "要客飞行",
    },
    MissionTypeEntry {
        numeric_value: 18,
        code: "X/L",
        description: "训练飞行",
    },
    MissionTypeEntry {
        numeric_value: 19,
        code: "O/F",
        description: "急救飞行",
    },
    MissionTypeEntry {
        numeric_value: 20,
        code: "W/Z",
        description: "正班飞行",
    },
    MissionTypeEntry {
        numeric_value: 21,
        code: "Z/P",
        description: "补班飞行",
    },
    MissionTypeEntry {
        numeric_value: 22,
        code: "Z/F",
        description: "执法飞行",
    },
    MissionTypeEntry {
        numeric_value: 23,
        code: "Y/Z",
        description: "验证飞行",
    },
    MissionTypeEntry {
        numeric_value: 24,
        code: "W/A",
        description: "转场飞行",
    },
    MissionTypeEntry {
        numeric_value: 25,
        code: "S/Q",
        description: "视察飞行（含巡线飞行）",
    },
    MissionTypeEntry {
        numeric_value: 26,
        code: "H/F",
        description: "航摄飞行",
    },
    MissionTypeEntry {
        numeric_value: 27,
        code: "X/X",
        description: "其他飞行",
    },
    MissionTypeEntry {
        numeric_value: 28,
        code: "OVERFLIGHT",
        description: "临时飞越",
    },
    MissionTypeEntry {
        numeric_value: 31,
        code: "TECH_STOP",
        description: "技术经停",
    },
];

/// 根据数字值查找
pub fn from_numeric_value(value: i32) -> Option<&'static MissionTypeEntry> {
    MISSION_TYPES.iter().find(|e| e.numeric_value == value)
}

/// 根据代码查找
pub fn from_code(code: &str) -> Option<&'static MissionTypeEntry> {
    let normalized = normalize_code(code);
    if normalized.is_empty() {
        return None;
    }
    MISSION_TYPES.iter().find(|e| e.code == normalized)
}

/// 将值标准化为数值任务类型
pub fn normalize_numeric_value(value: Option<i32>) -> Option<i32> {
    value.and_then(|v| from_numeric_value(v).map(|e| e.numeric_value))
}

fn normalize_code(code: &str) -> String {
    code.trim()
        .to_uppercase()
        .replace('／', "/")
        .replace("TECH STOP", "TECH_STOP")
}

impl fmt::Display for MissionTypeEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} - {}", self.code, self.description)
    }
}
