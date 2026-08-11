//! Ontology V1 只读动作（契约 §3.1；交接 Phase 1）
//!
//! `flight.get_context` / `flight.search` / `dispatch.get_status` /
//! `anomaly.list_open` / `stand.check_availability` / `report.generate_briefing`。
//!
//! 规则（契约 §4.2）：只读动作不创建 pending action，禁止直接 SQL；
//! 每个响应必须携带 `evidence`（检索时间 + ontology version / 查询参数）。

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{json, Value};

use fms_domain::models::anomaly::AnomalyStatus;
use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ontology::schema_export::FLIGHT_OPS_ONTOLOGY_VERSION;
use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_domain::ports::dispatch_repository::{DispatchOrderRepository, StandRepository, TeamRepository};
use fms_domain::ports::flight_repository::{FlightRepository, FlightSearchCriteria};
use fms_domain::ports::ontology_repository::StandOccupationRepository;

const SEARCH_LIMIT_MAX: i64 = 200;
const SEARCH_LIMIT_DEFAULT: i64 = 50;
const ANOMALY_LIMIT_DEFAULT: i64 = 50;
const ALTERNATIVE_STAND_SUGGESTIONS_MAX: usize = 5;
const ALTERNATIVE_STAND_CANDIDATES_SCANNED: i64 = 20;
const BRIEFING_UPCOMING_TASKS_MAX: usize = 10;

/// 动作名（`{object}.{action}` 小写契约命名）→ 所需权限。
pub fn read_action_permission(action_name: &str) -> Option<&'static str> {
    match action_name {
        "flight.get_context" | "flight.search" => Some("flight:read"),
        "dispatch.get_status" => Some("dispatch:read"),
        "anomaly.list_open" => Some("anomaly:read"),
        "stand.check_availability" => Some("flight:read"),
        "report.generate_briefing" => Some("flight:read"),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReadActionError {
    #[error("unknown read action: {0}")]
    UnknownAction(String),
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("repository error: {0}")]
    Repository(String),
    #[error("internal error: {0}")]
    Internal(String),
}

fn repo_err(error: impl std::fmt::Display) -> ReadActionError {
    ReadActionError::Repository(error.to_string())
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).filter(|value| !value.trim().is_empty())
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ReadActionError> {
    arg_str(args, key)
        .ok_or_else(|| ReadActionError::InvalidArguments(format!("missing required argument `{key}`")))
}

fn arg_datetime(args: &Value, key: &str) -> Result<Option<DateTime<Utc>>, ReadActionError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => raw
            .parse::<DateTime<Utc>>()
            .map(Some)
            .map_err(|_| ReadActionError::InvalidArguments(format!("`{key}` is not an RFC3339 datetime"))),
        Some(_) => Err(ReadActionError::InvalidArguments(format!(
            "`{key}` must be an RFC3339 datetime string"
        ))),
    }
}

fn evidence(query_params: Option<Value>) -> Value {
    let mut evidence = serde_json::Map::new();
    evidence.insert("retrieved_at".to_string(), json!(Utc::now()));
    evidence.insert("ontology_version".to_string(), json!(FLIGHT_OPS_ONTOLOGY_VERSION));
    if let Some(params) = query_params {
        evidence.insert("query_params".to_string(), params);
    }
    Value::Object(evidence)
}

pub struct OntologyReadActionService {
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
    team_repo: Arc<dyn TeamRepository + Send + Sync>,
    stand_repo: Arc<dyn StandRepository + Send + Sync>,
    stand_occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
    business_case_repo: Arc<dyn BusinessCaseRepository + Send + Sync>,
}

impl OntologyReadActionService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        stand_repo: Arc<dyn StandRepository + Send + Sync>,
        stand_occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
        business_case_repo: Arc<dyn BusinessCaseRepository + Send + Sync>,
    ) -> Self {
        Self {
            flight_repo,
            dispatch_repo,
            anomaly_repo,
            team_repo,
            stand_repo,
            stand_occupation_repo,
            business_case_repo,
        }
    }

    pub async fn execute(&self, action_name: &str, arguments: &Value) -> Result<Value, ReadActionError> {
        match action_name {
            "flight.get_context" => self.flight_get_context(arguments).await,
            "flight.search" => self.flight_search(arguments).await,
            "dispatch.get_status" => self.dispatch_get_status(arguments).await,
            "anomaly.list_open" => self.anomaly_list_open(arguments).await,
            "stand.check_availability" => self.stand_check_availability(arguments).await,
            "report.generate_briefing" => self.report_generate_briefing(arguments).await,
            other => Err(ReadActionError::UnknownAction(other.to_string())),
        }
    }

    async fn flight_get_context(&self, args: &Value) -> Result<Value, ReadActionError> {
        let flight_id = required_str(args, "flight_id")?;
        let include: Vec<String> = args
            .get("include_relations")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    "dispatch_orders".to_string(),
                    "anomalies".to_string(),
                    "business_cases".to_string(),
                    "labels".to_string(),
                ]
            });

        let flight = self
            .flight_repo
            .find_by_id(flight_id)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| ReadActionError::NotFound(format!("flight {flight_id}")))?;

        let mut response = json!({
            "flight": flight,
            "labels": flight.labels,
        });
        if include.iter().any(|relation| relation == "dispatch_orders") {
            let orders = self.dispatch_repo.find_by_flight(flight_id).await.map_err(repo_err)?;
            response["dispatch_orders"] = json!(orders);
        }
        if include.iter().any(|relation| relation == "anomalies") {
            let anomalies = self.anomaly_repo.find_by_flight(flight_id).await.map_err(repo_err)?;
            response["anomalies"] = json!(anomalies);
        }
        if include.iter().any(|relation| relation == "business_cases") {
            let cases = self.business_case_repo.find_by_flight(flight_id).await.map_err(repo_err)?;
            response["business_cases"] = json!(cases);
        }
        response["evidence"] = evidence(None);
        Ok(response)
    }

    async fn flight_search(&self, args: &Value) -> Result<Value, ReadActionError> {
        let limit = match args.get("limit").and_then(Value::as_i64) {
            None => SEARCH_LIMIT_DEFAULT,
            Some(value) if value <= 0 => SEARCH_LIMIT_DEFAULT,
            Some(value) => value.min(SEARCH_LIMIT_MAX),
        };
        let offset = args.get("offset").and_then(Value::as_i64).unwrap_or(0).max(0);

        let flights = match arg_str(args, "date") {
            Some(raw) => {
                let date = raw
                    .parse::<NaiveDate>()
                    .map_err(|_| ReadActionError::InvalidArguments("`date` must be YYYY-MM-DD".to_string()))?;
                let day_flights = self.flight_repo.find_by_date(date).await.map_err(repo_err)?;
                let filtered = day_flights
                    .into_iter()
                    .filter(|flight| matches_search_filters(flight, args))
                    .skip(offset as usize)
                    .take(limit as usize)
                    .collect::<Vec<_>>();
                filtered
            }
            None => {
                let criteria = FlightSearchCriteria {
                    flight_no: arg_str(args, "flight_no").map(str::to_string),
                    status: arg_str(args, "status").map(str::to_string),
                    origin: arg_str(args, "origin").map(str::to_string),
                    destination: arg_str(args, "destination").map(str::to_string),
                    has_open_anomaly: args.get("has_open_anomaly").and_then(Value::as_bool),
                };
                self.flight_repo.search(&criteria, limit, offset).await.map_err(repo_err)?
            }
        };

        let query_params = json!({
            "flight_no": args.get("flight_no"),
            "status": args.get("status"),
            "origin": args.get("origin"),
            "destination": args.get("destination"),
            "date": args.get("date"),
            "has_open_anomaly": args.get("has_open_anomaly"),
            "limit": limit,
            "offset": offset,
        });
        Ok(json!({
            "flights": flights,
            "total": flights.len(),
            "evidence": evidence(Some(query_params)),
        }))
    }

    async fn dispatch_get_status(&self, args: &Value) -> Result<Value, ReadActionError> {
        let order_id = required_str(args, "dispatch_order_id")?;
        let order = self
            .dispatch_repo
            .find_by_id(order_id, true, None)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| ReadActionError::NotFound(format!("dispatch order {order_id}")))?;

        let team = match &order.team_id {
            Some(team_id) => self.team_repo.find_by_id(team_id, true).await.map_err(repo_err)?,
            None => None,
        };

        let mut conflicts = Vec::new();
        if let Some(reason) = &order.conflict_reason {
            conflicts.push(json!({
                "type": "resource_conflict",
                "description": reason,
            }));
        }

        Ok(json!({
            "dispatch_order": order,
            "team": team,
            "equipment": order.equipment_assignment,
            "conflicts": conflicts,
            "evidence": evidence(None),
        }))
    }

    async fn anomaly_list_open(&self, args: &Value) -> Result<Value, ReadActionError> {
        let limit = args
            .get("limit")
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .unwrap_or(ANOMALY_LIMIT_DEFAULT);
        let severity_filter = arg_str(args, "severity");
        let flight_filter = arg_str(args, "flight_id");

        let mut unresolved = Vec::new();
        for status in [AnomalyStatus::Open, AnomalyStatus::Acknowledged] {
            unresolved.extend(self.anomaly_repo.find_by_status(status).await.map_err(repo_err)?);
        }
        unresolved.sort_by(|a, b| b.detected_at.cmp(&a.detected_at));

        let mut summary = json!({"critical": 0, "high": 0, "medium": 0, "low": 0});
        for anomaly in &unresolved {
            let key = anomaly.severity.as_ref();
            if let Some(count) = summary.get(key).and_then(Value::as_i64) {
                summary[key] = json!(count + 1);
            }
        }

        let anomalies: Vec<_> = unresolved
            .into_iter()
            .filter(|anomaly| severity_filter.is_none_or(|severity| anomaly.severity.as_ref() == severity))
            .filter(|anomaly| flight_filter.is_none_or(|flight_id| anomaly.flight_id == flight_id))
            .take(limit as usize)
            .collect();
        let total = anomalies.len();

        Ok(json!({
            "anomalies": anomalies,
            "total": total,
            "summary": summary,
            "evidence": evidence(None),
        }))
    }

    async fn stand_check_availability(&self, args: &Value) -> Result<Value, ReadActionError> {
        let stand_ref = required_str(args, "stand_id")?;
        let window = args
            .get("time_window")
            .ok_or_else(|| ReadActionError::InvalidArguments("missing required argument `time_window`".to_string()))?;
        let start = arg_datetime(window, "start")?
            .ok_or_else(|| ReadActionError::InvalidArguments("`time_window.start` is required".to_string()))?;
        let end = arg_datetime(window, "end")?
            .ok_or_else(|| ReadActionError::InvalidArguments("`time_window.end` is required".to_string()))?;
        if end <= start {
            return Err(ReadActionError::InvalidArguments(
                "`time_window.end` must be after `time_window.start`".to_string(),
            ));
        }

        let stand = match self.stand_repo.find_by_code(stand_ref).await.map_err(repo_err)? {
            Some(stand) => stand,
            None => self
                .stand_repo
                .find_by_id(stand_ref)
                .await
                .map_err(repo_err)?
                .ok_or_else(|| ReadActionError::NotFound(format!("stand {stand_ref}")))?,
        };

        let overlaps = self
            .stand_occupation_repo
            .list_overlapping(&stand.code, start, end)
            .await
            .map_err(repo_err)?;
        let conflicts: Vec<Value> = overlaps
            .iter()
            .map(|occupation| {
                json!({
                    "flight_id": occupation.flight_id,
                    "registration": occupation.registration,
                    "start_time": occupation.starts_at,
                    "end_time": occupation.ends_at,
                    "reason": "stand occupation overlaps requested window",
                })
            })
            .collect();
        let is_available = stand.is_active && conflicts.is_empty();

        let mut alternative_suggestions = Vec::new();
        if !is_available {
            let candidates = self
                .stand_repo
                .find_all(None, false, ALTERNATIVE_STAND_CANDIDATES_SCANNED, 0)
                .await
                .map_err(repo_err)?;
            for candidate in candidates {
                if candidate.code == stand.code || !candidate.is_active {
                    continue;
                }
                let candidate_overlaps = self
                    .stand_occupation_repo
                    .list_overlapping(&candidate.code, start, end)
                    .await
                    .map_err(repo_err)?;
                if candidate_overlaps.is_empty() {
                    alternative_suggestions.push(json!({
                        "stand_id": candidate.code,
                        "score": 1.0,
                    }));
                    if alternative_suggestions.len() >= ALTERNATIVE_STAND_SUGGESTIONS_MAX {
                        break;
                    }
                }
            }
        }

        Ok(json!({
            "stand": stand,
            "is_available": is_available,
            "conflicts": conflicts,
            "alternative_suggestions": alternative_suggestions,
            "evidence": evidence(None),
        }))
    }

    async fn report_generate_briefing(&self, args: &Value) -> Result<Value, ReadActionError> {
        let now = Utc::now();
        let shift_start = arg_datetime(args, "shift_start")?.unwrap_or(now);
        let shift_end = arg_datetime(args, "shift_end")?.unwrap_or(shift_start + chrono::Duration::hours(8));
        if shift_end <= shift_start {
            return Err(ReadActionError::InvalidArguments(
                "`shift_end` must be after `shift_start`".to_string(),
            ));
        }
        let scope = arg_str(args, "scope").unwrap_or("all");
        if !matches!(scope, "all" | "inbound" | "outbound") {
            return Err(ReadActionError::InvalidArguments(
                "`scope` must be one of all|inbound|outbound".to_string(),
            ));
        }
        let department_id = arg_str(args, "department_id");

        // 航班汇总：按覆盖的日期逐日取航班再按窗口过滤（仓储无区间查询）。
        let mut flights = Vec::new();
        let mut day = shift_start.date_naive();
        let last_day = shift_end.date_naive();
        let mut days_scanned = 0i32;
        while day <= last_day && days_scanned < 2 {
            let day_flights = self.flight_repo.find_by_date(day).await.map_err(repo_err)?;
            flights.extend(day_flights);
            day = day.succ_opt().ok_or_else(|| ReadActionError::Internal("date overflow".to_string()))?;
            days_scanned += 1;
        }
        flights.sort_by(|a, b| a.flight_id.as_str().cmp(b.flight_id.as_str()));
        flights.dedup_by(|a, b| a.flight_id == b.flight_id);
        flights.retain(|flight| {
            let in_window = [
                flight.scheduled_departure,
                flight.scheduled_arrival,
                flight.estimated_departure,
                flight.estimated_arrival,
            ]
            .into_iter()
            .flatten()
            .any(|moment| moment >= shift_start && moment <= shift_end);
            let in_scope = match scope {
                "inbound" => flight.inbound_leg.is_some(),
                "outbound" => flight.outbound_leg.is_some(),
                _ => true,
            };
            in_window && in_scope
        });

        let arrivals = flights.iter().filter(|flight| flight.inbound_leg.is_some()).count();
        let departures = flights.iter().filter(|flight| flight.outbound_leg.is_some()).count();
        let cancelled = flights.iter().filter(|flight| flight.status == FlightStatus::Cancelled).count();
        let delayed = flights.iter().filter(|flight| flight.status == FlightStatus::Delayed).count();

        // 派工汇总 + 即将开始任务：使用派工窗口查询。
        let orders = self
            .dispatch_repo
            .find_orders_in_window(shift_start, shift_end, &[], None, department_id, None, true)
            .await
            .map_err(repo_err)?;
        let pending = orders.iter().filter(|order| order.status.as_ref() == "pending").count();
        let in_progress = orders.iter().filter(|order| order.status.as_ref() == "in_progress").count();
        let completed = orders.iter().filter(|order| order.status.as_ref() == "completed").count();

        let mut upcoming: Vec<Value> = orders
            .iter()
            .filter(|order| matches!(order.status.as_ref(), "pending" | "assigned"))
            .filter(|order| order.planned_start_time.is_some_and(|moment| moment >= now))
            .collect::<Vec<_>>()
            .iter()
            .map(|order| {
                json!({
                    "dispatch_order_id": order.id,
                    "task_type": order.task_type,
                    "flight_id": order.flight_id,
                    "planned_time": order.planned_start_time,
                })
            })
            .collect();
        upcoming.sort_by(|a, b| a["planned_time"].to_string().cmp(&b["planned_time"].to_string()));
        upcoming.truncate(BRIEFING_UPCOMING_TASKS_MAX);

        let open_anomalies = self.anomaly_repo.find_by_status(AnomalyStatus::Open).await.map_err(repo_err)?;
        let critical_open = open_anomalies
            .iter()
            .filter(|anomaly| anomaly.severity.as_ref() == "critical")
            .count();

        // 数据缺口显式声明（契约：明确数据缺口和 confidence）。
        let mut limitations = vec![
            "flights_summary 基于按日检索 + 时间窗过滤，跨 2 天以上的班次不完全覆盖",
            "dispatch_summary 未过滤部门以外的组织维度",
        ];
        if flights.is_empty() {
            limitations.push("窗口内未检索到航班，汇总可能反映数据缺口而非零运行量");
        }
        let confidence = if flights.is_empty() { 0.5 } else { 0.9 };

        Ok(json!({
            "briefing": {
                "title": format!("Operations briefing {} ~ {}", shift_start, shift_end),
                "generated_at": now,
                "flights_summary": {
                    "total": flights.len(),
                    "arrivals": arrivals,
                    "departures": departures,
                    "delayed": delayed,
                    "cancelled": cancelled,
                },
                "dispatch_summary": {
                    "total": orders.len(),
                    "pending": pending,
                    "in_progress": in_progress,
                    "completed": completed,
                },
                "anomaly_summary": {
                    "open": open_anomalies.len(),
                    "critical": critical_open,
                },
                "upcoming_tasks": upcoming,
                "checklist": [
                    "确认窗口内 critical 异常已认领",
                    "核对即将开始的派工单资源到位情况",
                    "复核延误/取消航班的机位与登机口占用",
                ],
            },
            "confidence": confidence,
            "limitations": limitations,
            "evidence": evidence(None),
        }))
    }
}

fn matches_search_filters(
    flight: &fms_domain::models::flight::Flight,
    args: &Value,
) -> bool {
    if let Some(flight_no) = arg_str(args, "flight_no") {
        if !flight
            .get_flight_numbers()
            .iter()
            .any(|number| number.eq_ignore_ascii_case(flight_no))
        {
            return false;
        }
    }
    if let Some(status) = arg_str(args, "status") {
        if FlightStatus::from_str_loose(status) != Some(flight.status) {
            return false;
        }
    }
    if let Some(origin) = arg_str(args, "origin") {
        if !flight.get_origin_codes().iter().any(|code| code.eq_ignore_ascii_case(origin)) {
            return false;
        }
    }
    if let Some(destination) = arg_str(args, "destination") {
        if !flight
            .get_destination_codes()
            .iter()
            .any(|code| code.eq_ignore_ascii_case(destination))
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Duration;
    use fms_domain::error::DomainError;
    use fms_domain::models::anomaly::{Anomaly, AnomalySeverity, AnomalyType};
    use fms_domain::models::business_case::FlightBusinessCase;
    use fms_domain::models::dispatch::{DispatchOrder, Stand, Team};
    use fms_domain::models::flight::Flight;
    use fms_domain::models::ontology_v1::{OccupationKind, OccupationStatus, StandOccupation};
    use fms_domain::ports::dispatch_repository::CreateDispatchOrderCommand;

    // ── fake repositories（未用到的方法统一 unimplemented!）──

    #[derive(Default)]
    struct FakeFlightRepo {
        flights: std::sync::Mutex<Vec<Flight>>,
    }

    #[async_trait]
    impl FlightRepository for FakeFlightRepo {
        async fn find_by_id(&self, flight_id: &str) -> Result<Option<Flight>, DomainError> {
            Ok(self.flights.lock().unwrap().iter().find(|f| f.flight_id.as_str() == flight_id).cloned())
        }
        async fn find_all(&self, limit: i64, _offset: i64) -> Result<Vec<Flight>, DomainError> {
            Ok(self.flights.lock().unwrap().iter().take(limit as usize).cloned().collect())
        }
        async fn find_by_date(&self, _date: NaiveDate) -> Result<Vec<Flight>, DomainError> {
            Ok(self.flights.lock().unwrap().clone())
        }
        async fn find_by_flight_number(&self, _flight_no: &str) -> Result<Vec<Flight>, DomainError> {
            unimplemented!()
        }
        async fn find_by_status(&self, _status: i32, _limit: i64, _offset: i64) -> Result<Vec<Flight>, DomainError> {
            unimplemented!()
        }
        async fn save(&self, _flight: &Flight) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn update_partial(
            &self,
            _flight_id: &str,
            _patch: &fms_domain::ports::flight_repository::FlightUpdatePatch,
        ) -> Result<Option<Flight>, DomainError> {
            unimplemented!()
        }
        async fn save_batch(&self, _flights: &[Flight]) -> Result<usize, DomainError> {
            unimplemented!()
        }
        async fn update_status(&self, _flight_id: &str, _status: i32) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn delete(&self, _flight_id: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn search(
            &self,
            _criteria: &FlightSearchCriteria,
            limit: i64,
            _offset: i64,
        ) -> Result<Vec<Flight>, DomainError> {
            Ok(self.flights.lock().unwrap().iter().take(limit as usize).cloned().collect())
        }
        async fn count_by_date(&self, _date: NaiveDate) -> Result<i64, DomainError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct FakeDispatchRepo {
        orders: std::sync::Mutex<Vec<DispatchOrder>>,
    }

    #[async_trait]
    impl DispatchOrderRepository for FakeDispatchRepo {
        async fn save(&self, _order: &DispatchOrder) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn create_order_atomic(&self, _command: CreateDispatchOrderCommand) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn save_orders_atomic(&self, _commands: Vec<CreateDispatchOrderCommand>) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn find_by_id(
            &self,
            id: &str,
            _load_members: bool,
            _department: Option<&str>,
        ) -> Result<Option<DispatchOrder>, DomainError> {
            Ok(self.orders.lock().unwrap().iter().find(|o| o.id == id).cloned())
        }
        async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
            Ok(self
                .orders
                .lock()
                .unwrap()
                .iter()
                .filter(|o| o.flight_id == flight_id)
                .cloned()
                .collect())
        }
        async fn find_by_flight_with_filters(
            &self,
            _flight_id: &str,
            _status: Option<&str>,
            _source: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }
        async fn find_by_team(
            &self,
            _team_id: &str,
            _status: Option<&str>,
            _start_date: Option<DateTime<Utc>>,
            _end_date: Option<DateTime<Utc>>,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }
        async fn find_by_team_filtered(
            &self,
            _team_id: &str,
            _status: Option<&str>,
            _source: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }
        async fn find_by_user(&self, _user_id: &str, _status: Option<&str>) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }
        async fn find_all(
            &self,
            _status: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }
        async fn find_all_filtered(
            &self,
            _status: Option<&str>,
            _source: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }
        async fn find_orders_in_window(
            &self,
            _window_start: DateTime<Utc>,
            _window_end: DateTime<Utc>,
            _statuses: &[&str],
            _source: Option<&str>,
            _department: Option<&str>,
            _terminal: Option<&str>,
            _include_cancelled: bool,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            Ok(self.orders.lock().unwrap().clone())
        }
        async fn find_overlapping_orders(
            &self,
            _window_start: DateTime<Utc>,
            _window_end: DateTime<Utc>,
            _team_id: Option<&str>,
            _individual_user_id: Option<&str>,
            _stand_id: Option<&str>,
            _exclude_order_id: Option<&str>,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }
        async fn find_equipment_conflicts(
            &self,
            _equipment_ids: &[String],
            _window_start: DateTime<Utc>,
            _window_end: DateTime<Utc>,
            _exclude_order_id: Option<&str>,
        ) -> Result<Vec<Value>, DomainError> {
            unimplemented!()
        }
        async fn list_logs(&self, _dispatch_order_id: &str, _limit: i64) -> Result<Vec<Value>, DomainError> {
            unimplemented!()
        }
        async fn find_pending_for_flight(&self, _flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }
        async fn find_publishable_orders(
            &self,
            _as_of: DateTime<Utc>,
            _limit: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }
        async fn update_status(
            &self,
            _id: &str,
            _status: &str,
            _actor_id: Option<&str>,
            _enforce_actor_assignment: bool,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn start_order(&self, _id: &str, _actual_start: DateTime<Utc>, _actor_id: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn complete_order(
            &self,
            _id: &str,
            _actual_end: DateTime<Utc>,
            _actor_id: &str,
            _notes: Option<&str>,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn append_log(
            &self,
            _dispatch_order_id: &str,
            _action: &str,
            _actor_id: Option<&str>,
            _details: Option<Value>,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn append_log_once(
            &self,
            _dispatch_order_id: &str,
            _action: &str,
            _actor_id: Option<&str>,
            _details: Value,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn has_logged_action(
            &self,
            _dispatch_order_id: &str,
            _action: &str,
            _actor_id: Option<&str>,
            _client_action_id: Option<&str>,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn report_estimated_completion(
            &self,
            _id: &str,
            _estimated_time: DateTime<Utc>,
            _actor_id: &str,
            _note: Option<&str>,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn update_planned_times(
            &self,
            _id: &str,
            _planned_start: DateTime<Utc>,
            _planned_end: DateTime<Utc>,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn replace_order_equipment_assignments(&self, _id: &str, _equipment_ids: &[String]) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct FakeAnomalyRepo {
        anomalies: std::sync::Mutex<Vec<Anomaly>>,
    }

    #[async_trait]
    impl AnomalyRepository for FakeAnomalyRepo {
        async fn find_by_id(&self, _anomaly_id: &str) -> Result<Option<Anomaly>, DomainError> {
            unimplemented!()
        }
        async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<Anomaly>, DomainError> {
            Ok(self
                .anomalies
                .lock()
                .unwrap()
                .iter()
                .filter(|a| a.flight_id == flight_id)
                .cloned()
                .collect())
        }
        async fn find_by_status(&self, status: AnomalyStatus) -> Result<Vec<Anomaly>, DomainError> {
            Ok(self
                .anomalies
                .lock()
                .unwrap()
                .iter()
                .filter(|a| a.status == status)
                .cloned()
                .collect())
        }
        async fn list_rules(&self, _enabled_only: bool) -> Result<Vec<fms_domain::models::anomaly::AnomalyRule>, DomainError> {
            unimplemented!()
        }
        async fn get_rule(&self, _rule_id: &str) -> Result<Option<fms_domain::models::anomaly::AnomalyRule>, DomainError> {
            unimplemented!()
        }
        async fn upsert_rule(
            &self,
            _rule: &fms_domain::models::anomaly::AnomalyRule,
        ) -> Result<fms_domain::models::anomaly::AnomalyRule, DomainError> {
            unimplemented!()
        }
        async fn save(&self, _anomaly: &Anomaly) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn update(&self, _anomaly: &Anomaly) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn acknowledge(&self, _anomaly_id: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn resolve(&self, _anomaly_id: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn escalate(&self, _anomaly_id: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct FakeTeamRepo {
        teams: std::sync::Mutex<Vec<Team>>,
    }

    #[async_trait]
    impl TeamRepository for FakeTeamRepo {
        async fn save(&self, _team: &Team) -> Result<Team, DomainError> {
            unimplemented!()
        }
        async fn find_by_id(&self, id: &str, _load_members: bool) -> Result<Option<Team>, DomainError> {
            Ok(self.teams.lock().unwrap().iter().find(|t| t.id == id).cloned())
        }
        async fn find_by_code(&self, _code: &str) -> Result<Option<Team>, DomainError> {
            unimplemented!()
        }
        async fn find_available_for_dispatch(
            &self,
            _team_type_id: Option<&str>,
            _terminal: Option<&str>,
        ) -> Result<Vec<Team>, DomainError> {
            unimplemented!()
        }
        async fn find_all(
            &self,
            _include_inactive: bool,
            _team_type_id: Option<&str>,
            _terminal: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<Team>, DomainError> {
            unimplemented!()
        }
        async fn update_position(&self, _id: &str, _lat: f64, _lng: f64, _stand_id: Option<&str>) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn update_status(&self, _id: &str, _status: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct FakeStandRepo {
        stands: std::sync::Mutex<Vec<Stand>>,
    }

    #[async_trait]
    impl StandRepository for FakeStandRepo {
        async fn save(&self, _stand: &Stand) -> Result<Stand, DomainError> {
            unimplemented!()
        }
        async fn find_by_id(&self, id: &str) -> Result<Option<Stand>, DomainError> {
            Ok(self.stands.lock().unwrap().iter().find(|s| s.id == id).cloned())
        }
        async fn find_by_code(&self, code: &str) -> Result<Option<Stand>, DomainError> {
            Ok(self.stands.lock().unwrap().iter().find(|s| s.code == code).cloned())
        }
        async fn find_all(
            &self,
            _terminal: Option<&str>,
            include_inactive: bool,
            limit: i64,
            _offset: i64,
        ) -> Result<Vec<Stand>, DomainError> {
            Ok(self
                .stands
                .lock()
                .unwrap()
                .iter()
                .filter(|s| include_inactive || s.is_active)
                .take(limit as usize)
                .cloned()
                .collect())
        }
        async fn is_active(&self, _id_or_code: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct FakeOccupationRepo {
        occupations: std::sync::Mutex<Vec<StandOccupation>>,
    }

    #[async_trait]
    impl StandOccupationRepository for FakeOccupationRepo {
        async fn find_by_id(&self, _id: &str) -> Result<Option<StandOccupation>, DomainError> {
            unimplemented!()
        }
        async fn create(&self, _occupation: &StandOccupation) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn update(&self, _occupation: &StandOccupation) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn release(&self, _id: &str, _released_by: &str) -> Result<Option<StandOccupation>, DomainError> {
            unimplemented!()
        }
        async fn find_active_by_registration(
            &self,
            _registration: &str,
            _now: DateTime<Utc>,
        ) -> Result<Option<StandOccupation>, DomainError> {
            unimplemented!()
        }
        async fn find_active_by_flight(&self, _flight_id: &str) -> Result<Vec<StandOccupation>, DomainError> {
            unimplemented!()
        }
        async fn list_by_registration(
            &self,
            _registration: &str,
            _limit: i64,
        ) -> Result<Vec<StandOccupation>, DomainError> {
            unimplemented!()
        }
        async fn list_overlapping(
            &self,
            stand_code: &str,
            starts_at: DateTime<Utc>,
            ends_at: DateTime<Utc>,
        ) -> Result<Vec<StandOccupation>, DomainError> {
            Ok(self
                .occupations
                .lock()
                .unwrap()
                .iter()
                .filter(|o| o.stand_code.as_str() == stand_code && o.starts_at < ends_at && o.ends_at > starts_at)
                .cloned()
                .collect())
        }
        async fn list_active_by_registration(
            &self,
            _registration: &str,
            _now: DateTime<Utc>,
        ) -> Result<Vec<StandOccupation>, DomainError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct FakeBusinessCaseRepo;

    #[async_trait]
    impl BusinessCaseRepository for FakeBusinessCaseRepo {
        async fn save(&self, _case: &FlightBusinessCase) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn find_by_id(&self, _case_id: &str) -> Result<Option<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn find_by_id_scoped(
            &self,
            _case_id: &str,
            _viewer_department_id: Option<&str>,
            _viewer_department_name: Option<&str>,
            _include_common: bool,
        ) -> Result<Option<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn find_by_flight(&self, _flight_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
            Ok(vec![])
        }
        async fn find_by_flight_scoped(
            &self,
            _flight_id: &str,
            _viewer_department_id: Option<&str>,
            _viewer_department_name: Option<&str>,
            _include_common: bool,
        ) -> Result<Vec<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn find_by_flight_ids(&self, _flight_ids: &[String]) -> Result<Vec<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn find_by_copilot_batch_action(
            &self,
            _batch_id: &str,
            _action_id: &str,
        ) -> Result<Option<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn list_by_copilot_batch(&self, _batch_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn find_by_flight_ids_scoped(
            &self,
            _flight_ids: &[String],
            _viewer_department_id: Option<&str>,
            _viewer_department_name: Option<&str>,
            _include_common: bool,
        ) -> Result<Vec<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn find_all(
            &self,
            _status: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn find_all_scoped(
            &self,
            _status: Option<&str>,
            _limit: i64,
            _offset: i64,
            _viewer_department_id: Option<&str>,
            _viewer_department_name: Option<&str>,
            _include_common: bool,
        ) -> Result<Vec<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn find_filtered(
            &self,
            _flight_id: Option<&str>,
            _case_type: Option<&str>,
            _status: Option<&str>,
            _limit: Option<i64>,
            _offset: Option<i64>,
        ) -> Result<Vec<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn find_filtered_scoped(
            &self,
            _flight_id: Option<&str>,
            _case_type: Option<&str>,
            _status: Option<&str>,
            _viewer_department_id: Option<&str>,
            _viewer_department_name: Option<&str>,
            _include_common: bool,
            _limit: Option<i64>,
            _offset: Option<i64>,
        ) -> Result<Vec<FlightBusinessCase>, DomainError> {
            unimplemented!()
        }
        async fn update_case(&self, _case: &FlightBusinessCase) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn update_status(&self, _case_id: &str, _status: &str, _actor: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn insert_append(
            &self,
            _append: &fms_domain::models::business_case::BusinessCaseAppendEntry,
        ) -> Result<fms_domain::models::business_case::BusinessCaseAppendEntry, DomainError> {
            unimplemented!()
        }
        async fn insert_append_once(
            &self,
            _append: &fms_domain::models::business_case::BusinessCaseAppendEntry,
        ) -> Result<(fms_domain::models::business_case::BusinessCaseAppendEntry, bool), DomainError> {
            unimplemented!()
        }
        async fn find_append_by_id(
            &self,
            _append_id: &str,
        ) -> Result<Option<fms_domain::models::business_case::BusinessCaseAppendEntry>, DomainError> {
            unimplemented!()
        }
        async fn update_append_metadata(&self, _append_id: &str, _metadata: Value) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn delete(&self, _case_id: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
    }

    // ── fixtures ──

    fn flight_fixture(id: &str, status: FlightStatus, inbound: bool, outbound: bool) -> Flight {
        let leg = fms_domain::models::flight_leg::FlightLeg {
            leg_type: fms_domain::models::flight_leg::LegType::Inbound,
            flight_no: "CA1234".to_string(),
            flight_type: fms_domain::models::flight_leg::FlightTypeCode::Domestic,
            mission: None,
            origin_code: Some("PEK".to_string()),
            origin_name: None,
            destination_code: Some("SHA".to_string()),
            destination_name: None,
            is_vip: false,
            stand_type: None,
            scheduled_time: Some(Utc::now()),
        };
        Flight {
            flight_id: id.into(),
            airline_code: Some("CA".to_string()),
            flight_number: Some("CA1234".into()),
            registration: Some("B-1234".to_string()),
            aircraft_type_detail: None,
            stand: None,
            gate: None,
            terminal: None,
            position: None,
            baggage_carousel: None,
            scheduled_departure: Some(Utc::now()),
            scheduled_arrival: Some(Utc::now()),
            estimated_departure: None,
            estimated_arrival: None,
            actual_departure: None,
            actual_arrival: None,
            cobt_time: None,
            codt: None,
            has_boarding_restriction: false,
            is_quick_turnaround: false,
            is_commercial_signed: true,
            status,
            inbound_leg: if inbound { Some(leg.clone()) } else { None },
            outbound_leg: if outbound {
                Some(fms_domain::models::flight_leg::FlightLeg {
                    leg_type: fms_domain::models::flight_leg::LegType::Outbound,
                    ..leg
                })
            } else {
                None
            },
            anomaly_summary: Default::default(),
            direction: None,
            flight_kind: "passenger".to_string(),
            is_draft: false,
            divert: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            labels: vec!["vip".to_string()],
            flight_remarks: None,
            load_planning_remarks: None,
            aircraft_maintenance_remarks: None,
            aircraft_check_remarks: None,
        }
    }

    fn anomaly_fixture(id: &str, flight_id: &str, severity: AnomalySeverity, status: AnomalyStatus, minutes_ago: i64) -> Anomaly {
        let now = Utc::now();
        Anomaly {
            anomaly_id: id.to_string(),
            flight_id: flight_id.to_string(),
            anomaly_type: AnomalyType::DispatchIssue,
            severity,
            title: format!("anomaly {id}"),
            description: None,
            status,
            detected_at: now - Duration::minutes(minutes_ago),
            resolved_at: None,
            escalation_level: 0,
            last_escalated_at: None,
            linked_todo_id: None,
            rule_id: None,
            context_data: Default::default(),
            created_at: now,
            updated_at: now,
        }
    }

    fn order_fixture(id: &str, flight_id: &str, status: &str, team_id: Option<&str>) -> DispatchOrder {
        let mut raw = serde_json::json!({
            "id": id,
            "flight_id": flight_id,
            "task_type": "baggage_unload",
            "status": status,
            "planned_start_time": Utc::now() + Duration::minutes(30),
        });
        if let Some(team_id) = team_id {
            raw["team_id"] = serde_json::json!(team_id);
        }
        serde_json::from_value(raw).expect("dispatch order fixture")
    }

    fn stand_fixture(id: &str, code: &str, active: bool) -> Stand {
        Stand {
            id: id.to_string(),
            code: code.to_string(),
            name: None,
            terminal: None,
            area: None,
            position_lat: 0.0,
            position_lng: 0.0,
            stand_type: None,
            size_category: None,
            is_active: active,
            created_at: None,
        }
    }

    fn occupation_fixture(stand_code: &str, registration: &str, start_offset_mins: i64, end_offset_mins: i64) -> StandOccupation {
        let now = Utc::now();
        StandOccupation {
            id: format!("occ-{stand_code}"),
            registration: registration.to_string(),
            stand_code: stand_code.into(),
            starts_at: now + Duration::minutes(start_offset_mins),
            ends_at: now + Duration::minutes(end_offset_mins),
            kind: OccupationKind::Normal,
            moving_to_stand: None,
            flight_id: Some("FL_OTHER".into()),
            status: OccupationStatus::Active,
            created_by: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn service(
        flights: Vec<Flight>,
        orders: Vec<DispatchOrder>,
        anomalies: Vec<Anomaly>,
        teams: Vec<Team>,
        stands: Vec<Stand>,
        occupations: Vec<StandOccupation>,
    ) -> OntologyReadActionService {
        OntologyReadActionService::new(
            Arc::new(FakeFlightRepo {
                flights: std::sync::Mutex::new(flights),
            }),
            Arc::new(FakeDispatchRepo {
                orders: std::sync::Mutex::new(orders),
            }),
            Arc::new(FakeAnomalyRepo {
                anomalies: std::sync::Mutex::new(anomalies),
            }),
            Arc::new(FakeTeamRepo {
                teams: std::sync::Mutex::new(teams),
            }),
            Arc::new(FakeStandRepo {
                stands: std::sync::Mutex::new(stands),
            }),
            Arc::new(FakeOccupationRepo {
                occupations: std::sync::Mutex::new(occupations),
            }),
            Arc::new(FakeBusinessCaseRepo),
        )
    }

    fn empty_service() -> OntologyReadActionService {
        service(vec![], vec![], vec![], vec![], vec![], vec![])
    }

    // ── tests ──

    #[test]
    fn permission_mapping_covers_all_read_actions() {
        assert_eq!(read_action_permission("flight.get_context"), Some("flight:read"));
        assert_eq!(read_action_permission("flight.search"), Some("flight:read"));
        assert_eq!(read_action_permission("dispatch.get_status"), Some("dispatch:read"));
        assert_eq!(read_action_permission("anomaly.list_open"), Some("anomaly:read"));
        assert_eq!(read_action_permission("stand.check_availability"), Some("flight:read"));
        assert_eq!(read_action_permission("report.generate_briefing"), Some("flight:read"));
        assert_eq!(read_action_permission("Flight.change_stand"), None);
    }

    #[tokio::test]
    async fn unknown_action_is_rejected() {
        let err = empty_service()
            .execute("flight.delete", &json!({}))
            .await
            .expect_err("unknown action must fail");
        assert!(matches!(err, ReadActionError::UnknownAction(_)));
    }

    #[tokio::test]
    async fn flight_get_context_returns_relations_and_evidence() {
        let svc = service(
            vec![flight_fixture("FL1", FlightStatus::Scheduled, true, true)],
            vec![order_fixture("ORD1", "FL1", "pending", None)],
            vec![anomaly_fixture("AN1", "FL1", AnomalySeverity::High, AnomalyStatus::Open, 5)],
            vec![],
            vec![],
            vec![],
        );
        let result = svc
            .execute("flight.get_context", &json!({"flight_id": "FL1"}))
            .await
            .expect("get_context");
        assert_eq!(result["flight"]["flight_id"], "FL1");
        assert_eq!(result["dispatch_orders"][0]["id"], "ORD1");
        assert_eq!(result["anomalies"][0]["anomaly_id"], "AN1");
        assert_eq!(result["labels"][0], "vip");
        assert_eq!(result["evidence"]["ontology_version"], FLIGHT_OPS_ONTOLOGY_VERSION);
        assert!(result["evidence"]["retrieved_at"].is_string());
    }

    #[tokio::test]
    async fn flight_get_context_missing_flight_is_not_found() {
        let err = empty_service()
            .execute("flight.get_context", &json!({"flight_id": "MISSING"}))
            .await
            .expect_err("missing flight");
        assert!(matches!(err, ReadActionError::NotFound(_)));
    }

    #[tokio::test]
    async fn flight_get_context_requires_flight_id() {
        let err = empty_service()
            .execute("flight.get_context", &json!({}))
            .await
            .expect_err("missing argument");
        assert!(matches!(err, ReadActionError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn flight_search_filters_by_date_and_status() {
        let svc = service(
            vec![
                flight_fixture("FL1", FlightStatus::Delayed, true, true),
                flight_fixture("FL2", FlightStatus::Scheduled, false, true),
            ],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let result = svc
            .execute(
                "flight.search",
                &json!({"date": Utc::now().format("%Y-%m-%d").to_string(), "status": "delayed"}),
            )
            .await
            .expect("search");
        assert_eq!(result["total"], 1);
        assert_eq!(result["flights"][0]["flight_id"], "FL1");
        assert!(result["evidence"]["query_params"]["status"] == "delayed");
    }

    #[tokio::test]
    async fn flight_search_invalid_date_is_rejected() {
        let err = empty_service()
            .execute("flight.search", &json!({"date": "not-a-date"}))
            .await
            .expect_err("bad date");
        assert!(matches!(err, ReadActionError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn dispatch_get_status_returns_team_and_conflicts() {
        let mut order = order_fixture("ORD1", "FL1", "in_progress", Some("TEAM1"));
        order.conflict_reason = Some("equip conflict".to_string());
        let team = Team {
            id: "TEAM1".to_string(),
            name:"Alpha".to_string(),
            team_type_id: None,
            code: None,
            leader_id: None,
            terminal: None,
            current_status: fms_domain::models::dispatch::TeamStatus::OnDuty,
            current_position_lat: None,
            current_position_lng: None,
            current_stand_id: None,
            last_position_update: None,
            created_at: None,
            updated_at: None,
            is_active: true,
            team_type: None,
            members: vec![],
        };
        let svc = service(vec![], vec![order], vec![], vec![team], vec![], vec![]);
        let result = svc
            .execute("dispatch.get_status", &json!({"dispatch_order_id": "ORD1"}))
            .await
            .expect("get_status");
        assert_eq!(result["dispatch_order"]["status"], "in_progress");
        assert_eq!(result["team"]["name"], "Alpha");
        assert_eq!(result["conflicts"][0]["description"], "equip conflict");
        assert!(result["evidence"].is_object());
    }

    #[tokio::test]
    async fn anomaly_list_open_merges_open_and_acknowledged() {
        let svc = service(
            vec![],
            vec![],
            vec![
                anomaly_fixture("AN1", "FL1", AnomalySeverity::Critical, AnomalyStatus::Open, 5),
                anomaly_fixture("AN2", "FL2", AnomalySeverity::Low, AnomalyStatus::Acknowledged, 60),
                anomaly_fixture("AN3", "FL1", AnomalySeverity::Medium, AnomalyStatus::Resolved, 120),
            ],
            vec![],
            vec![],
            vec![],
        );
        let result = svc.execute("anomaly.list_open", &json!({})).await.expect("list_open");
        assert_eq!(result["total"], 2, "resolved anomalies excluded");
        assert_eq!(result["summary"]["critical"], 1);
        assert_eq!(result["summary"]["low"], 1);
        assert_eq!(result["anomalies"][0]["anomaly_id"], "AN1", "newest first");

        let filtered = svc
            .execute("anomaly.list_open", &json!({"severity": "low", "flight_id": "FL2"}))
            .await
            .expect("filtered");
        assert_eq!(filtered["total"], 1);
        assert_eq!(filtered["anomalies"][0]["anomaly_id"], "AN2");
    }

    #[tokio::test]
    async fn stand_check_availability_detects_conflict_and_suggests_alternatives() {
        let now = Utc::now();
        let svc = service(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![stand_fixture("S1", "101", true), stand_fixture("S2", "102", true)],
            vec![occupation_fixture("101", "B-9999", 0, 60)],
        );
        let result = svc
            .execute(
                "stand.check_availability",
                &json!({
                    "stand_id": "101",
                    "time_window": {
                        "start": (now + Duration::minutes(10)).to_rfc3339(),
                        "end": (now + Duration::minutes(30)).to_rfc3339(),
                    }
                }),
            )
            .await
            .expect("check_availability");
        assert_eq!(result["is_available"], false);
        assert_eq!(result["conflicts"][0]["registration"], "B-9999");
        assert_eq!(result["alternative_suggestions"][0]["stand_id"], "102");
    }

    #[tokio::test]
    async fn stand_check_availability_rejects_invalid_window() {
        let now = Utc::now();
        let svc = service(vec![], vec![], vec![], vec![], vec![stand_fixture("S1", "101", true)], vec![]);
        let err = svc
            .execute(
                "stand.check_availability",
                &json!({
                    "stand_id": "101",
                    "time_window": {
                        "start": (now + Duration::minutes(30)).to_rfc3339(),
                        "end": (now + Duration::minutes(10)).to_rfc3339(),
                    }
                }),
            )
            .await
            .expect_err("inverted window");
        assert!(matches!(err, ReadActionError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn report_generate_briefing_aggregates_and_declares_limitations() {
        let now = Utc::now();
        let svc = service(
            vec![
                flight_fixture("FL1", FlightStatus::Delayed, true, true),
                flight_fixture("FL2", FlightStatus::Cancelled, false, true),
            ],
            vec![order_fixture("ORD1", "FL1", "pending", None)],
            vec![anomaly_fixture("AN1", "FL1", AnomalySeverity::Critical, AnomalyStatus::Open, 5)],
            vec![],
            vec![],
            vec![],
        );
        let result = svc
            .execute(
                "report.generate_briefing",
                &json!({
                    "shift_start": now.to_rfc3339(),
                    "shift_end": (now + Duration::hours(8)).to_rfc3339(),
                }),
            )
            .await
            .expect("briefing");
        assert_eq!(result["briefing"]["flights_summary"]["total"], 2);
        assert_eq!(result["briefing"]["flights_summary"]["delayed"], 1);
        assert_eq!(result["briefing"]["flights_summary"]["cancelled"], 1);
        assert_eq!(result["briefing"]["dispatch_summary"]["pending"], 1);
        assert_eq!(result["briefing"]["anomaly_summary"]["critical"], 1);
        assert!(result["limitations"].as_array().is_some_and(|items| !items.is_empty()));
        assert!(result["confidence"].as_f64().is_some());
        assert_eq!(result["evidence"]["ontology_version"], FLIGHT_OPS_ONTOLOGY_VERSION);
    }

    #[tokio::test]
    async fn report_generate_briefing_rejects_bad_scope() {
        let err = empty_service()
            .execute("report.generate_briefing", &json!({"scope": "sideways"}))
            .await
            .expect_err("bad scope");
        assert!(matches!(err, ReadActionError::InvalidArguments(_)));
    }
}
