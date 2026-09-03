use crate::schemas::flight_schemas::{FlightAnomalySummary, FlightLegPayload, FlightResponse, RouteStationPayload};
use chrono::NaiveDate;
use fms_domain::error::DomainError;
use fms_domain::models::flight::Flight;
use fms_domain::models::flight_monitor_row::FlightMonitorRow;
use fms_domain::ports::flight_monitor_row_repository::FlightMonitorRowRepository;
use std::sync::Arc;

pub struct FlightMonitorRowService<R: FlightMonitorRowRepository + ?Sized> {
    repo: Arc<R>,
}
impl<R: FlightMonitorRowRepository + ?Sized> FlightMonitorRowService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }
    pub async fn list(
        &self,
        date: Option<NaiveDate>,
        query: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<FlightMonitorRow>, i64), DomainError> {
        let rows = self.repo.list(date, query, limit, offset).await?;
        let total = self.repo.count(date, query).await?;
        Ok((rows, total))
    }

    pub fn project_from_flight(flight: &Flight) -> FlightMonitorRow {
        // Once a directional identity exists, only that side is projected.
        // The legacy aggregate fallback remains solely for pre-F4 rows with
        // no direction and both compatibility legs.
        let (inbound, outbound) = match flight.direction.as_deref() {
            Some("inbound") => (flight.inbound_leg.as_ref(), None),
            Some("outbound") => (None, flight.outbound_leg.as_ref()),
            _ => (flight.inbound_leg.as_ref(), flight.outbound_leg.as_ref()),
        };
        let is_inbound = flight.direction.as_deref() == Some("inbound");
        let is_outbound = flight.direction.as_deref() == Some("outbound");
        let sort_time = inbound
            .and_then(|l| l.scheduled_time)
            .or_else(|| outbound.and_then(|l| l.scheduled_time))
            .or(flight.scheduled_departure)
            .or(flight.scheduled_arrival);
        FlightMonitorRow {
            row_id: flight.flight_id.to_string(),
            link_id: None,
            kind: if inbound.is_some() && outbound.is_some() {
                "turnaround"
            } else {
                "single"
            }
            .into(),
            inbound_flight_id: inbound.map(|_| flight.flight_id.to_string()),
            outbound_flight_id: outbound.map(|_| flight.flight_id.to_string()),
            inbound_flight_no: inbound.map(|l| l.flight_no.clone()).or_else(|| {
                (flight.direction.as_deref() == Some("inbound"))
                    .then(|| flight.flight_number.as_ref().map(ToString::to_string))
                    .flatten()
            }),
            outbound_flight_no: outbound.map(|l| l.flight_no.clone()).or_else(|| {
                (flight.direction.as_deref() == Some("outbound"))
                    .then(|| flight.flight_number.as_ref().map(ToString::to_string))
                    .flatten()
            }),
            inbound_scheduled_at: inbound
                .and_then(|l| l.scheduled_time)
                .or_else(|| is_inbound.then_some(flight.scheduled_arrival).flatten()),
            outbound_scheduled_at: outbound
                .and_then(|l| l.scheduled_time)
                .or_else(|| is_outbound.then_some(flight.scheduled_departure).flatten()),
            inbound_estimated_at: if is_outbound { None } else { flight.estimated_arrival },
            outbound_estimated_at: if is_inbound { None } else { flight.estimated_departure },
            inbound_actual_at: if is_outbound { None } else { flight.actual_arrival },
            outbound_actual_at: if is_inbound { None } else { flight.actual_departure },
            inbound_station_code: inbound.and_then(|l| l.destination_code.clone()),
            outbound_station_code: outbound.and_then(|l| l.origin_code.clone()),
            inbound_is_vip: inbound.map(|l| l.is_vip).unwrap_or(false),
            outbound_is_vip: outbound.map(|l| l.is_vip).unwrap_or(false),
            registration: flight.registration.clone(),
            aircraft_type: flight.aircraft_type_detail.as_ref().map(|x| x.to_string()),
            stand_code: flight.stand.as_ref().map(|x| x.to_string()),
            gate_code: flight.gate.as_ref().map(|x| x.to_string()),
            terminal_code: flight.terminal.clone(),
            baggage_carousel_code: flight.baggage_carousel.clone(),
            status: Some(flight.status.to_string()),
            workspace_date: sort_time.map(|v| v.date_naive()),
            sort_time,
            has_open_anomaly: flight
                .anomaly_summary
                .values()
                .any(|v| v.get("has_open_anomaly").and_then(|x| x.as_bool()).unwrap_or(false)),
            version: flight.version,
            updated_at: Some(flight.updated_at),
        }
    }

    /// 将宽表行映射为旧监控 DTO。该映射只使用宽表列，不回读 flights/flight_legs。
    pub fn to_response(row: &FlightMonitorRow) -> FlightResponse {
        let inbound_leg = row.inbound_flight_no.as_ref().map(|flight_no| FlightLegPayload {
            leg_type: "inbound".to_string(),
            flight_no: flight_no.clone(),
            flight_type: "domestic".to_string(),
            mission: None,
            origin_stations: Vec::new(),
            destination_stations: row
                .inbound_station_code
                .as_ref()
                .map(|code| {
                    vec![RouteStationPayload {
                        code: code.clone(),
                        name: None,
                    }]
                })
                .unwrap_or_default(),
            origin_code: None,
            destination_code: row.inbound_station_code.clone(),
            origin_name: None,
            destination_name: None,
            is_vip: row.inbound_is_vip,
            stand_type: None,
            scheduled_time: row.inbound_scheduled_at,
        });
        let outbound_leg = row.outbound_flight_no.as_ref().map(|flight_no| FlightLegPayload {
            leg_type: "outbound".to_string(),
            flight_no: flight_no.clone(),
            flight_type: "domestic".to_string(),
            mission: None,
            origin_stations: row
                .outbound_station_code
                .as_ref()
                .map(|code| {
                    vec![RouteStationPayload {
                        code: code.clone(),
                        name: None,
                    }]
                })
                .unwrap_or_default(),
            destination_stations: Vec::new(),
            origin_code: row.outbound_station_code.clone(),
            destination_code: None,
            origin_name: None,
            destination_name: None,
            is_vip: row.outbound_is_vip,
            stand_type: None,
            scheduled_time: row.outbound_scheduled_at,
        });
        let primary_id = row
            // Turnaround merge keeps the inbound monitor row_id stable;
            // preserve that identity when projecting the compatibility DTO.
            .inbound_flight_id
            .clone()
            .or_else(|| row.outbound_flight_id.clone())
            .or_else(|| Some(row.row_id.clone()));
        let direction = match (row.inbound_flight_id.is_some(), row.outbound_flight_id.is_some()) {
            (true, false) => Some("inbound".to_string()),
            (false, true) => Some("outbound".to_string()),
            _ => None,
        };
        FlightResponse {
            flight_id: primary_id,
            // 监控行稳定身份：row_id 永不因建链/拆链而改，是前端选中键。
            // flight_id 只作过渡期兼容（旧详情点击路径），不再是选中键的唯一来源。
            row_id: Some(row.row_id.clone()),
            link_id: row.link_id.clone(),
            kind: Some(row.kind.clone()),
            inbound_flight_id: row.inbound_flight_id.clone(),
            outbound_flight_id: row.outbound_flight_id.clone(),
            flight_number: row.outbound_flight_no.clone().or_else(|| row.inbound_flight_no.clone()),
            airline_code: None,
            registration: row.registration.clone(),
            aircraft_type_detail: row.aircraft_type.clone(),
            status: row.status.clone(),
            scheduled_departure: row.outbound_scheduled_at,
            scheduled_arrival: row.inbound_scheduled_at,
            estimated_departure: row.outbound_estimated_at,
            estimated_arrival: row.inbound_estimated_at,
            actual_departure: row.outbound_actual_at,
            actual_arrival: row.inbound_actual_at,
            cobt_time: None,
            codt: None,
            on_blocks_time: None,
            cabin_door_open_time: None,
            deboarding_complete_time: None,
            cleaning_start_time: None,
            cleaning_end_time: None,
            boarding_allowed_time: None,
            start_boarding_time: None,
            passenger_ready_time: None,
            end_boarding_time: None,
            cabin_door_close_time: None,
            cargo_door_close_time: None,
            loading_complete_time: None,
            off_blocks_time: None,
            stand: row.stand_code.clone(),
            gate: row.gate_code.clone(),
            terminal: row.terminal_code.clone(),
            position: None,
            baggage_carousel: row.baggage_carousel_code.clone(),
            has_boarding_restriction: false,
            is_quick_turnaround: row.kind == "turnaround",
            is_commercial_signed: true,
            inbound_leg,
            outbound_leg,
            anomaly_summary: FlightAnomalySummary {
                has_open_anomaly: row.has_open_anomaly,
                open_count: i32::from(row.has_open_anomaly),
                acknowledged_count: 0,
            },
            business_cases: Vec::new(),
            created_at: None,
            updated_at: row.updated_at,
            version: row.version,
            labels: Vec::new(),
            flight_remarks: None,
            load_planning_remarks: None,
            aircraft_maintenance_remarks: None,
            aircraft_check_remarks: None,
            direction,
            flight_kind: Some("passenger".to_string()),
            is_draft: Some(false),
            divert: Some(false),
            created_by: None,
            updated_by: None,
            risk_score: None,
            risk_level: None,
            risk_reasons: None,
            next_primary_action: None,
            data_freshness: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FlightMonitorRowService;
    use fms_domain::models::flight_monitor_row::FlightMonitorRow;

    #[test]
    fn turnaround_projection_preserves_inbound_stable_identity() {
        // 拆表后形状：过站行 row_id 是旧聚合 id（= 链 id），进/出港是全新方向航班 id。
        // row_id == inbound_flight_id 只在拆表前成立，用该形状断言会假绿。
        let row = FlightMonitorRow {
            row_id: "OLD".into(),
            link_id: Some("OLD".into()),
            kind: "turnaround".into(),
            inbound_flight_id: Some("IN-NEW".into()),
            outbound_flight_id: Some("OUT-NEW".into()),
            inbound_flight_no: Some("CA100".into()),
            outbound_flight_no: Some("CA101".into()),
            inbound_scheduled_at: None,
            outbound_scheduled_at: None,
            inbound_estimated_at: None,
            outbound_estimated_at: None,
            inbound_actual_at: None,
            outbound_actual_at: None,
            inbound_station_code: None,
            outbound_station_code: None,
            inbound_is_vip: false,
            outbound_is_vip: false,
            registration: None,
            aircraft_type: None,
            stand_code: None,
            gate_code: None,
            terminal_code: None,
            baggage_carousel_code: None,
            status: Some("SCHEDULED".into()),
            workspace_date: None,
            sort_time: None,
            has_open_anomaly: false,
            version: 1,
            updated_at: None,
        };

        let response = FlightMonitorRowService::<
            dyn fms_domain::ports::flight_monitor_row_repository::FlightMonitorRowRepository + Send + Sync,
        >::to_response(&row);
        // 选中键：row_id 稳定，不随方向航班 id 漂移。
        assert_eq!(response.row_id.as_deref(), Some("OLD"));
        assert_ne!(response.row_id.as_deref(), response.inbound_flight_id.as_deref());
        assert_ne!(response.row_id.as_deref(), response.outbound_flight_id.as_deref());
        // 方向航班 id：详情/单元格 PATCH 的真实目标。
        assert_eq!(response.inbound_flight_id.as_deref(), Some("IN-NEW"));
        assert_eq!(response.outbound_flight_id.as_deref(), Some("OUT-NEW"));
        assert_eq!(response.link_id.as_deref(), Some("OLD"));
        assert_eq!(response.kind.as_deref(), Some("turnaround"));
        // 过渡期兼容：旧详情路径的 flight_id 仍指向进港方向航班。
        assert_eq!(response.flight_id.as_deref(), Some("IN-NEW"));
        assert_eq!(response.direction.as_deref(), None);
    }

    #[test]
    fn single_outbound_row_keeps_row_id_and_direction() {
        let row = FlightMonitorRow {
            row_id: "OUT-1".into(),
            link_id: None,
            kind: "single".into(),
            inbound_flight_id: None,
            outbound_flight_id: Some("OUT-1".into()),
            inbound_flight_no: None,
            outbound_flight_no: Some("CA101".into()),
            inbound_scheduled_at: None,
            outbound_scheduled_at: None,
            inbound_estimated_at: None,
            outbound_estimated_at: None,
            inbound_actual_at: None,
            outbound_actual_at: None,
            inbound_station_code: None,
            outbound_station_code: None,
            inbound_is_vip: false,
            outbound_is_vip: false,
            registration: None,
            aircraft_type: None,
            stand_code: None,
            gate_code: None,
            terminal_code: None,
            baggage_carousel_code: None,
            status: Some("SCHEDULED".into()),
            workspace_date: None,
            sort_time: None,
            has_open_anomaly: false,
            version: 1,
            updated_at: None,
        };
        let response = FlightMonitorRowService::<
            dyn fms_domain::ports::flight_monitor_row_repository::FlightMonitorRowRepository + Send + Sync,
        >::to_response(&row);
        assert_eq!(response.row_id.as_deref(), Some("OUT-1"));
        assert_eq!(response.flight_id.as_deref(), Some("OUT-1"));
        assert_eq!(response.direction.as_deref(), Some("outbound"));
        assert!(response.inbound_flight_id.is_none());
    }
}
