//! 航班系统 DTO 模式
//!
//! 对应 Python `src/application/schemas/flight_schemas.py`。

use chrono::{DateTime, Utc};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use std::fmt;
use std::marker::PhantomData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStationPayload {
    pub code: String,
    pub name: Option<String>,
}

// ---------------------------------------------------------------------------
// 航段载荷
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlightLegPayload {
    pub leg_type: String, // "inbound" | "outbound"
    pub flight_no: String,
    #[serde(default = "default_domestic")]
    pub flight_type: String, // "domestic" | "intl" | "region"
    pub mission: Option<i32>,
    #[serde(default)]
    pub origin_stations: Vec<RouteStationPayload>,
    #[serde(default)]
    pub destination_stations: Vec<RouteStationPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_name: Option<String>,
    #[serde(default)]
    pub is_vip: bool,
    pub stand_type: Option<String>,
    pub scheduled_time: Option<DateTime<Utc>>,
}

fn default_domestic() -> String {
    "domestic".to_string()
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// 异常摘要
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlightAnomalySummary {
    #[serde(default)]
    pub has_open_anomaly: bool,
    #[serde(default)]
    pub open_count: i32,
    #[serde(default)]
    pub acknowledged_count: i32,
}

// ---------------------------------------------------------------------------
// 风险与下一步动作
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightRiskReason {
    pub code: String,
    pub label: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightNextAction {
    pub code: String,
    pub label: String,
    pub target: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightDataFreshness {
    pub source: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightRiskAssessment {
    pub risk_score: i32,
    pub risk_level: String,
    pub risk_reasons: Vec<FlightRiskReason>,
    pub next_primary_action: Option<FlightNextAction>,
    pub data_freshness: FlightDataFreshness,
}

// ---------------------------------------------------------------------------
// 创建
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlightCreate {
    pub flight_id: Option<String>,
    pub flight_number: Option<String>,
    pub airline_code: Option<String>,
    pub registration: Option<String>,
    pub aircraft_type_detail: Option<String>,
    #[serde(default = "default_scheduled")]
    pub status: Option<String>,

    pub scheduled_departure: Option<DateTime<Utc>>,
    pub scheduled_arrival: Option<DateTime<Utc>>,
    pub estimated_departure: Option<DateTime<Utc>>,
    pub estimated_arrival: Option<DateTime<Utc>>,
    pub actual_departure: Option<DateTime<Utc>>,
    pub actual_arrival: Option<DateTime<Utc>>,

    pub stand: Option<String>,
    pub gate: Option<String>,
    pub terminal: Option<String>,
    pub position: Option<String>,
    pub baggage_carousel: Option<String>,

    #[serde(default)]
    pub has_boarding_restriction: bool,
    #[serde(default)]
    pub is_quick_turnaround: bool,
    #[serde(default = "default_true")]
    pub is_commercial_signed: bool,

    pub inbound_leg: Option<FlightLegPayload>,
    pub outbound_leg: Option<FlightLegPayload>,

    pub flight_remarks: Option<String>,
    pub load_planning_remarks: Option<String>,
    pub aircraft_maintenance_remarks: Option<String>,
    pub aircraft_check_remarks: Option<String>,
}

fn default_scheduled() -> Option<String> {
    Some("SCHEDULED".to_string())
}

// ---------------------------------------------------------------------------
// 更新 (全部 Option)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NullableUpdate<T> {
    Unset,
    Clear,
    Set(T),
}

impl<T> Default for NullableUpdate<T> {
    fn default() -> Self {
        Self::Unset
    }
}

impl<T> NullableUpdate<T> {
    pub fn is_touched(&self) -> bool {
        !matches!(self, Self::Unset)
    }

    pub fn as_ref(&self) -> NullableUpdate<&T> {
        match self {
            Self::Unset => NullableUpdate::Unset,
            Self::Clear => NullableUpdate::Clear,
            Self::Set(value) => NullableUpdate::Set(value),
        }
    }
}

impl<T> Serialize for NullableUpdate<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Unset | Self::Clear => serializer.serialize_none(),
            Self::Set(value) => serializer.serialize_some(value),
        }
    }
}

impl<'de, T> Deserialize<'de> for NullableUpdate<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NullableUpdateVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for NullableUpdateVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = NullableUpdate<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an omitted field, null, or a concrete value")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(NullableUpdate::Clear)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(NullableUpdate::Clear)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                T::deserialize(deserializer).map(NullableUpdate::Set)
            }
        }

        deserializer.deserialize_option(NullableUpdateVisitor(PhantomData))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlightUpdate {
    #[serde(default)]
    pub expected_version: Option<i32>,

    pub status: Option<String>,
    #[serde(default)]
    pub gate: NullableUpdate<String>,
    #[serde(default)]
    pub terminal: NullableUpdate<String>,
    #[serde(default)]
    pub stand: NullableUpdate<String>,
    #[serde(default)]
    pub position: NullableUpdate<String>,
    #[serde(default)]
    pub baggage_carousel: NullableUpdate<String>,

    #[serde(default)]
    pub scheduled_departure: NullableUpdate<DateTime<Utc>>,
    #[serde(default)]
    pub scheduled_arrival: NullableUpdate<DateTime<Utc>>,
    #[serde(default)]
    pub estimated_departure: NullableUpdate<DateTime<Utc>>,
    #[serde(default)]
    pub estimated_arrival: NullableUpdate<DateTime<Utc>>,
    #[serde(default)]
    pub actual_departure: NullableUpdate<DateTime<Utc>>,
    #[serde(default)]
    pub actual_arrival: NullableUpdate<DateTime<Utc>>,
    #[serde(default)]
    pub cobt_time: NullableUpdate<DateTime<Utc>>,

    #[serde(default)]
    pub aircraft_type_detail: NullableUpdate<String>,
    #[serde(default)]
    pub registration: NullableUpdate<String>,

    pub has_boarding_restriction: Option<bool>,
    pub is_quick_turnaround: Option<bool>,
    pub is_commercial_signed: Option<bool>,

    #[serde(default)]
    pub inbound_leg: NullableUpdate<FlightLegPayload>,
    #[serde(default)]
    pub outbound_leg: NullableUpdate<FlightLegPayload>,

    #[serde(default)]
    pub flight_remarks: NullableUpdate<String>,
    #[serde(default)]
    pub load_planning_remarks: NullableUpdate<String>,
    #[serde(default)]
    pub aircraft_maintenance_remarks: NullableUpdate<String>,
    #[serde(default)]
    pub aircraft_check_remarks: NullableUpdate<String>,

    // 本体 V1（ONTOLOGY_V1.md §4.2）
    pub is_draft: Option<bool>,
    pub divert: Option<bool>,
    #[serde(default)]
    pub flight_kind: NullableUpdate<String>,
    #[serde(default)]
    pub direction: NullableUpdate<String>,
}

#[cfg(test)]
mod tests {
    use super::{FlightUpdate, NullableUpdate};

    #[test]
    fn flight_update_distinguishes_absent_null_and_value_for_nullable_fields() {
        let absent: FlightUpdate = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(matches!(absent.gate, NullableUpdate::Unset));
        assert!(matches!(absent.scheduled_departure, NullableUpdate::Unset));

        let clear: FlightUpdate = serde_json::from_value(serde_json::json!({
            "gate": null,
            "scheduled_departure": null,
            "inbound_leg": null
        }))
        .unwrap();
        assert!(matches!(clear.gate, NullableUpdate::Clear));
        assert!(matches!(clear.scheduled_departure, NullableUpdate::Clear));
        assert!(matches!(clear.inbound_leg, NullableUpdate::Clear));

        let value: FlightUpdate = serde_json::from_value(serde_json::json!({
            "gate": "G12",
            "scheduled_departure": "2026-04-27T08:30:00Z"
        }))
        .unwrap();
        assert!(matches!(value.gate, NullableUpdate::Set(ref gate) if gate == "G12"));
        assert!(matches!(value.scheduled_departure, NullableUpdate::Set(_)));
    }
}

// ---------------------------------------------------------------------------
// 响应
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightResponse {
    pub flight_id: Option<String>,
    pub flight_number: Option<String>,
    pub airline_code: Option<String>,
    pub registration: Option<String>,
    pub aircraft_type_detail: Option<String>,
    pub status: Option<String>,

    pub scheduled_departure: Option<DateTime<Utc>>,
    pub scheduled_arrival: Option<DateTime<Utc>>,
    pub estimated_departure: Option<DateTime<Utc>>,
    pub estimated_arrival: Option<DateTime<Utc>>,
    pub actual_departure: Option<DateTime<Utc>>,
    pub actual_arrival: Option<DateTime<Utc>>,
    pub cobt_time: Option<DateTime<Utc>>,
    pub codt: Option<DateTime<Utc>>,
    pub on_blocks_time: Option<DateTime<Utc>>,
    pub cabin_door_open_time: Option<DateTime<Utc>>,
    pub deboarding_complete_time: Option<DateTime<Utc>>,
    pub cleaning_start_time: Option<DateTime<Utc>>,
    pub cleaning_end_time: Option<DateTime<Utc>>,
    pub boarding_allowed_time: Option<DateTime<Utc>>,
    pub start_boarding_time: Option<DateTime<Utc>>,
    pub passenger_ready_time: Option<DateTime<Utc>>,
    pub end_boarding_time: Option<DateTime<Utc>>,
    pub cabin_door_close_time: Option<DateTime<Utc>>,
    pub cargo_door_close_time: Option<DateTime<Utc>>,
    pub loading_complete_time: Option<DateTime<Utc>>,
    pub off_blocks_time: Option<DateTime<Utc>>,

    pub stand: Option<String>,
    pub gate: Option<String>,
    pub terminal: Option<String>,
    pub position: Option<String>,
    pub baggage_carousel: Option<String>,

    #[serde(default)]
    pub has_boarding_restriction: bool,
    #[serde(default)]
    pub is_quick_turnaround: bool,
    #[serde(default = "default_true")]
    pub is_commercial_signed: bool,

    pub inbound_leg: Option<FlightLegPayload>,
    pub outbound_leg: Option<FlightLegPayload>,
    #[serde(default)]
    pub anomaly_summary: FlightAnomalySummary,
    #[serde(default)]
    pub business_cases: Vec<Value>,

    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub version: i32,
    #[serde(default)]
    pub labels: Vec<String>,
    pub flight_remarks: Option<String>,
    pub load_planning_remarks: Option<String>,
    pub aircraft_maintenance_remarks: Option<String>,
    pub aircraft_check_remarks: Option<String>,
    pub direction: Option<String>,
    #[serde(default)]
    pub flight_kind: Option<String>,
    #[serde(default)]
    pub is_draft: Option<bool>,
    #[serde(default)]
    pub divert: Option<bool>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_score: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_reasons: Option<Vec<FlightRiskReason>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_primary_action: Option<FlightNextAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_freshness: Option<FlightDataFreshness>,
}

// ---------------------------------------------------------------------------
// 列表响应
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightListResponse {
    pub items: Vec<FlightResponse>,
    pub total: i64,
    pub page: i64,
    pub size: i64,
    pub pages: i64,
}

// ---------------------------------------------------------------------------
// 调度时间线
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchTimelineEventCreate {
    pub milestone_code: String,
    pub occurred_at: DateTime<Utc>,
    pub leg_type: Option<String>,
    pub recorded_by: Option<String>,
    #[serde(default)]
    pub client_action_id: Option<String>,
    #[serde(default = "default_manual_source")]
    pub source: String,
    #[serde(default)]
    pub payload: Value,
}

fn default_manual_source() -> String {
    "manual".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchTimelineEventResponse {
    pub timeline_id: String,
    pub flight_id: String,
    pub milestone_code: String,
    pub occurred_at: DateTime<Utc>,
    pub leg_type: Option<String>,
    pub recorded_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_action_id: Option<String>,
    pub source: String,
    #[serde(default)]
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchTimelineListResponse {
    #[serde(default)]
    pub items: Vec<DispatchTimelineEventResponse>,
}

// ---------------------------------------------------------------------------
// 批量单元格更新 (batch-cells)
// ---------------------------------------------------------------------------

/// Controlled set of fields writable via `PATCH /api/v2/flights/batch-cells`.
/// Free-string field writes are rejected by serde (deny unknown / enum only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightBatchEditableField {
    ScheduledDeparture,
    ScheduledArrival,
    CobtTime,
    FlightRemarks,
    BoardingAllowedTime,
    StartBoardingTime,
    EndBoardingTime,
    OnBlocksTime,
    OffBlocksTime,
}

impl FlightBatchEditableField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScheduledDeparture => "scheduled_departure",
            Self::ScheduledArrival => "scheduled_arrival",
            Self::CobtTime => "cobt_time",
            Self::FlightRemarks => "flight_remarks",
            Self::BoardingAllowedTime => "boarding_allowed_time",
            Self::StartBoardingTime => "start_boarding_time",
            Self::EndBoardingTime => "end_boarding_time",
            Self::OnBlocksTime => "on_blocks_time",
            Self::OffBlocksTime => "off_blocks_time",
        }
    }

    pub fn is_snapshot(self) -> bool {
        matches!(
            self,
            Self::ScheduledDeparture | Self::ScheduledArrival | Self::CobtTime | Self::FlightRemarks
        )
    }

    pub fn is_timeline(self) -> bool {
        !self.is_snapshot()
    }

    /// Sync-locked fields require admin or `*` permission (same policy as single-flight PATCH).
    pub fn is_sync_locked(self) -> bool {
        matches!(
            self,
            Self::ScheduledDeparture | Self::ScheduledArrival | Self::CobtTime
        )
    }

    /// Default leg_type for timeline milestones (aligned with frontend DISPATCH_TIMELINE_FIELD_META).
    pub fn timeline_leg_type(self) -> Option<&'static str> {
        match self {
            Self::OnBlocksTime => Some("inbound"),
            Self::BoardingAllowedTime | Self::StartBoardingTime | Self::EndBoardingTime | Self::OffBlocksTime => {
                Some("outbound")
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightBatchCellTarget {
    pub flight_id: String,
    #[serde(default)]
    pub expected_version: Option<i32>,
    /// Required optimistic check against the current cell value.
    /// JSON `null` explicitly means that the selected cell was empty.
    pub expected_value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightBatchCellUpdateRequest {
    pub field: FlightBatchEditableField,
    /// Shared value applied to every target. `null` clears snapshot fields;
    /// timeline fields require a concrete datetime.
    pub value: Value,
    /// Client-supplied batch idempotency key (becomes batch_id when non-empty).
    #[serde(default)]
    pub client_action_id: Option<String>,
    pub targets: Vec<FlightBatchCellTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightBatchCellResultItem {
    pub flight_id: String,
    pub version: i32,
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightBatchCellUpdateResponse {
    pub batch_id: String,
    pub field: String,
    pub updated_count: usize,
    pub results: Vec<FlightBatchCellResultItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightBatchCellConflictItem {
    pub flight_id: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_version: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_value: Option<Value>,
}

#[cfg(test)]
mod batch_cells_schema_tests {
    use super::{FlightBatchCellUpdateRequest, FlightBatchEditableField};
    use serde_json::{json, Value};

    #[test]
    fn batch_request_accepts_targets_contract() {
        let req: FlightBatchCellUpdateRequest = serde_json::from_value(json!({
            "field": "flight_remarks",
            "value": "备注A",
            "client_action_id": "BATCH01",
            "targets": [
                {
                    "flight_id": "f1",
                    "expected_version": 3,
                    "expected_value": "旧备注"
                }
            ]
        }))
        .unwrap();
        assert_eq!(req.field, FlightBatchEditableField::FlightRemarks);
        assert_eq!(req.targets.len(), 1);
        assert_eq!(req.targets[0].expected_version, Some(3));
        assert_eq!(req.client_action_id.as_deref(), Some("BATCH01"));
    }

    #[test]
    fn batch_request_preserves_explicit_null_expected_value() {
        let req: FlightBatchCellUpdateRequest = serde_json::from_value(json!({
            "field": "start_boarding_time",
            "value": "2026-07-17T10:00:00Z",
            "targets": [{
                "flight_id": "f1",
                "expected_value": null
            }]
        }))
        .unwrap();

        assert_eq!(req.targets[0].expected_value, Value::Null);
    }

    #[test]
    fn batch_request_rejects_missing_expected_value() {
        let result = serde_json::from_value::<FlightBatchCellUpdateRequest>(json!({
            "field": "start_boarding_time",
            "value": "2026-07-17T10:00:00Z",
            "targets": [{ "flight_id": "f1" }]
        }));

        assert!(result.is_err());
    }

    #[test]
    fn batch_request_rejects_unknown_field() {
        let err = serde_json::from_value::<FlightBatchCellUpdateRequest>(json!({
            "field": "status",
            "value": "1",
            "targets": [{ "flight_id": "f1", "expected_version": 1 }]
        }));
        assert!(err.is_err());
    }

    #[test]
    fn batch_field_enum_snake_case_roundtrip() {
        let fields = [
            "scheduled_departure",
            "cobt_time",
            "boarding_allowed_time",
            "flight_remarks",
        ];
        for field in fields {
            let parsed: FlightBatchEditableField = serde_json::from_value(json!(field)).expect(field);
            assert_eq!(parsed.as_str(), field);
            assert_eq!(serde_json::to_value(parsed).unwrap(), json!(field));
        }
    }
}
