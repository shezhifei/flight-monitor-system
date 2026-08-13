use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;

use crate::services::ai_execution_allowlist::ExecutionAllowlist;

use fms_domain::models::ai_execution_readiness::{
    AiExecutionReadinessCheck, AiExecutionReadinessReport, ReadinessCheckStatus,
};
use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;
use fms_domain::ports::ai_proposal_repository::AiProposalRepository;
use fms_domain::ports::database_metadata_port::DatabaseMetadataPort;
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxRepository;

const OUTBOX_BACKLOG_THRESHOLD: i64 = 1000;
const PROPOSAL_FAILURE_RATE_THRESHOLD: i64 = 50;

#[derive(Clone)]
pub struct AiExecutionReadinessService {
    db_metadata_port: Option<Arc<dyn DatabaseMetadataPort + Send + Sync>>,
    outbox_repo: Option<Arc<dyn DomainEventOutboxRepository + Send + Sync>>,
    proposal_repo: Option<Arc<dyn AiProposalRepository + Send + Sync>>,
    ontology_repo: Option<Arc<dyn AiOntologyRepository + Send + Sync>>,
    env_overrides: Arc<RwLock<Vec<(String, String)>>>,
}

impl AiExecutionReadinessService {
    pub fn new(
        db_metadata_port: Option<Arc<dyn DatabaseMetadataPort + Send + Sync>>,
        outbox_repo: Option<Arc<dyn DomainEventOutboxRepository + Send + Sync>>,
    ) -> Self {
        Self {
            db_metadata_port,
            outbox_repo,
            proposal_repo: None,
            ontology_repo: None,
            env_overrides: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn new_for_test() -> Self {
        Self {
            db_metadata_port: None,
            outbox_repo: None,
            proposal_repo: None,
            ontology_repo: None,
            env_overrides: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn with_proposal_repo(mut self, repo: Arc<dyn AiProposalRepository + Send + Sync>) -> Self {
        self.proposal_repo = Some(repo);
        self
    }

    pub fn with_ontology_repo(mut self, repo: Arc<dyn AiOntologyRepository + Send + Sync>) -> Self {
        self.ontology_repo = Some(repo);
        self
    }

    pub async fn with_env_value(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_overrides.write().await.push((key.into(), value.into()));
        self
    }

    pub fn always_not_ready_for_test(failing_check: &str) -> Self {
        let failing = failing_check.to_string();
        Self {
            db_metadata_port: None,
            outbox_repo: None,
            proposal_repo: None,
            ontology_repo: None,
            env_overrides: Arc::new(RwLock::new(vec![("__FAILING_CHECK__".to_string(), failing)])),
        }
    }

    pub async fn evaluate(&self) -> AiExecutionReadinessReport {
        let mut checks = Vec::new();

        checks.push(self.check_feature_flags().await);
        checks.push(self.check_database_schema().await);
        checks.push(self.check_ontology_registry().await);
        checks.push(self.check_outbox_health().await);
        checks.push(self.check_proposal_failure_rate().await);
        checks.push(self.check_sidecar_write_boundary().await);

        if let Some(forced) = self.forced_failing_check().await {
            for check in &mut checks {
                if check.name == forced {
                    check.status = ReadinessCheckStatus::Fail;
                    check.message = "forced failure for testing".to_string();
                }
            }
        }

        AiExecutionReadinessReport::from_checks(checks)
    }

    pub async fn evaluate_static_checks(&self) -> AiExecutionReadinessReport {
        let mut checks = Vec::new();
        checks.push(self.check_feature_flags().await);
        checks.push(self.check_sidecar_write_boundary().await);

        if let Some(forced) = self.forced_failing_check().await {
            for check in &mut checks {
                if check.name == forced {
                    check.status = ReadinessCheckStatus::Fail;
                    check.message = "forced failure for testing".to_string();
                }
            }
        }

        AiExecutionReadinessReport::from_checks(checks)
    }

    async fn forced_failing_check(&self) -> Option<String> {
        let overrides = self.env_overrides.read().await;
        overrides
            .iter()
            .find(|(k, _)| k == "__FAILING_CHECK__")
            .map(|(_, v)| v.clone())
    }

    async fn read_env(&self, key: &str) -> Option<String> {
        let overrides = self.env_overrides.read().await;
        if let Some((_, value)) = overrides.iter().find(|(k, _)| k == key) {
            if value.is_empty() {
                None
            } else {
                Some(value.clone())
            }
        } else {
            std::env::var(key).ok()
        }
    }

    fn is_truthy(value: Option<&str>) -> bool {
        value
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false)
    }

    async fn check_feature_flags(&self) -> AiExecutionReadinessCheck {
        let execution_allowlist = ExecutionAllowlist::parse(
            self.read_env("FMS_AI_PROPOSAL_EXECUTION_ENABLED")
                .await
                .as_deref()
                .unwrap_or_default(),
        );

        if execution_allowlist.requires_readiness_override() {
            let override_value = self.read_env("FMS_AI_EXECUTION_READINESS_OVERRIDE").await;
            let has_staging_override = override_value
                .as_deref()
                .map(|value| value.trim() == "staging")
                .unwrap_or(false);

            if !has_staging_override {
                return AiExecutionReadinessCheck::fail(
                    "feature_flags",
                    format!(
                        "FMS_AI_PROPOSAL_EXECUTION_ENABLED={} requires FMS_AI_EXECUTION_READINESS_OVERRIDE=staging",
                        execution_allowlist.execution_mode()
                    ),
                );
            }

            return AiExecutionReadinessCheck::pass(
                "feature_flags",
                format!(
                    "execution {} enabled with staging override",
                    execution_allowlist.execution_mode()
                ),
            );
        }

        AiExecutionReadinessCheck::pass("feature_flags", "safe defaults verified")
    }

    async fn check_database_schema(&self) -> AiExecutionReadinessCheck {
        let Some(db_metadata_port) = &self.db_metadata_port else {
            return AiExecutionReadinessCheck::warn(
                "database_schema",
                "no database metadata port provided, skipping schema check",
            );
        };

        let required_tables = [
            "ai_action_proposals",
            "ai_run_events",
            "domain_event_outbox",
            "aip_ontology_objects",
            "aip_ontology_actions",
        ];

        for table in &required_tables {
            let qualified = format!("public.{table}");
            match db_metadata_port.relation_exists(&qualified).await {
                Ok(true) => {}
                Ok(false) => {
                    return AiExecutionReadinessCheck::fail(
                        "database_schema",
                        format!("required table '{table}' not found"),
                    );
                }
                Err(error) => {
                    return AiExecutionReadinessCheck::fail(
                        "database_schema",
                        format!("schema check query failed: {error}"),
                    );
                }
            }
        }

        AiExecutionReadinessCheck::pass("database_schema", "required relations present")
    }

    async fn check_ontology_registry(&self) -> AiExecutionReadinessCheck {
        let Some(ontology_repo) = &self.ontology_repo else {
            return AiExecutionReadinessCheck::warn(
                "ontology_registry",
                "no ontology repository provided, skipping ontology check",
            );
        };

        match ontology_repo.count_active_objects().await {
            Ok(count) if count > 0 => match ontology_repo.count_active_write_actions().await {
                Ok(count) if count > 0 => AiExecutionReadinessCheck::pass(
                    "ontology_registry",
                    format!("{count} active write actions registered"),
                ),
                Ok(_) => {
                    AiExecutionReadinessCheck::warn("ontology_registry", "no active write actions found in ontology")
                }
                Err(e) => AiExecutionReadinessCheck::fail(
                    "ontology_registry",
                    format!("failed to query ontology actions: {e}"),
                ),
            },
            Ok(_) => AiExecutionReadinessCheck::warn("ontology_registry", "no active ontology objects found"),
            Err(e) => {
                AiExecutionReadinessCheck::fail("ontology_registry", format!("failed to query ontology objects: {e}"))
            }
        }
    }

    async fn check_outbox_health(&self) -> AiExecutionReadinessCheck {
        let Some(outbox_repo) = &self.outbox_repo else {
            return AiExecutionReadinessCheck::warn(
                "outbox_health",
                "no database pool provided, skipping outbox check",
            );
        };

        match outbox_repo.count_unpublished().await {
            Ok(count) if count > OUTBOX_BACKLOG_THRESHOLD => AiExecutionReadinessCheck::fail(
                "outbox_health",
                format!("domain_event_outbox has {count} unprocessed events (threshold: {OUTBOX_BACKLOG_THRESHOLD})"),
            ),
            Ok(count) => AiExecutionReadinessCheck::pass("outbox_health", format!("{count} unprocessed outbox events")),
            Err(e) => AiExecutionReadinessCheck::fail("outbox_health", format!("failed to query outbox: {e}")),
        }
    }

    async fn check_proposal_failure_rate(&self) -> AiExecutionReadinessCheck {
        let Some(proposal_repo) = &self.proposal_repo else {
            return AiExecutionReadinessCheck::warn(
                "proposal_failure_rate",
                "no proposal repository provided, skipping proposal failure rate check",
            );
        };

        let cutoff = Utc::now() - chrono::Duration::hours(24);
        match proposal_repo.count_failed_since(cutoff).await {
            Ok(count) if count > PROPOSAL_FAILURE_RATE_THRESHOLD => AiExecutionReadinessCheck::fail(
                "proposal_failure_rate",
                format!("{count} proposal failures in last 24h (threshold: {PROPOSAL_FAILURE_RATE_THRESHOLD})"),
            ),
            Ok(count) => AiExecutionReadinessCheck::pass(
                "proposal_failure_rate",
                format!("{count} proposal failures in last 24h"),
            ),
            Err(e) => AiExecutionReadinessCheck::fail(
                "proposal_failure_rate",
                format!("failed to query proposal failures: {e}"),
            ),
        }
    }

    async fn check_sidecar_write_boundary(&self) -> AiExecutionReadinessCheck {
        let sidecar_write_mode = self.read_env("FMS_AI_SIDECAR_WRITE_MODE").await;
        if let Some(mode) = sidecar_write_mode {
            let mode_lower = mode.trim().to_ascii_lowercase();
            if mode_lower != "disabled" && mode_lower != "" && mode_lower != "0" {
                return AiExecutionReadinessCheck::fail(
                    "sidecar_write_boundary",
                    format!("FMS_AI_SIDECAR_WRITE_MODE='{mode}' is not disabled"),
                );
            }
        }

        AiExecutionReadinessCheck::pass("sidecar_write_boundary", "sidecar write boundary intact")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_infrastructure::PgDatabaseMetadataAdapter;
    use sqlx::PgPool;

    fn build_db_metadata_port(pool: &sqlx::PgPool) -> Arc<dyn DatabaseMetadataPort + Send + Sync> {
        Arc::new(PgDatabaseMetadataAdapter::new(pool.clone()))
    }

    #[tokio::test]
    async fn readiness_fails_when_allowlist_enabled_without_staging_override() {
        let service = AiExecutionReadinessService::new_for_test()
            .with_env_value("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "Todo.create")
            .await
            .with_env_value("FMS_AI_EXECUTION_READINESS_OVERRIDE", "")
            .await;

        let report = service.evaluate_static_checks().await;

        assert!(!report.is_ready());
        assert!(report.failed_checks().iter().any(|check| check.name == "feature_flags"));
    }

    #[tokio::test]
    async fn readiness_passes_when_allowlist_enabled_with_staging_override() {
        let service = AiExecutionReadinessService::new_for_test()
            .with_env_value("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "Todo.create")
            .await
            .with_env_value("FMS_AI_EXECUTION_READINESS_OVERRIDE", "staging")
            .await;

        let report = service.evaluate_static_checks().await;

        assert!(report.is_ready());
    }

    #[tokio::test]
    async fn readiness_fails_when_production_execution_flag_is_enabled_without_staging_override() {
        let service = AiExecutionReadinessService::new_for_test()
            .with_env_value("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true")
            .await
            .with_env_value("FMS_AI_EXECUTION_READINESS_OVERRIDE", "")
            .await;

        let report = service.evaluate_static_checks().await;

        assert!(!report.is_ready());
        assert!(report.failed_checks().iter().any(|check| check.name == "feature_flags"));
    }

    #[tokio::test]
    async fn readiness_passes_safe_default_flags() {
        let service = AiExecutionReadinessService::new_for_test()
            .with_env_value("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "false")
            .await;

        let report = service.evaluate_static_checks().await;

        assert!(report.checks.iter().any(|check| check.name == "feature_flags"));
        assert!(report.failed_checks().is_empty());
    }

    #[tokio::test]
    async fn readiness_passes_when_execution_enabled_with_staging_override() {
        let service = AiExecutionReadinessService::new_for_test()
            .with_env_value("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true")
            .await
            .with_env_value("FMS_AI_EXECUTION_READINESS_OVERRIDE", "staging")
            .await;

        let report = service.evaluate_static_checks().await;

        assert!(report.is_ready());
    }

    #[tokio::test]
    async fn sidecar_write_boundary_fails_when_sidecar_write_mode_active() {
        let service = AiExecutionReadinessService::new_for_test()
            .with_env_value("FMS_AI_SIDECAR_WRITE_MODE", "enabled")
            .await;

        let report = service.evaluate_static_checks().await;

        assert!(report
            .failed_checks()
            .iter()
            .any(|check| check.name == "sidecar_write_boundary"));
    }

    #[tokio::test]
    async fn always_not_ready_for_test_forces_failure() {
        let service = AiExecutionReadinessService::always_not_ready_for_test("feature_flags");
        let report = service.evaluate_static_checks().await;
        assert!(!report.is_ready());
    }

    #[tokio::test]
    async fn evaluate_without_pool_warns_on_db_checks() {
        let service = AiExecutionReadinessService::new_for_test();
        let report = service.evaluate().await;

        let db_check = report
            .checks
            .iter()
            .find(|c| c.name == "database_schema")
            .expect("database_schema check present");
        assert_eq!(db_check.status, ReadinessCheckStatus::Warn);
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn readiness_passes_required_schema_on_test_db() {
        let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
        let pool = PgPool::connect(&database_url).await.expect("connect");
        let service = AiExecutionReadinessService::new(Some(build_db_metadata_port(&pool)), None);
        let report = service.evaluate().await;

        assert!(
            !report
                .checks
                .iter()
                .any(|check| check.name == "database_schema" && check.status == ReadinessCheckStatus::Fail),
            "{report:?}"
        );
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn readiness_proposal_failure_rate_check_does_not_fail_on_test_db() {
        let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
        let pool = PgPool::connect(&database_url).await.expect("connect");
        let service = AiExecutionReadinessService::new(Some(build_db_metadata_port(&pool)), None);
        let report = service.evaluate().await;

        let failure_rate_check = report
            .checks
            .iter()
            .find(|c| c.name == "proposal_failure_rate")
            .expect("proposal_failure_rate check present");
        assert_ne!(
            failure_rate_check.status,
            ReadinessCheckStatus::Fail,
            "proposal_failure_rate check must not fail (SQL error or threshold exceeded): {:?}",
            failure_rate_check.message
        );
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn readiness_report_has_no_sql_error_failures_on_test_db() {
        let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
        let pool = PgPool::connect(&database_url).await.expect("connect");
        let service = AiExecutionReadinessService::new(Some(build_db_metadata_port(&pool)), None);
        let report = service.evaluate().await;

        for check in &report.checks {
            assert_ne!(
                check.status,
                ReadinessCheckStatus::Fail,
                "check '{}' failed: {}",
                check.name,
                check.message
            );
        }
    }
}
