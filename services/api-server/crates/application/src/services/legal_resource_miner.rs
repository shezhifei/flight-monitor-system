//! Legal resource miner.
//!
//! Reuses qualification repositories as the source of truth for grant validity,
//! instead of duplicating status / time filtering in memory.

use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{DepartmentQualificationLevel, QualificationGrant};
use fms_domain::ports::dispatch_repository::{DepartmentQualificationRepository, QualificationGrantRepository};

pub struct LegalResourceMiner {
    qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
    qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
}

impl LegalResourceMiner {
    pub fn new(
        qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
        qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
    ) -> Self {
        Self {
            qualification_repo,
            qualification_grant_repo,
        }
    }

    pub async fn mine_resources(
        &self,
        department_id: &str,
        qualification_code: &str,
        min_level_code: Option<&str>,
        at_time: DateTime<Utc>,
        user_ids: &[String],
    ) -> Result<Vec<QualificationGrant>, DomainError> {
        let normalized_department_id = require_non_empty(department_id, "department_id")?;
        let normalized_qualification_code = require_non_empty(qualification_code, "qualification_code")?;
        let normalized_min_level_code = normalize_optional_text(min_level_code);

        let levels = self
            .qualification_repo
            .list_levels(&normalized_department_id, Some(&normalized_qualification_code), false)
            .await?;
        let coverage_index = build_level_coverage_index(levels);

        let mut grants = self
            .qualification_grant_repo
            .find_by_department(&normalized_department_id, Some(at_time), user_ids, false)
            .await?
            .into_iter()
            .filter(|grant| {
                grant.department_id.trim() == normalized_department_id
                    && grant.qualification_code.trim() == normalized_qualification_code
            })
            .filter(|grant| {
                level_satisfies_requirement(
                    grant.level_code.trim(),
                    normalized_min_level_code.as_deref(),
                    &coverage_index,
                )
            })
            .collect::<Vec<_>>();

        grants.sort_by(|left, right| {
            left.user_id
                .cmp(&right.user_id)
                .then_with(|| left.qualification_code.cmp(&right.qualification_code))
                .then_with(|| left.level_code.cmp(&right.level_code))
        });
        Ok(grants)
    }
}

fn require_non_empty(value: &str, field_name: &str) -> Result<String, DomainError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(DomainError::ValidationError(format!("{field_name} cannot be empty")));
    }
    Ok(normalized.to_string())
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|item| !item.is_empty()).map(str::to_string)
}

fn build_level_coverage_index(levels: Vec<DepartmentQualificationLevel>) -> HashMap<String, HashSet<String>> {
    levels
        .into_iter()
        .map(|level| {
            let mut covered = level
                .covered_level_codes
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<HashSet<_>>();
            covered.insert(level.level_code.trim().to_string());
            (level.level_code.trim().to_string(), covered)
        })
        .filter(|(level_code, _)| !level_code.is_empty())
        .collect()
}

fn level_satisfies_requirement(
    grant_level_code: &str,
    required_level_code: Option<&str>,
    coverage_index: &HashMap<String, HashSet<String>>,
) -> bool {
    let normalized_grant_level = grant_level_code.trim();
    if normalized_grant_level.is_empty() {
        return false;
    }

    let Some(required_level_code) = required_level_code.map(str::trim).filter(|item| !item.is_empty()) else {
        return true;
    };

    if normalized_grant_level == required_level_code {
        return true;
    }

    coverage_index
        .get(normalized_grant_level)
        .map(|covered| covered.contains(required_level_code))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::LegalResourceMiner;
    use chrono::{TimeZone, Utc};
    use fms_domain::error::DomainError;
    use fms_domain::models::dispatch::{
        DepartmentQualificationCatalog, DepartmentQualificationLevel, QualificationGrant, QualificationGrantStatus,
    };
    use fms_domain::ports::dispatch_repository::{DepartmentQualificationRepository, QualificationGrantRepository};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct MockQualificationRepository {
        levels: Vec<DepartmentQualificationLevel>,
    }

    #[async_trait::async_trait]
    impl DepartmentQualificationRepository for MockQualificationRepository {
        async fn save_catalog(
            &self,
            _catalog: &DepartmentQualificationCatalog,
        ) -> Result<DepartmentQualificationCatalog, DomainError> {
            unreachable!("not used in legal resource miner tests")
        }

        async fn list_catalogs(
            &self,
            _department_id: &str,
            _include_inactive: bool,
        ) -> Result<Vec<DepartmentQualificationCatalog>, DomainError> {
            Ok(Vec::new())
        }

        async fn save_level(
            &self,
            _level: &DepartmentQualificationLevel,
        ) -> Result<DepartmentQualificationLevel, DomainError> {
            unreachable!("not used in legal resource miner tests")
        }

        async fn list_levels(
            &self,
            _department_id: &str,
            _qualification_code: Option<&str>,
            _include_inactive: bool,
        ) -> Result<Vec<DepartmentQualificationLevel>, DomainError> {
            Ok(self.levels.clone())
        }
    }

    struct MockQualificationGrantRepository {
        grants: Vec<QualificationGrant>,
        captured_at_time: Mutex<Option<chrono::DateTime<Utc>>>,
        captured_include_inactive: Mutex<Vec<bool>>,
    }

    #[async_trait::async_trait]
    impl QualificationGrantRepository for MockQualificationGrantRepository {
        async fn save(&self, _grant: &QualificationGrant) -> Result<QualificationGrant, DomainError> {
            unreachable!("not used in legal resource miner tests")
        }

        async fn find_by_department(
            &self,
            _department_id: &str,
            at_time: Option<chrono::DateTime<Utc>>,
            _user_ids: &[String],
            include_inactive: bool,
        ) -> Result<Vec<QualificationGrant>, DomainError> {
            *self.captured_at_time.lock().expect("lock poisoned") = at_time;
            self.captured_include_inactive
                .lock()
                .expect("lock poisoned")
                .push(include_inactive);
            Ok(self.grants.clone())
        }
    }

    fn grant(user_id: &str, qualification_code: &str, level_code: &str) -> QualificationGrant {
        QualificationGrant {
            id: format!("grant-{user_id}-{qualification_code}-{level_code}"),
            user_id: user_id.to_string(),
            department_id: "dept-1".to_string(),
            qualification_code: qualification_code.to_string(),
            level_code: level_code.to_string(),
            valid_from: None,
            valid_to: None,
            status: QualificationGrantStatus::Active,
            source_team_id: None,
            metadata: HashMap::new(),
            created_at: None,
            updated_at: None,
        }
    }

    fn level(level_code: &str, covered_level_codes: &[&str]) -> DepartmentQualificationLevel {
        DepartmentQualificationLevel {
            id: format!("level-{level_code}"),
            department_id: "dept-1".to_string(),
            qualification_code: "ops_license".to_string(),
            level_code: level_code.to_string(),
            level_name: level_code.to_string(),
            level_rank: 0,
            covered_level_codes: covered_level_codes.iter().map(|item| item.to_string()).collect(),
            is_active: true,
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn mine_resources_uses_repository_at_time_and_level_coverage() {
        let grant_repo = Arc::new(MockQualificationGrantRepository {
            grants: vec![
                grant("user-a", "ops_license", "L3"),
                grant("user-b", "ops_license", "L1"),
                grant("user-c", "other_license", "L3"),
            ],
            captured_at_time: Mutex::new(None),
            captured_include_inactive: Mutex::new(Vec::new()),
        });
        let miner = LegalResourceMiner::new(
            Arc::new(MockQualificationRepository {
                levels: vec![level("L1", &[]), level("L2", &["L1"]), level("L3", &["L1", "L2"])],
            }),
            grant_repo.clone(),
        );
        let at_time = Utc.with_ymd_and_hms(2026, 3, 28, 8, 0, 0).unwrap();

        let result = miner
            .mine_resources(
                "dept-1",
                "ops_license",
                Some("L2"),
                at_time,
                &["user-a".to_string(), "user-b".to_string()],
            )
            .await
            .expect("mine resources should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].user_id, "user-a");
        assert_eq!(result[0].level_code, "L3");
        assert_eq!(
            grant_repo
                .captured_at_time
                .lock()
                .expect("lock poisoned")
                .expect("captured time should exist"),
            at_time
        );
    }

    #[tokio::test]
    async fn mine_resources_matches_exact_level_even_without_coverage_metadata() {
        let grant_repo = Arc::new(MockQualificationGrantRepository {
            grants: vec![grant("user-a", "ops_license", "L2")],
            captured_at_time: Mutex::new(None),
            captured_include_inactive: Mutex::new(Vec::new()),
        });
        let miner = LegalResourceMiner::new(
            Arc::new(MockQualificationRepository {
                levels: vec![level("L1", &[])],
            }),
            grant_repo.clone(),
        );
        let at_time = Utc.with_ymd_and_hms(2026, 3, 28, 8, 0, 0).unwrap();

        let result = miner
            .mine_resources("dept-1", "ops_license", Some("L2"), at_time, &[])
            .await
            .expect("mine resources should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].level_code, "L2");
        assert_eq!(
            grant_repo
                .captured_include_inactive
                .lock()
                .expect("lock poisoned")
                .as_slice(),
            &[false]
        );
    }

    #[tokio::test]
    async fn mine_resources_rejects_blank_identifiers() {
        let miner = LegalResourceMiner::new(
            Arc::new(MockQualificationRepository::default()),
            Arc::new(MockQualificationGrantRepository {
                grants: Vec::new(),
                captured_at_time: Mutex::new(None),
                captured_include_inactive: Mutex::new(Vec::new()),
            }),
        );
        let at_time = Utc.with_ymd_and_hms(2026, 3, 28, 8, 0, 0).unwrap();

        let error = miner
            .mine_resources(" ", "ops_license", None, at_time, &[])
            .await
            .expect_err("blank department_id should fail");

        assert!(matches!(error, DomainError::ValidationError(_)));
    }
}
