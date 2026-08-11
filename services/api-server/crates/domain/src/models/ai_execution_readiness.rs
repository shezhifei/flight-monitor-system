use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReadinessStatus {
    Ready,
    NotReady,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReadinessCheckStatus {
    Pass,
    Fail,
    Warn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiExecutionReadinessCheck {
    pub name: String,
    pub status: ReadinessCheckStatus,
    pub message: String,
}

impl AiExecutionReadinessCheck {
    pub fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ReadinessCheckStatus::Pass,
            message: message.into(),
        }
    }

    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ReadinessCheckStatus::Fail,
            message: message.into(),
        }
    }

    pub fn warn(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ReadinessCheckStatus::Warn,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiExecutionReadinessReport {
    pub overall_status: ReadinessStatus,
    pub checks: Vec<AiExecutionReadinessCheck>,
    pub generated_at: DateTime<Utc>,
}

impl AiExecutionReadinessReport {
    pub fn from_checks(checks: Vec<AiExecutionReadinessCheck>) -> Self {
        let overall_status = if checks.iter().any(|check| check.status == ReadinessCheckStatus::Fail) {
            ReadinessStatus::NotReady
        } else {
            ReadinessStatus::Ready
        };

        Self {
            overall_status,
            checks,
            generated_at: Utc::now(),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.overall_status == ReadinessStatus::Ready
    }

    pub fn failed_checks(&self) -> Vec<&AiExecutionReadinessCheck> {
        self.checks
            .iter()
            .filter(|check| check.status == ReadinessCheckStatus::Fail)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_is_ready_only_when_all_required_checks_pass() {
        let report = AiExecutionReadinessReport {
            overall_status: ReadinessStatus::Ready,
            checks: vec![
                AiExecutionReadinessCheck::pass("feature_flags", "safe defaults verified"),
                AiExecutionReadinessCheck::pass("database_schema", "required relations present"),
            ],
            generated_at: Utc::now(),
        };

        assert!(report.is_ready());
    }

    #[test]
    fn readiness_is_not_ready_when_any_required_check_fails() {
        let report = AiExecutionReadinessReport {
            overall_status: ReadinessStatus::NotReady,
            checks: vec![AiExecutionReadinessCheck::fail(
                "outbox_health",
                "domain_event_outbox backlog exceeds threshold",
            )],
            generated_at: Utc::now(),
        };

        assert!(!report.is_ready());
        assert!(report.failed_checks().iter().any(|check| check.name == "outbox_health"));
    }

    #[test]
    fn from_checks_sets_not_ready_when_any_fail() {
        let checks = vec![
            AiExecutionReadinessCheck::pass("a", "ok"),
            AiExecutionReadinessCheck::fail("b", "bad"),
        ];
        let report = AiExecutionReadinessReport::from_checks(checks);
        assert!(!report.is_ready());
        assert_eq!(report.failed_checks().len(), 1);
    }

    #[test]
    fn from_checks_sets_ready_when_all_pass() {
        let checks = vec![
            AiExecutionReadinessCheck::pass("a", "ok"),
            AiExecutionReadinessCheck::warn("b", "minor"),
        ];
        let report = AiExecutionReadinessReport::from_checks(checks);
        assert!(report.is_ready());
        assert!(report.failed_checks().is_empty());
    }
}
