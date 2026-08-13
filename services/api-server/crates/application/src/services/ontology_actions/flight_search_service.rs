use std::sync::Arc;

use chrono::NaiveDate;
use serde_json::{json, Value};

use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ports::flight_repository::{FlightRepository, FlightSearchCriteria};

use super::error::{repo_err, OntologyActionError};
use super::support::{arg_str, evidence, SEARCH_LIMIT_DEFAULT, SEARCH_LIMIT_MAX};

pub struct FlightSearchService {
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
}

impl FlightSearchService {
    pub fn new(flight_repo: Arc<dyn FlightRepository + Send + Sync>) -> Self {
        Self { flight_repo }
    }

    pub async fn search(&self, args: &Value) -> Result<Value, OntologyActionError> {
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
                    .map_err(|_| OntologyActionError::InvalidArguments("`date` must be YYYY-MM-DD".to_string()))?;
                let day_flights = self.flight_repo.find_by_date(date).await.map_err(repo_err)?;
                day_flights
                    .into_iter()
                    .filter(|flight| matches_search_filters(flight, args))
                    .skip(offset as usize)
                    .take(limit as usize)
                    .collect::<Vec<_>>()
            }
            None => {
                let criteria = FlightSearchCriteria {
                    flight_no: arg_str(args, "flight_no").map(str::to_string),
                    status: arg_str(args, "status").map(str::to_string),
                    origin: arg_str(args, "origin").map(str::to_string),
                    destination: arg_str(args, "destination").map(str::to_string),
                    has_open_anomaly: args.get("has_open_anomaly").and_then(Value::as_bool),
                };
                self.flight_repo
                    .search(&criteria, limit, offset)
                    .await
                    .map_err(repo_err)?
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
}

fn matches_search_filters(flight: &fms_domain::models::flight::Flight, args: &Value) -> bool {
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
        if !flight
            .get_origin_codes()
            .iter()
            .any(|code| code.eq_ignore_ascii_case(origin))
        {
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
