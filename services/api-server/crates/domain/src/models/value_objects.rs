//! 值对象定义
//!
//! 对应 Python `src/domain/models/value_objects.py`。
//! 包含航班状态/类型枚举、强类型 ID/编号值对象等。

use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// FlightType — 航班类型
// ---------------------------------------------------------------------------

/// 航班类型枚举 (使用数字存储)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlightType {
    /// 国内 (0)
    Domestic = 0,
    /// 国际 (1)
    International = 1,
    /// 地区 (2)
    Region = 2,
}

impl FlightType {
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Domestic => "国内",
            Self::International => "国际",
            Self::Region => "地区",
        }
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Domestic),
            1 => Some(Self::International),
            2 => Some(Self::Region),
            _ => None,
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "domestic" | "国内" | "0" => Some(Self::Domestic),
            "intl" | "international" | "国际" | "1" => Some(Self::International),
            "region" | "地区" | "2" => Some(Self::Region),
            _ => None,
        }
    }
}

impl fmt::Display for FlightType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// ---------------------------------------------------------------------------
// FlightStatus — 航班状态
// ---------------------------------------------------------------------------

/// 航班状态枚举 (使用数字存储)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlightStatus {
    /// 计划中 (0)
    Scheduled = 0,
    /// 前站起飞 (1)
    PrevDeparted = 1,
    /// 到达本站 (2)
    Arrived = 2,
    /// 值机结束 (3)
    CheckInEnd = 3,
    /// 登机 (4)
    Boarding = 4,
    /// 催促登机 (5)
    BoardingUrge = 5,
    /// 结束登机 (6)
    BoardingEnd = 6,
    /// 已起飞 (7)
    Departed = 7,
    /// 到下站 (8)
    NextArrived = 8,
    /// 取消 (9)
    Cancelled = 9,
    /// 延误 (10)
    Delayed = 10,
}

impl FlightStatus {
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Scheduled => "计划中",
            Self::PrevDeparted => "前站起飞",
            Self::Arrived => "到达本站",
            Self::CheckInEnd => "值机结束",
            Self::Boarding => "登机",
            Self::BoardingUrge => "催促登机",
            Self::BoardingEnd => "结束登机",
            Self::Departed => "已起飞",
            Self::NextArrived => "到下站",
            Self::Cancelled => "取消",
            Self::Delayed => "延误",
        }
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Scheduled),
            1 => Some(Self::PrevDeparted),
            2 => Some(Self::Arrived),
            3 => Some(Self::CheckInEnd),
            4 => Some(Self::Boarding),
            5 => Some(Self::BoardingUrge),
            6 => Some(Self::BoardingEnd),
            7 => Some(Self::Departed),
            8 => Some(Self::NextArrived),
            9 => Some(Self::Cancelled),
            10 => Some(Self::Delayed),
            _ => None,
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        let normalized = s.trim();
        // 按中文标签匹配
        match normalized {
            "计划中" => return Some(Self::Scheduled),
            "前站起飞" => return Some(Self::PrevDeparted),
            "到达本站" => return Some(Self::Arrived),
            "值机结束" => return Some(Self::CheckInEnd),
            "登机" => return Some(Self::Boarding),
            "催促登机" => return Some(Self::BoardingUrge),
            "结束登机" => return Some(Self::BoardingEnd),
            "已起飞" => return Some(Self::Departed),
            "到下站" => return Some(Self::NextArrived),
            "取消" => return Some(Self::Cancelled),
            "延误" => return Some(Self::Delayed),
            _ => {}
        }
        // 按英文名匹配 (case-insensitive)
        match normalized.to_uppercase().as_str() {
            "SCHEDULED" => Some(Self::Scheduled),
            "PREV_DEPARTED" => Some(Self::PrevDeparted),
            "ARRIVED" => Some(Self::Arrived),
            "CHECK_IN_END" => Some(Self::CheckInEnd),
            "BOARDING" => Some(Self::Boarding),
            "BOARDING_URGE" => Some(Self::BoardingUrge),
            "BOARDING_END" => Some(Self::BoardingEnd),
            "DEPARTED" => Some(Self::Departed),
            "NEXT_ARRIVED" => Some(Self::NextArrived),
            "CANCELLED" => Some(Self::Cancelled),
            "DELAYED" => Some(Self::Delayed),
            _ => {
                // 尝试数字
                normalized.parse::<i32>().ok().and_then(Self::from_code)
            }
        }
    }
}

impl fmt::Display for FlightStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// ---------------------------------------------------------------------------
// 强类型 ID / 编号值对象（newtype 模式）
// ---------------------------------------------------------------------------

macro_rules! define_string_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

define_string_newtype!(
    /// 航班 ID 值对象
    FlightId
);

define_string_newtype!(
    /// 航班号值对象
    FlightNumber
);

define_string_newtype!(
    /// 机场代码值对象 (IATA/ICAO)
    AirportCode
);

define_string_newtype!(
    /// 机型值对象
    AircraftType
);

define_string_newtype!(
    /// 机位号值对象
    StandNumber
);

define_string_newtype!(
    /// 登机口号值对象
    GateNumber
);

define_string_newtype!(
    /// 用户 ID 值对象
    UserId
);

define_string_newtype!(
    /// 流程实例 ID 值对象
    ProcessInstanceId
);

define_string_newtype!(
    /// 任务类型代码值对象
    MissionType
);

impl FlightNumber {
    /// 提取航空公司代码 (前 2-3 位大写字母)
    pub fn airline_code(&self) -> &str {
        let bytes = self.0.as_bytes();
        let end = bytes
            .iter()
            .position(|b| !b.is_ascii_uppercase())
            .unwrap_or(bytes.len())
            .min(3);
        &self.0[..end]
    }
}

impl FlightId {
    /// 生成新的航班 ID (ULID)
    pub fn generate() -> Self {
        Self(ulid::Ulid::new().to_string())
    }
}

impl UserId {
    /// 生成新的用户 ID (ULID)
    pub fn generate() -> Self {
        Self(ulid::Ulid::new().to_string())
    }
}
