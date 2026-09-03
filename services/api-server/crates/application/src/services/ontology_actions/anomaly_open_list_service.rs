use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::models::anomaly::AnomalyStatus;
use fms_domain::ports::anomaly_repository::AnomalyRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{arg_str, evidence, ANOMALY_LIMIT_DEFAULT};

pub struct AnomalyOpenListService {
    anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
}

impl AnomalyOpenListService {
    pub fn new(anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>) -> Self {
        Self { anomaly_repo }
    }

    pub async fn list(&self, args: &Value) -> Result<Value, OntologyActionError> {
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
        unresolved.sort_by_key(|a| std::cmp::Reverse(a.detected_at));

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
}
