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

    // 本体 V1（ONTOLOGY_V1.md §4.2）
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default = "default_flight_kind")]
    pub flight_kind: String,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub divert: bool,

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

fn default_flight_kind() -> String {
    "passenger".to_string()
}

impl Flight {
    /// Validate the post-migration identity contract. Existing rows may still
    /// have a null direction during the staged F3 rollout, but new writes may
    /// only identify one directional flight; the legacy `both` value is
    /// rejected at the write boundary.
    pub fn validate_direction_contract(&self) -> Result<(), String> {
        if let Some(direction) = self.direction.as_deref() {
            match direction.trim().to_ascii_lowercase().as_str() {
                "inbound" | "outbound" => Ok(()),
                "both" => Err("direction=both 已废弃；请拆分为 inbound/outbound 航班".into()),
                _ => Err("direction 仅支持 inbound 或 outbound".into()),
            }
        } else {
            Ok(())
        }
    }

    /// Return the leg represented by this flight's canonical direction.
    ///
    /// During the F4 transition old aggregate rows may still carry both
    /// compatibility legs and no direction. Those rows intentionally fall
    /// back to the legacy helpers below; once `direction` is present, the
    /// directional column is authoritative and the opposite compatibility leg
    /// is ignored.
    pub fn directional_leg(&self) -> Option<&FlightLeg> {
        match self.direction.as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("inbound") => self.inbound_leg.as_ref(),
            Some("outbound") => self.outbound_leg.as_ref(),
            _ => None,
        }
    }

    /// Compatibility view for callers that still need an inbound/outbound
    /// slot while the DTO migration is in progress. Directional flights only
    /// expose their canonical side; legacy aggregate rows expose both slots.
    pub fn inbound_leg_view(&self) -> Option<&FlightLeg> {
        match self.direction.as_deref() {
            Some("inbound") => self.directional_leg(),
            Some("outbound") => None,
            _ => self.inbound_leg.as_ref(),
        }
    }

    pub fn outbound_leg_view(&self) -> Option<&FlightLeg> {
        match self.direction.as_deref() {
            Some("outbound") => self.directional_leg(),
            Some("inbound") => None,
            _ => self.outbound_leg.as_ref(),
        }
    }

    /// Whether this instance is still an un-split legacy aggregate.
    pub fn is_legacy_aggregate(&self) -> bool {
        self.direction.is_none() && self.inbound_leg.is_some() && self.outbound_leg.is_some()
    }

    /// 是否为进港航班
    pub fn is_arrival_flight(&self) -> bool {
        match self.direction.as_deref() {
            Some("inbound") => true,
            Some("outbound") => false,
            _ => self.inbound_leg.is_some(),
        }
    }

    /// 是否为出港航班
    pub fn is_departure_flight(&self) -> bool {
        match self.direction.as_deref() {
            Some("outbound") => true,
            Some("inbound") => false,
            _ => self.outbound_leg.is_some(),
        }
    }

    /// 是否为过站航班
    pub fn is_turnaround_flight(&self) -> bool {
        self.is_legacy_aggregate()
    }

    /// 获取所有目的地代码
    pub fn get_destination_codes(&self) -> Vec<String> {
        if let Some(leg) = self.directional_leg() {
            return leg
                .destination_code
                .as_deref()
                .filter(|code| !code.is_empty())
                .map(|code| vec![code.to_string()])
                .unwrap_or_default();
        }
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
        if let Some(leg) = self.directional_leg() {
            return leg
                .origin_code
                .as_deref()
                .filter(|code| !code.is_empty())
                .map(|code| vec![code.to_string()])
                .unwrap_or_default();
        }
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
        if let Some(leg) = self.directional_leg() {
            let number = leg.flight_no.trim();
            return if number.is_empty() {
                Vec::new()
            } else {
                vec![number.to_string()]
            };
        }
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

#[cfg(test)]
mod identity_tests {
    use super::{Flight, FlightStatus};
    use crate::models::flight_leg::{FlightLeg, LegType};
    use std::collections::HashMap;

    fn flight(direction: Option<&str>) -> Flight {
        Flight {
            flight_id: "f1".into(),
            airline_code: None,
            flight_number: None,
            registration: None,
            aircraft_type_detail: None,
            stand: None,
            gate: None,
            terminal: None,
            position: None,
            baggage_carousel: None,
            scheduled_departure: None,
            scheduled_arrival: None,
            estimated_departure: None,
            estimated_arrival: None,
            actual_departure: None,
            actual_arrival: None,
            cobt_time: None,
            codt: None,
            has_boarding_restriction: false,
            is_quick_turnaround: false,
            is_commercial_signed: true,
            status: FlightStatus::Scheduled,
            inbound_leg: None,
            outbound_leg: None,
            anomaly_summary: HashMap::new(),
            direction: direction.map(str::to_string),
            flight_kind: "passenger".into(),
            is_draft: false,
            divert: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 0,
            labels: vec![],
            flight_remarks: None,
            load_planning_remarks: None,
            aircraft_maintenance_remarks: None,
            aircraft_check_remarks: None,
        }
    }

    #[test]
    fn direction_contract_rejects_legacy_both() {
        assert!(flight(Some("inbound")).validate_direction_contract().is_ok());
        assert!(flight(Some("both")).validate_direction_contract().is_err());
        assert!(flight(Some("sideways")).validate_direction_contract().is_err());
    }

    #[test]
    fn directional_identity_ignores_opposite_compatibility_leg() {
        let mut value = flight(Some("outbound"));
        value.inbound_leg = Some(FlightLeg {
            leg_type: LegType::Inbound,
            flight_no: "IN-LEGACY".into(),
            flight_type: Default::default(),
            mission: None,
            origin_code: Some("OLD-ORIGIN".into()),
            destination_code: Some("OLD-DEST".into()),
            origin_name: None,
            destination_name: None,
            is_vip: false,
            stand_type: None,
            scheduled_time: None,
        });
        value.outbound_leg = Some(FlightLeg {
            leg_type: LegType::Outbound,
            flight_no: "OUT-CANONICAL".into(),
            flight_type: Default::default(),
            mission: None,
            origin_code: Some("CANONICAL-ORIGIN".into()),
            destination_code: Some("CANONICAL-DEST".into()),
            origin_name: None,
            destination_name: None,
            is_vip: false,
            stand_type: None,
            scheduled_time: None,
        });

        assert_eq!(value.get_flight_numbers(), vec!["OUT-CANONICAL"]);
        assert_eq!(value.get_origin_codes(), vec!["CANONICAL-ORIGIN"]);
        assert!(value.is_departure_flight());
        assert!(!value.is_arrival_flight());
        assert!(!value.is_turnaround_flight());
    }
}
