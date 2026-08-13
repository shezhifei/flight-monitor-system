//! Generates a stand-change proposal (`change_stand`).

use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use fms_domain::ports::dispatch_repository::StandRepository;
use fms_domain::ports::flight_repository::FlightRepository;
use fms_domain::ports::ontology_repository::StandOccupationRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{arg_str, constraint, required_str, suggestion_envelope, CANDIDATE_STANDS_SCANNED};

pub struct StandRecommendationService {
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    stand_repo: Arc<dyn StandRepository + Send + Sync>,
    stand_occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
}

impl StandRecommendationService {
    pub fn new(
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        stand_repo: Arc<dyn StandRepository + Send + Sync>,
        stand_occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
    ) -> Self {
        Self {
            flight_repo,
            stand_repo,
            stand_occupation_repo,
        }
    }

    pub async fn suggest(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let flight_id = required_str(args, "flight_id")?;
        let flight = self
            .flight_repo
            .find_by_id(flight_id)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| OntologyActionError::NotFound(format!("flight {flight_id}")))?;

        let now = Utc::now();
        let window_start = flight
            .actual_arrival
            .or(flight.estimated_arrival)
            .or(flight.scheduled_arrival)
            .unwrap_or(now);
        let window_end = flight
            .actual_departure
            .or(flight.estimated_departure)
            .or(flight.scheduled_departure)
            .unwrap_or(window_start + Duration::hours(2));
        let (window_start, window_end) = if window_end <= window_start {
            (now, now + Duration::hours(2))
        } else {
            (window_start, window_end)
        };

        let current_stand = flight.stand.clone();
        let requested = arg_str(args, "new_stand_id");
        let candidates = self
            .stand_repo
            .find_all(None, false, CANDIDATE_STANDS_SCANNED, 0)
            .await
            .map_err(repo_err)?;

        let (target, overlap_conflicts) = match requested {
            Some(code) => {
                let stand = candidates
                    .iter()
                    .find(|s| s.code == code || s.id == code)
                    .cloned()
                    .ok_or_else(|| OntologyActionError::NotFound(format!("stand {code}")))?;
                let overlaps = self
                    .stand_occupation_repo
                    .list_overlapping(&stand.code, window_start, window_end)
                    .await
                    .map_err(repo_err)?;
                (stand, overlaps)
            }
            None => {
                let mut chosen = None;
                for candidate in &candidates {
                    if current_stand
                        .as_ref()
                        .is_some_and(|s| s.as_str() == candidate.code.as_str())
                    {
                        continue;
                    }
                    let overlaps = self
                        .stand_occupation_repo
                        .list_overlapping(&candidate.code, window_start, window_end)
                        .await
                        .map_err(repo_err)?;
                    if overlaps.is_empty() {
                        chosen = Some((candidate.clone(), overlaps));
                        break;
                    }
                }
                chosen.ok_or_else(|| {
                    OntologyActionError::NotFound("no available stand in scanned candidates".to_string())
                })?
            }
        };

        let mut constraint_results = vec![
            constraint("target_stand_exists", true, "error", None),
            constraint("target_stand_active", target.is_active, "error", None),
        ];
        if overlap_conflicts.is_empty() {
            constraint_results.push(constraint("no_occupation_overlap", true, "warning", None));
        } else {
            constraint_results.push(constraint(
                "no_occupation_overlap",
                false,
                "warning",
                Some(&format!("{} overlapping occupation(s)", overlap_conflicts.len())),
            ));
        }

        let confidence = if overlap_conflicts.is_empty() { 0.9 } else { 0.6 };
        Ok(suggestion_envelope(
            "Flight",
            flight_id,
            "change_stand",
            json!({ "new_stand_id": target.code }),
            "medium",
            constraint_results,
            json!({ "stand": current_stand }),
            json!({ "stand": target.code }),
            confidence,
            &format!(
                "stand {} suggested for flight {} in window {} ~ {}",
                target.code, flight_id, window_start, window_end
            ),
            json!({
                "conflicts": overlap_conflicts.iter().map(|o| json!({
                    "registration": o.registration,
                    "start_time": o.starts_at,
                    "end_time": o.ends_at,
                })).collect::<Vec<_>>(),
            }),
        ))
    }
}
