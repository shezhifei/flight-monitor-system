//! 航班领域模型
//!
//! 对应 Python `src/domain/models/flight.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::flight_leg::FlightLeg;
use super::value_objects::{AircraftType, FlightId, FlightNumber, FlightStatus, GateNumber, StandNumber};

/// 航班实体 (核心聚合根)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flight {
    pub flight_id: FlightId,

    // 核心身份与资源状态
    pub airline_code: Option<String>,
    pub flight_number: Option<FlightNumber>,
    pub registration: Option<String>,
    pub aircraft_type_detail: Option<AircraftType>,
    pub stand: Option<StandNumber>,
    pub gate: Option<GateNumber>,
    pub terminal: Option<String>,
    pub position: Option<String>,
    pub baggage_carousel: Option<String>,

    // 时间节点
    pub scheduled_departure: Option<DateTime<Utc>>,
    pub scheduled_arrival: Option<DateTime<Utc>>,
    pub estimated_departure: Option<DateTime<Utc>>,
    pub estimated_arrival: Option<DateTime<Utc>>,
    pub actual_departure: Option<DateTime<Utc>>,
    pub actual_arrival: Option<DateTime<Utc>>,
    pub cobt_time: Option<DateTime<Utc>>,
    pub codt: Option<DateTime<Utc>>,

    // 业务标志
    #[serde(default)]
    pub has_boarding_restriction: bool,
    #[serde(default)]
    pub is_quick_turnaround: bool,
    #[serde(default = "default_true")]
    pub is_commercial_signed: bool,

    // 聚合状态
    #[serde(default)]
    pub status: FlightStatus,
    pub inbound_leg: Option<FlightLeg>,
    pub outbound_leg: Option<FlightLeg>,
    #[serde(default)]
    pub anomaly_summary: HashMap<String, serde_json::Value>,

    // 审计
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub version: i32,
    #[serde(default)]
    pub labels: Vec<String>,

    // 备注
    pub flight_remarks: Option<String>,
    pub load_planning_remarks: Option<String>,
    pub aircraft_maintenance_remarks: Option<String>,
    pub aircraft_check_remarks: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for FlightStatus {
    fn default() -> Self {
        Self::Scheduled
    }
}

impl Flight {
    /// 是否为进港航班
    pub fn is_arrival_flight(&self) -> bool {
        self.inbound_leg.is_some()
    }

    /// 是否为出港航班
    pub fn is_departure_flight(&self) -> bool {
        self.outbound_leg.is_some()
    }

    /// 是否为过站航班
    pub fn is_turnaround_flight(&self) -> bool {
        self.inbound_leg.is_some() && self.outbound_leg.is_some()
    }

    /// 获取所有目的地代码
    pub fn get_destination_codes(&self) -> Vec<String> {
        [&self.inbound_leg, &self.outbound_leg]
            .iter()
            .filter_map(|leg| {
                leg.as_ref()
                    .and_then(|l| l.destination_code.as_deref())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            })
            .collect()
    }

    /// 获取所有出发地代码
    pub fn get_origin_codes(&self) -> Vec<String> {
        [&self.inbound_leg, &self.outbound_leg]
            .iter()
            .filter_map(|leg| {
                leg.as_ref()
                    .and_then(|l| l.origin_code.as_deref())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            })
            .collect()
    }

    /// 获取所有航班号
    pub fn get_flight_numbers(&self) -> Vec<String> {
        [&self.outbound_leg, &self.inbound_leg]
            .iter()
            .filter_map(|leg| {
                leg.as_ref()
                    .map(|l| l.flight_no.trim())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            })
            .collect()
    }
}
