//! 航班 Leg 模型
//!
//! 对应 Python `src/domain/models/flight_leg.py`。
//! 表示航班的进/出港航段。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 航段方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LegType {
    Inbound,
    Outbound,
}

/// 航班类型代码 (domestic / intl / region)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlightTypeCode {
    Domestic,
    Intl,
    Region,
}

impl Default for FlightTypeCode {
    fn default() -> Self {
        Self::Domestic
    }
}

/// 航班航段 — 一个方向的进港或出港航段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightLeg {
    pub leg_type: LegType,
    pub flight_no: String,
    #[serde(default)]
    pub flight_type: FlightTypeCode,
    /// 任务性质 (数字值)
    pub mission: Option<i32>,
    pub origin_code: Option<String>,
    pub destination_code: Option<String>,
    pub origin_name: Option<String>,
    pub destination_name: Option<String>,
    #[serde(default)]
    pub is_vip: bool,
    pub stand_type: Option<String>,
    pub scheduled_time: Option<DateTime<Utc>>,
}
