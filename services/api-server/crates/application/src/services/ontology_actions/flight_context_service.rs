use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use fms_domain::ports::flight_repository::FlightRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{evidence, required_str};

pub struct FlightContextService {
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
    business_case_repo: Arc<dyn BusinessCaseRepository + Send + Sync>,
}

impl FlightContextService {
    pub fn new(
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
        business_case_repo: Arc<dyn BusinessCaseRepository + Send + Sync>,
    ) -> Self {
        Self {
            flight_repo,
            dispatch_repo,
            anomaly_repo,
            business_case_repo,
        }
    }

    pub async fn get(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let flight_id = required_str(args, "flight_id")?;
        let include: Vec<String> = args
            .get("include_relations")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
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
            .ok_or_else(|| OntologyActionError::NotFound(format!("flight {flight_id}")))?;

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
            let cases = self
                .business_case_repo
                .find_by_flight(flight_id)
                .await
                .map_err(repo_err)?;
            response["business_cases"] = json!(cases);
        }
        response["evidence"] = evidence(None);
        Ok(response)
    }
}
