//! Generates an anomaly escalation proposal (`escalate`).

use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};

use fms_domain::models::anomaly::AnomalySeverity;
use fms_domain::ports::anomaly_repository::AnomalyRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{constraint, required_str, suggestion_envelope};

pub struct AnomalyEscalationAdvisorService {
    anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
}

impl AnomalyEscalationAdvisorService {
    pub fn new(anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>) -> Self {
        Self { anomaly_repo }
    }

    pub async fn suggest(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let anomaly_id = required_str(args, "anomaly_id")?;
        let anomaly = self
            .anomaly_repo
            .find_by_id(anomaly_id)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| OntologyActionError::NotFound(format!("anomaly {anomaly_id}")))?;
        if anomaly.status == fms_domain::models::anomaly::AnomalyStatus::Resolved {
            return Err(OntologyActionError::InvalidArguments(format!(
                "anomaly {anomaly_id} is already resolved"
            )));
        }

        let now = Utc::now();
        let age_minutes = (now - anomaly.detected_at).num_minutes();
        let unacknowledged = anomaly.status == fms_domain::models::anomaly::AnomalyStatus::Open;
        let (escalation_type, severity_after) =
            if anomaly.severity == AnomalySeverity::Critical || (unacknowledged && age_minutes >= 60) {
                ("severity_escalation", "critical")
            } else {
                ("handling_escalation", anomaly.severity.as_ref())
            };

        let constraint_results = vec![
            constraint("anomaly_unresolved", true, "error", None),
            constraint(
                "escalation_needed",
                anomaly.severity == AnomalySeverity::Critical || unacknowledged,
                "warning",
                Some(&format!("age {} min, status {}", age_minutes, anomaly.status.as_ref())),
            ),
        ];

        let reason = format!(
            "{}: anomaly {} ({}) open for {} min",
            escalation_type,
            anomaly_id,
            anomaly.severity.as_ref(),
            age_minutes
        );
        Ok(suggestion_envelope(
            "Anomaly",
            anomaly_id,
            "escalate",
            json!({ "reason": reason }),
            "medium",
            constraint_results,
            json!({
                "severity": anomaly.severity.as_ref(),
                "status": anomaly.status.as_ref(),
                "escalation_level": anomaly.escalation_level,
            }),
            json!({
                "escalation_level": anomaly.escalation_level + 1,
                "severity": severity_after,
            }),
            0.8,
            &reason,
            json!({
                "escalation_type": escalation_type,
                "targets": {
                    "notification": {
                        "action_name": "send",
                        "title": format!("[{}] {}", escalation_type, anomaly.title),
                        "body": reason,
                    },
                    "todo": anomaly.linked_todo_id,
                },
            }),
        ))
    }
}
