use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::ports::dispatch_repository::StandRepository;
use fms_domain::ports::ontology_repository::StandOccupationRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{
    arg_datetime, evidence, required_str, ALTERNATIVE_STAND_CANDIDATES_SCANNED, ALTERNATIVE_STAND_SUGGESTIONS_MAX,
};

pub struct StandAvailabilityService {
    stand_repo: Arc<dyn StandRepository + Send + Sync>,
    stand_occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
}

impl StandAvailabilityService {
    pub fn new(
        stand_repo: Arc<dyn StandRepository + Send + Sync>,
        stand_occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
    ) -> Self {
        Self {
            stand_repo,
            stand_occupation_repo,
        }
    }

    pub async fn check(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let stand_ref = required_str(args, "stand_id")?;
        let window = args.get("time_window").ok_or_else(|| {
            OntologyActionError::InvalidArguments("missing required argument `time_window`".to_string())
        })?;
        let start = arg_datetime(window, "start")?
            .ok_or_else(|| OntologyActionError::InvalidArguments("`time_window.start` is required".to_string()))?;
        let end = arg_datetime(window, "end")?
            .ok_or_else(|| OntologyActionError::InvalidArguments("`time_window.end` is required".to_string()))?;
        if end <= start {
            return Err(OntologyActionError::InvalidArguments(
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
                .ok_or_else(|| OntologyActionError::NotFound(format!("stand {stand_ref}")))?,
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
}
