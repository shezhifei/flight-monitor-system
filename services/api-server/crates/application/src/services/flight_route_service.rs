//! 航班路由应用服务
//!
//! 承载 `crate::api::routes::flights` 中的业务编排、DTO 转换与缓存失效逻辑，
//! 使路由层保持为轻量包装器。

use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};

use fms_domain::error::DomainError;

use crate::schemas::flight_schemas::{
    DispatchTimelineEventCreate, DispatchTimelineEventResponse, FlightCreate, FlightResponse, FlightUpdate,
};
use crate::services::flight_commands::{FlightCreateCommand, FlightUpdateCommand};
use crate::services::flight_runtime_service::FlightRuntimeService;

use crate::types::ConcreteFlightService;

/// 航班路由应用服务。
pub struct FlightRouteService {
    flight_service: Arc<ConcreteFlightService>,
    runtime: Arc<FlightRuntimeService>,
}

impl FlightRouteService {
    pub fn new(flight_service: Arc<ConcreteFlightService>, runtime: Arc<FlightRuntimeService>) -> Self {
        Self {
            flight_service,
            runtime,
        }
    }

    /// 列表查询并返回可用于视图展示的航班数据。
    pub async fn list_flights(
        &self,
        page: i64,
        size: i64,
        has_open_anomaly: Option<bool>,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<(Vec<FlightResponse>, String), DomainError> {
        let result = self.flight_service.list_flights(page, size, has_open_anomaly).await?;
        let items = self
            .runtime
            .enrich_flights_for_viewer(result.items, viewer_department_id, viewer_department_name)
            .await?;
        let message = format!("成功获取 {} 个航班", items.len());
        Ok((items, message))
    }

    /// 搜索并返回可用于视图展示的航班数据。
    pub async fn search_flights(
        &self,
        flight_no: Option<&str>,
        status: Option<&str>,
        origin: Option<&str>,
        destination: Option<&str>,
        has_open_anomaly: Option<bool>,
        page: i64,
        size: i64,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<(Vec<FlightResponse>, String), DomainError> {
        let result = self
            .flight_service
            .search_flights(flight_no, status, origin, destination, has_open_anomaly, page, size)
            .await?;
        let items = self
            .runtime
            .enrich_flights_for_viewer(result, viewer_department_id, viewer_department_name)
            .await?;
        let message = format!("找到 {} 个匹配航班", items.len());
        Ok((items, message))
    }

    /// 计算受控更新字段集合（用于权限检查）。
    pub fn denied_update_fields(&self, dto: &FlightUpdate, is_admin: bool, permissions: &[String]) -> Vec<String> {
        self.flight_service.denied_update_fields(dto, is_admin, permissions)
    }

    /// 获取单个航班的视图展示数据。
    pub async fn get_flight(
        &self,
        flight_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<FlightResponse>, DomainError> {
        match self.flight_service.get_flight(flight_id).await? {
            Some(f) => Ok(Some(
                self.runtime
                    .enrich_flight_for_viewer(f, viewer_department_id, viewer_department_name)
                    .await?,
            )),
            None => Ok(None),
        }
    }

    /// 创建航班并返回富化后的结果以及审计记录。
    pub async fn create_flight(
        &self,
        payload: FlightCreate,
        actor: Option<String>,
    ) -> Result<(FlightResponse, Value), DomainError> {
        let command = FlightCreateCommand::new(payload, actor.clone());
        command
            .validate()
            .map_err(|e| DomainError::ValidationError(e.to_string()))?;
        let flight = self
            .runtime
            .enrich_flight(self.flight_service.execute_create(command).await?)
            .await?;
        let audit = self
            .runtime
            .record_created(actor.as_deref().unwrap_or("System"), &flight)
            .await;
        Ok((flight, audit))
    }

    /// 计算航班更新字段集合。
    pub fn update_changed_fields(dto: &FlightUpdate) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if dto.status.is_some() {
            fields.push("status");
        }
        if dto.gate.is_touched() {
            fields.push("gate");
        }
        if dto.terminal.is_touched() {
            fields.push("terminal");
        }
        if dto.stand.is_touched() {
            fields.push("stand");
        }
        if dto.position.is_touched() {
            fields.push("position");
        }
        if dto.baggage_carousel.is_touched() {
            fields.push("baggage_carousel");
        }
        if dto.scheduled_departure.is_touched() {
            fields.push("scheduled_departure");
        }
        if dto.scheduled_arrival.is_touched() {
            fields.push("scheduled_arrival");
        }
        if dto.estimated_departure.is_touched() {
            fields.push("estimated_departure");
        }
        if dto.estimated_arrival.is_touched() {
            fields.push("estimated_arrival");
        }
        if dto.actual_departure.is_touched() {
            fields.push("actual_departure");
        }
        if dto.actual_arrival.is_touched() {
            fields.push("actual_arrival");
        }
        if dto.cobt_time.is_touched() {
            fields.push("cobt_time");
        }
        if dto.aircraft_type_detail.is_touched() {
            fields.push("aircraft_type_detail");
        }
        if dto.registration.is_touched() {
            fields.push("registration");
        }
        if dto.has_boarding_restriction.is_some() {
            fields.push("has_boarding_restriction");
        }
        if dto.is_quick_turnaround.is_some() {
            fields.push("is_quick_turnaround");
        }
        if dto.is_commercial_signed.is_some() {
            fields.push("is_commercial_signed");
        }
        if dto.inbound_leg.is_touched() {
            fields.push("inbound_leg");
        }
        if dto.outbound_leg.is_touched() {
            fields.push("outbound_leg");
        }
        if dto.flight_remarks.is_touched() {
            fields.push("flight_remarks");
        }
        if dto.load_planning_remarks.is_touched() {
            fields.push("load_planning_remarks");
        }
        if dto.aircraft_maintenance_remarks.is_touched() {
            fields.push("aircraft_maintenance_remarks");
        }
        if dto.aircraft_check_remarks.is_touched() {
            fields.push("aircraft_check_remarks");
        }
        fields
    }

    /// 更新航班并返回富化结果、变更字段、状态是否变更以及审计记录。
    pub async fn update_flight(
        &self,
        flight_id: &str,
        dto: FlightUpdate,
        actor: &str,
    ) -> Result<Option<(FlightResponse, Vec<String>, bool, Value)>, DomainError> {
        let before = self.flight_service.get_flight(flight_id).await?;
        let changed_fields = Self::update_changed_fields(&dto);
        let command = FlightUpdateCommand::build(flight_id.to_string(), dto, Some(actor.to_string()))
            .map_err(|e| DomainError::ValidationError(e.to_string()))?;
        match self.flight_service.execute_update(command).await? {
            Some(flight) => {
                let flight = self.runtime.enrich_flight(flight).await?;
                let status_changed = changed_fields.iter().any(|field| *field == "status");
                let audit_changed_fields: Vec<String> =
                    changed_fields.iter().map(|field| (*field).to_owned()).collect();
                let audit = self
                    .runtime
                    .record_updated(actor, before.as_ref(), &flight, &audit_changed_fields)
                    .await;
                Ok(Some((flight, audit_changed_fields, status_changed, audit)))
            }
            None => Ok(None),
        }
    }

    /// 查询最近航班更新。
    pub async fn recent_updates(&self, minutes: i64, limit: usize) -> Result<(Vec<Value>, String), DomainError> {
        let updates = self.runtime.get_recent_flight_updates(minutes, limit).await?;
        let message = format!("获取到 {} 条最近更新", updates.len());
        Ok((updates, message))
    }

    /// 查询航班历史记录。
    pub async fn get_flight_history(
        &self,
        flight_id: &str,
        page: usize,
        page_size: usize,
    ) -> Result<(Vec<Value>, String), DomainError> {
        let history = self
            .runtime
            .get_flight_update_history(flight_id, page, page_size)
            .await?;
        let message = format!("获取到 {} 条历史记录", history.len());
        Ok((history, message))
    }

    /// 生成航班历史报表。
    pub async fn generate_history_report(
        &self,
        flight_id: &str,
        hours: i64,
        incident_type: Option<&str>,
    ) -> Result<Value, DomainError> {
        self.runtime
            .generate_history_report(flight_id, hours, incident_type)
            .await
    }

    /// 生成航班事件经过。
    pub async fn generate_event_journey(&self, flight_id: &str, hours: i64) -> Result<Value, DomainError> {
        self.runtime.generate_event_journey(flight_id, hours).await
    }

    /// 查询派工时间线。
    pub async fn list_dispatch_timeline(
        &self,
        flight_id: &str,
    ) -> Result<Vec<DispatchTimelineEventResponse>, DomainError> {
        self.runtime.list_dispatch_timeline(flight_id).await
    }

    /// 创建派工时间线事件（写后实时由 outbox subscriber 消费）。
    pub async fn create_dispatch_timeline_event(
        &self,
        flight_id: &str,
        mut payload: DispatchTimelineEventCreate,
        actor: Option<String>,
    ) -> Result<(DispatchTimelineEventResponse, Option<FlightResponse>), DomainError> {
        if payload.recorded_by.is_none() {
            payload.recorded_by = actor;
        }
        let write_result = self.runtime.create_dispatch_timeline_event(flight_id, payload).await?;
        Ok((write_result.event, None))
    }

    /// 删除派工时间线事件。
    pub async fn delete_dispatch_timeline_event(
        &self,
        flight_id: &str,
        timeline_id: &str,
    ) -> Result<bool, DomainError> {
        self.runtime
            .delete_dispatch_timeline_event(flight_id, timeline_id)
            .await
    }

    /// 构建航班更新 patch 负载（用于 SSE/WS 广播）。
    pub fn flight_update_patch_payload<S: AsRef<str>>(flight: &FlightResponse, changed_fields: &[S]) -> Value {
        use serde_json::Map;

        let mut patch = Map::new();
        patch.insert("flight_id".to_string(), json!(flight.flight_id));
        patch.insert("version".to_string(), json!(flight.version));
        patch.insert("updated_at".to_string(), json!(flight.updated_at));

        for field in changed_fields {
            let field = field.as_ref();
            let value = match field {
                "status" => json!(flight.status),
                "gate" => json!(flight.gate),
                "terminal" => json!(flight.terminal),
                "stand" => json!(flight.stand),
                "position" => json!(flight.position),
                "baggage_carousel" => json!(flight.baggage_carousel),
                "scheduled_departure" => json!(flight.scheduled_departure),
                "scheduled_arrival" => json!(flight.scheduled_arrival),
                "estimated_departure" => json!(flight.estimated_departure),
                "estimated_arrival" => json!(flight.estimated_arrival),
                "actual_departure" => json!(flight.actual_departure),
                "actual_arrival" => json!(flight.actual_arrival),
                "cobt_time" => json!(flight.cobt_time),
                "codt" => json!(flight.codt),
                "on_blocks_time" => json!(flight.on_blocks_time),
                "cabin_door_open_time" => json!(flight.cabin_door_open_time),
                "deboarding_complete_time" => json!(flight.deboarding_complete_time),
                "cleaning_start_time" => json!(flight.cleaning_start_time),
                "cleaning_end_time" => json!(flight.cleaning_end_time),
                "boarding_allowed_time" => json!(flight.boarding_allowed_time),
                "start_boarding_time" => json!(flight.start_boarding_time),
                "passenger_ready_time" => json!(flight.passenger_ready_time),
                "end_boarding_time" => json!(flight.end_boarding_time),
                "cabin_door_close_time" => json!(flight.cabin_door_close_time),
                "cargo_door_close_time" => json!(flight.cargo_door_close_time),
                "loading_complete_time" => json!(flight.loading_complete_time),
                "off_blocks_time" => json!(flight.off_blocks_time),
                "aircraft_type_detail" => json!(flight.aircraft_type_detail),
                "registration" => json!(flight.registration),
                "has_boarding_restriction" => json!(flight.has_boarding_restriction),
                "is_quick_turnaround" => json!(flight.is_quick_turnaround),
                "is_commercial_signed" => json!(flight.is_commercial_signed),
                "inbound_leg" => json!(flight.inbound_leg),
                "outbound_leg" => json!(flight.outbound_leg),
                "flight_remarks" => json!(flight.flight_remarks),
                "load_planning_remarks" => json!(flight.load_planning_remarks),
                "aircraft_maintenance_remarks" => json!(flight.aircraft_maintenance_remarks),
                "aircraft_check_remarks" => json!(flight.aircraft_check_remarks),
                _ => continue,
            };
            patch.insert(field.to_string(), value);
        }

        Value::Object(patch)
    }

    /// 构建派工时间线 patch 负载。
    pub fn dispatch_timeline_patch_payload(
        flight: Option<&FlightResponse>,
        event: &DispatchTimelineEventResponse,
    ) -> Value {
        let field = event.milestone_code.trim().to_string();
        if field.is_empty() {
            let mut patch = serde_json::Map::new();
            patch.insert("flight_id".to_string(), json!(event.flight_id));
            return Value::Object(patch);
        }

        let mut patch = flight
            .map(|flight| Self::flight_update_patch_payload(flight, std::slice::from_ref(&field)))
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_else(|| {
                let mut patch = serde_json::Map::new();
                patch.insert("flight_id".to_string(), json!(event.flight_id));
                patch
            });

        patch
            .entry("flight_id".to_string())
            .or_insert_with(|| json!(event.flight_id));
        patch.entry(field).or_insert_with(|| json!(event.occurred_at));
        Value::Object(patch)
    }

    /// 构建派工时间线航班更新广播负载。
    pub fn dispatch_timeline_flight_updated_payload(
        flight_id: &str,
        patch: Value,
        event: &DispatchTimelineEventResponse,
    ) -> Value {
        let mut payload = json!({
            "type": "flight_updated",
            "flight_id": flight_id,
            "changed_fields": [event.milestone_code.clone()],
            "timeline_event": event,
            "timestamp": Utc::now().to_rfc3339(),
        });
        payload["flight"] = patch.clone();
        payload["patch"] = patch;
        payload
    }
}

#[cfg(test)]
mod tests {
    use super::FlightRouteService;
    use crate::schemas::flight_schemas::{DispatchTimelineEventResponse, FlightAnomalySummary, FlightResponse};
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn base_flight() -> FlightResponse {
        FlightResponse {
            flight_id: Some("flight-001".to_string()),
            flight_number: Some("MU1234".to_string()),
            airline_code: None,
            registration: None,
            aircraft_type_detail: None,
            status: Some("boarding".to_string()),
            scheduled_departure: None,
            scheduled_arrival: None,
            estimated_departure: None,
            estimated_arrival: None,
            actual_departure: None,
            actual_arrival: None,
            cobt_time: None,
            codt: None,
            on_blocks_time: None,
            cabin_door_open_time: None,
            deboarding_complete_time: None,
            cleaning_start_time: None,
            cleaning_end_time: None,
            boarding_allowed_time: Some(Utc.with_ymd_and_hms(2026, 4, 27, 8, 30, 0).unwrap()),
            start_boarding_time: None,
            passenger_ready_time: None,
            end_boarding_time: None,
            cabin_door_close_time: None,
            cargo_door_close_time: None,
            loading_complete_time: None,
            off_blocks_time: None,
            stand: None,
            gate: None,
            terminal: None,
            position: None,
            baggage_carousel: None,
            has_boarding_restriction: false,
            is_quick_turnaround: false,
            is_commercial_signed: true,
            inbound_leg: None,
            outbound_leg: None,
            anomaly_summary: FlightAnomalySummary::default(),
            business_cases: Vec::new(),
            created_at: None,
            updated_at: Some(Utc.with_ymd_and_hms(2026, 4, 27, 8, 0, 0).unwrap()),
            version: 7,
            labels: Vec::new(),
            flight_remarks: None,
            load_planning_remarks: None,
            aircraft_maintenance_remarks: None,
            aircraft_check_remarks: None,
            direction: None,
            flight_kind: None,
            is_draft: None,
            divert: None,
            created_by: None,
            updated_by: None,
            risk_score: None,
            risk_level: None,
            risk_reasons: None,
            next_primary_action: None,
            data_freshness: None,
        }
    }

    fn timeline_event(milestone_code: &str) -> DispatchTimelineEventResponse {
        DispatchTimelineEventResponse {
            timeline_id: "timeline-001".to_string(),
            flight_id: "flight-001".to_string(),
            milestone_code: milestone_code.to_string(),
            occurred_at: Utc.with_ymd_and_hms(2026, 4, 27, 8, 30, 0).unwrap(),
            leg_type: Some("outbound".to_string()),
            recorded_by: Some("tester".to_string()),
            client_action_id: Some("action-001".to_string()),
            source: "manual".to_string(),
            payload: json!({}),
            created_at: Utc.with_ymd_and_hms(2026, 4, 27, 8, 31, 0).unwrap(),
        }
    }

    #[test]
    fn dispatch_timeline_patch_includes_milestone_and_version_fields() {
        let flight = base_flight();
        let event = timeline_event("boarding_allowed_time");

        let patch = FlightRouteService::dispatch_timeline_patch_payload(Some(&flight), &event);

        assert_eq!(patch["flight_id"], json!("flight-001"));
        assert_eq!(patch["version"], json!(7));
        assert_eq!(patch["updated_at"], json!("2026-04-27T08:00:00Z"));
        assert_eq!(patch["boarding_allowed_time"], json!("2026-04-27T08:30:00Z"));
        assert!(patch.get("flight_number").is_none());
        assert!(patch.get("status").is_none());
    }

    #[test]
    fn dispatch_timeline_flight_updated_payload_uses_patch_not_full_snapshot() {
        let flight = base_flight();
        let event = timeline_event("boarding_allowed_time");
        let patch = FlightRouteService::dispatch_timeline_patch_payload(Some(&flight), &event);

        let payload = FlightRouteService::dispatch_timeline_flight_updated_payload("flight-001", patch, &event);

        assert_eq!(payload["type"], json!("flight_updated"));
        assert_eq!(payload["flight_id"], json!("flight-001"));
        assert_eq!(payload["changed_fields"], json!(["boarding_allowed_time"]));
        assert_eq!(payload["flight"], payload["patch"]);
        assert_eq!(payload["patch"]["boarding_allowed_time"], json!("2026-04-27T08:30:00Z"));
        assert!(payload["patch"].get("flight_number").is_none());
        assert!(payload["patch"].get("business_cases").is_none());
        assert!(payload.get("flights").is_none());
    }
}
