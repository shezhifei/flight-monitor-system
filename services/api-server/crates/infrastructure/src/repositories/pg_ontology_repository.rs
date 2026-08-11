//! PostgreSQL 本体 V1 仓储实现（ONTOLOGY_V1.md §4）
//!
//! 实现 domain::ports::ontology_repository 中五个聚合接口。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};

use fms_domain::error::DomainError;
use fms_domain::models::ontology_v1::{
    Aircraft, AssignmentStatus, GateAssignment, OccupationKind, OccupationStatus, ResourceAdjustmentSuggestion,
    StandOccupation, SuggestionKind, SuggestionStatus, TurnaroundLink, TurnaroundLinkSource, TurnaroundLinkStatus,
};
use fms_domain::models::value_objects::{FlightId, GateNumber, StandNumber};
use fms_domain::ports::ontology_repository::{
    AircraftRepository, GateAssignmentRepository, OntologyTransactionalRepository,
    ResourceAdjustmentSuggestionRepository, StandOccupationRepository, TurnaroundLinkRepository,
};

// ---------------------------------------------------------------------------
// 事务实现（跨聚合原子写）
// ---------------------------------------------------------------------------

#[async_trait]
impl<'tx> OntologyTransactionalRepository<Transaction<'tx, Postgres>> for PgAircraftRepository {
    async fn upsert_aircraft_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        registration: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO aircraft (registration) VALUES ($1) \
             ON CONFLICT (registration) DO UPDATE SET last_seen_at = NOW()",
        )
        .bind(registration)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn create_occupation_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        occupation: &StandOccupation,
    ) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO stand_occupations ({}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            OCCUPATION_COLUMNS
        ))
        .bind(&occupation.id)
        .bind(&occupation.registration)
        .bind(&occupation.stand_code.0)
        .bind(occupation.starts_at)
        .bind(occupation.ends_at)
        .bind(occupation_kind_str(occupation.kind))
        .bind(occupation.moving_to_stand.as_ref().map(|s| &s.0))
        .bind(occupation.flight_id.as_ref().map(|f| &f.0))
        .bind(occupation_status_str(occupation.status))
        .bind(&occupation.created_by)
        .bind(occupation.created_at)
        .bind(occupation.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update_occupation_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        occupation: &StandOccupation,
    ) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE stand_occupations SET stand_code=$2, starts_at=$3, ends_at=$4, kind=$5, \
             moving_to_stand=$6, status=$7, updated_at=NOW() WHERE id=$1 AND status='active'",
        )
        .bind(&occupation.id)
        .bind(&occupation.stand_code.0)
        .bind(occupation.starts_at)
        .bind(occupation.ends_at)
        .bind(occupation_kind_str(occupation.kind))
        .bind(occupation.moving_to_stand.as_ref().map(|s| &s.0))
        .bind(occupation_status_str(occupation.status))
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        if result.rows_affected() != 1 {
            return Err(DomainError::ConcurrencyConflict(format!(
                "active stand occupation {} changed concurrently",
                occupation.id
            )));
        }
        Ok(())
    }

    async fn release_occupation_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        id: &str,
        released_by: &str,
    ) -> Result<Option<StandOccupation>, DomainError> {
        let row = sqlx::query(
            "UPDATE stand_occupations SET status='released', updated_at=NOW(), created_by=COALESCE(created_by,$2) \
             WHERE id=$1 AND status='active' RETURNING *",
        )
        .bind(id)
        .bind(released_by)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_occupation(&r)))
    }

    async fn create_assignment_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        assignment: &GateAssignment,
    ) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO gate_assignments ({}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            ASSIGNMENT_COLUMNS
        ))
        .bind(&assignment.id)
        .bind(&assignment.registration)
        .bind(&assignment.gate_code.0)
        .bind(assignment.starts_at)
        .bind(assignment.ends_at)
        .bind(assignment.flight_id.as_ref().map(|f| &f.0))
        .bind(assignment_status_str(assignment.status))
        .bind(&assignment.created_by)
        .bind(assignment.created_at)
        .bind(assignment.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update_assignment_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        assignment: &GateAssignment,
    ) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE gate_assignments SET gate_code=$2, starts_at=$3, ends_at=$4, status=$5, \
             updated_at=NOW() WHERE id=$1 AND status='active'",
        )
        .bind(&assignment.id)
        .bind(&assignment.gate_code.0)
        .bind(assignment.starts_at)
        .bind(assignment.ends_at)
        .bind(assignment_status_str(assignment.status))
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        if result.rows_affected() != 1 {
            return Err(DomainError::ConcurrencyConflict(format!(
                "active gate assignment {} changed concurrently",
                assignment.id
            )));
        }
        Ok(())
    }

    async fn release_assignment_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        id: &str,
        released_by: &str,
    ) -> Result<Option<GateAssignment>, DomainError> {
        let row = sqlx::query(
            "UPDATE gate_assignments SET status='released', updated_at=NOW(), created_by=COALESCE(created_by,$2) \
             WHERE id=$1 AND status='active' RETURNING *",
        )
        .bind(id)
        .bind(released_by)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_assignment(&r)))
    }

    async fn create_link_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        link: &TurnaroundLink,
    ) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO turnaround_links ({}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
            LINK_COLUMNS
        ))
        .bind(&link.id)
        .bind(&link.inbound_flight_id.0)
        .bind(&link.outbound_flight_id.0)
        .bind(link_status_str(link.status))
        .bind(link_source_str(link.source))
        .bind(&link.broken_reason)
        .bind(&link.created_by)
        .bind(link.created_at)
        .bind(link.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update_link_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        link: &TurnaroundLink,
    ) -> Result<(), DomainError> {
        sqlx::query("UPDATE turnaround_links SET status=$2, broken_reason=$3, updated_at=NOW() WHERE id=$1")
            .bind(&link.id)
            .bind(link_status_str(link.status))
            .bind(&link.broken_reason)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn create_suggestion_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        suggestion: &ResourceAdjustmentSuggestion,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO resource_adjustment_suggestions \
             (id, flight_id, kind, current_value, suggested_value, status, reason, payload, created_by, decided_by, decided_at, expires_at, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(&suggestion.id)
        .bind(&suggestion.flight_id.0)
        .bind(suggestion_kind_str(suggestion.kind))
        .bind(&suggestion.current_value)
        .bind(&suggestion.suggested_value)
        .bind(suggestion_status_str(suggestion.status))
        .bind(&suggestion.reason)
        .bind(&suggestion.payload)
        .bind(&suggestion.created_by)
        .bind(&suggestion.decided_by)
        .bind(suggestion.decided_at)
        .bind(suggestion.expires_at)
        .bind(suggestion.created_at)
        .bind(suggestion.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update_suggestion_status_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        id: &str,
        status: &str,
        decided_by: Option<&str>,
        decided_at: Option<DateTime<Utc>>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE resource_adjustment_suggestions SET status=$2, decided_by=$3, decided_at=$4, updated_at=NOW() \
             WHERE id=$1",
        )
        .bind(id)
        .bind(status)
        .bind(decided_by)
        .bind(decided_at)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn expire_pending_suggestions_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        flight_id: &str,
        kind: &str,
    ) -> Result<usize, DomainError> {
        let result = sqlx::query(
            "UPDATE resource_adjustment_suggestions SET status='expired', updated_at=NOW() \
             WHERE flight_id=$1 AND kind=$2 AND status='pending'",
        )
        .bind(flight_id)
        .bind(kind)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() as usize)
    }
}

// ---------------------------------------------------------------------------
// Aircraft
// ---------------------------------------------------------------------------

pub struct PgAircraftRepository {
    pool: PgPool,
}

impl PgAircraftRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AircraftRepository for PgAircraftRepository {
    async fn find_by_registration(&self, registration: &str) -> Result<Option<Aircraft>, DomainError> {
        let row = sqlx::query(
            "SELECT registration, first_seen_at, last_seen_at, notes FROM aircraft WHERE registration = $1",
        )
        .bind(registration)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| Aircraft {
            registration: r.get("registration"),
            first_seen_at: r.get("first_seen_at"),
            last_seen_at: r.get("last_seen_at"),
            notes: r.get("notes"),
        }))
    }

    async fn upsert(&self, aircraft: &Aircraft) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO aircraft (registration, first_seen_at, last_seen_at, notes) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (registration) DO UPDATE SET \
                 last_seen_at = GREATEST(aircraft.last_seen_at, EXCLUDED.last_seen_at), \
                 notes = COALESCE(EXCLUDED.notes, aircraft.notes)",
        )
        .bind(&aircraft.registration)
        .bind(aircraft.first_seen_at)
        .bind(aircraft.last_seen_at)
        .bind(&aircraft.notes)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn touch(&self, registration: &str) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO aircraft (registration) VALUES ($1) ON CONFLICT (registration) DO UPDATE SET last_seen_at = NOW()")
            .bind(registration)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// StandOccupation
// ---------------------------------------------------------------------------

pub struct PgStandOccupationRepository {
    pool: PgPool,
}

impl PgStandOccupationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_occupation(r: &sqlx::postgres::PgRow) -> StandOccupation {
    StandOccupation {
        id: r.get("id"),
        registration: r.get("registration"),
        stand_code: StandNumber(r.get("stand_code")),
        starts_at: r.get("starts_at"),
        ends_at: r.get("ends_at"),
        kind: match r.get::<String, _>("kind").as_str() {
            "moving" => OccupationKind::Moving,
            _ => OccupationKind::Normal,
        },
        moving_to_stand: r
            .try_get::<Option<String>, _>("moving_to_stand")
            .unwrap_or(None)
            .map(StandNumber),
        flight_id: r
            .try_get::<Option<String>, _>("flight_id")
            .unwrap_or(None)
            .map(FlightId),
        status: match r.get::<String, _>("status").as_str() {
            "released" => OccupationStatus::Released,
            "expired" => OccupationStatus::Expired,
            _ => OccupationStatus::Active,
        },
        created_by: r.try_get("created_by").unwrap_or(None),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

const OCCUPATION_COLUMNS: &str = "id, registration, stand_code, starts_at, ends_at, kind, moving_to_stand, flight_id, status, created_by, created_at, updated_at";

#[async_trait]
impl StandOccupationRepository for PgStandOccupationRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<StandOccupation>, DomainError> {
        let row = sqlx::query("SELECT * FROM stand_occupations WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_occupation(&r)))
    }

    async fn create(&self, occupation: &StandOccupation) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO stand_occupations ({}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            OCCUPATION_COLUMNS
        ))
        .bind(&occupation.id)
        .bind(&occupation.registration)
        .bind(&occupation.stand_code.0)
        .bind(occupation.starts_at)
        .bind(occupation.ends_at)
        .bind(occupation_kind_str(occupation.kind))
        .bind(occupation.moving_to_stand.as_ref().map(|s| &s.0))
        .bind(occupation.flight_id.as_ref().map(|f| &f.0))
        .bind(occupation_status_str(occupation.status))
        .bind(&occupation.created_by)
        .bind(occupation.created_at)
        .bind(occupation.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, occupation: &StandOccupation) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE stand_occupations SET stand_code=$2, starts_at=$3, ends_at=$4, kind=$5, \
             moving_to_stand=$6, status=$7, updated_at=NOW() WHERE id=$1",
        )
        .bind(&occupation.id)
        .bind(&occupation.stand_code.0)
        .bind(occupation.starts_at)
        .bind(occupation.ends_at)
        .bind(occupation_kind_str(occupation.kind))
        .bind(occupation.moving_to_stand.as_ref().map(|s| &s.0))
        .bind(occupation_status_str(occupation.status))
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn release(&self, id: &str, released_by: &str) -> Result<Option<StandOccupation>, DomainError> {
        let row = sqlx::query(
            "UPDATE stand_occupations SET status='released', updated_at=NOW(), created_by=COALESCE(created_by,$2) \
             WHERE id=$1 AND status='active' RETURNING *",
        )
        .bind(id)
        .bind(released_by)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_occupation(&r)))
    }

    async fn find_active_by_registration(
        &self,
        registration: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<StandOccupation>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM stand_occupations WHERE registration=$1 AND status='active' AND ends_at > $2 \
             ORDER BY starts_at DESC LIMIT 1",
        )
        .bind(registration)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_occupation(&r)))
    }

    async fn find_active_by_flight(&self, flight_id: &str) -> Result<Vec<StandOccupation>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM stand_occupations WHERE flight_id=$1 AND status='active' ORDER BY starts_at DESC",
        )
        .bind(flight_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_occupation).collect())
    }

    async fn list_by_registration(&self, registration: &str, limit: i64) -> Result<Vec<StandOccupation>, DomainError> {
        let rows =
            sqlx::query("SELECT * FROM stand_occupations WHERE registration=$1 ORDER BY starts_at DESC LIMIT $2")
                .bind(registration)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_occupation).collect())
    }

    async fn list_overlapping(
        &self,
        stand_code: &str,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Vec<StandOccupation>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM stand_occupations \
             WHERE stand_code=$1 AND status='active' AND starts_at < $3 AND ends_at > $2 \
             ORDER BY starts_at DESC",
        )
        .bind(stand_code)
        .bind(starts_at)
        .bind(ends_at)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_occupation).collect())
    }

    async fn list_active_by_registration(
        &self,
        registration: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<StandOccupation>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM stand_occupations WHERE registration=$1 AND status='active' AND ends_at > $2 \
             ORDER BY starts_at DESC",
        )
        .bind(registration)
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_occupation).collect())
    }
}

// ---------------------------------------------------------------------------
// GateAssignment
// ---------------------------------------------------------------------------

pub struct PgGateAssignmentRepository {
    pool: PgPool,
}

impl PgGateAssignmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_assignment(r: &sqlx::postgres::PgRow) -> GateAssignment {
    GateAssignment {
        id: r.get("id"),
        registration: r.get("registration"),
        gate_code: GateNumber(r.get("gate_code")),
        starts_at: r.get("starts_at"),
        ends_at: r.get("ends_at"),
        flight_id: r
            .try_get::<Option<String>, _>("flight_id")
            .unwrap_or(None)
            .map(FlightId),
        status: match r.get::<String, _>("status").as_str() {
            "released" => AssignmentStatus::Released,
            "expired" => AssignmentStatus::Expired,
            _ => AssignmentStatus::Active,
        },
        created_by: r.try_get("created_by").unwrap_or(None),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

const ASSIGNMENT_COLUMNS: &str =
    "id, registration, gate_code, starts_at, ends_at, flight_id, status, created_by, created_at, updated_at";

#[async_trait]
impl GateAssignmentRepository for PgGateAssignmentRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<GateAssignment>, DomainError> {
        let row = sqlx::query("SELECT * FROM gate_assignments WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_assignment(&r)))
    }

    async fn create(&self, assignment: &GateAssignment) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO gate_assignments ({}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            ASSIGNMENT_COLUMNS
        ))
        .bind(&assignment.id)
        .bind(&assignment.registration)
        .bind(&assignment.gate_code.0)
        .bind(assignment.starts_at)
        .bind(assignment.ends_at)
        .bind(assignment.flight_id.as_ref().map(|f| &f.0))
        .bind(assignment_status_str(assignment.status))
        .bind(&assignment.created_by)
        .bind(assignment.created_at)
        .bind(assignment.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, assignment: &GateAssignment) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE gate_assignments SET gate_code=$2, starts_at=$3, ends_at=$4, status=$5, updated_at=NOW() \
             WHERE id=$1",
        )
        .bind(&assignment.id)
        .bind(&assignment.gate_code.0)
        .bind(assignment.starts_at)
        .bind(assignment.ends_at)
        .bind(assignment_status_str(assignment.status))
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn release(&self, id: &str, released_by: &str) -> Result<Option<GateAssignment>, DomainError> {
        let row = sqlx::query(
            "UPDATE gate_assignments SET status='released', updated_at=NOW(), created_by=COALESCE(created_by,$2) \
             WHERE id=$1 AND status='active' RETURNING *",
        )
        .bind(id)
        .bind(released_by)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_assignment(&r)))
    }

    async fn find_active_by_registration(
        &self,
        registration: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<GateAssignment>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM gate_assignments WHERE registration=$1 AND status='active' AND ends_at > $2 \
             ORDER BY starts_at DESC LIMIT 1",
        )
        .bind(registration)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_assignment(&r)))
    }

    async fn find_active_by_flight(&self, flight_id: &str) -> Result<Vec<GateAssignment>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM gate_assignments WHERE flight_id=$1 AND status='active' ORDER BY starts_at DESC",
        )
        .bind(flight_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_assignment).collect())
    }

    async fn list_by_registration(&self, registration: &str, limit: i64) -> Result<Vec<GateAssignment>, DomainError> {
        let rows = sqlx::query("SELECT * FROM gate_assignments WHERE registration=$1 ORDER BY starts_at DESC LIMIT $2")
            .bind(registration)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_assignment).collect())
    }

    async fn list_active_by_registration(
        &self,
        registration: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<GateAssignment>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM gate_assignments WHERE registration=$1 AND status='active' AND ends_at > $2 \
             ORDER BY starts_at DESC",
        )
        .bind(registration)
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_assignment).collect())
    }
}

// ---------------------------------------------------------------------------
// TurnaroundLink
// ---------------------------------------------------------------------------

pub struct PgTurnaroundLinkRepository {
    pool: PgPool,
}

impl PgTurnaroundLinkRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_link(r: &sqlx::postgres::PgRow) -> TurnaroundLink {
    TurnaroundLink {
        id: r.get("id"),
        inbound_flight_id: FlightId(r.get("inbound_flight_id")),
        outbound_flight_id: FlightId(r.get("outbound_flight_id")),
        status: match r.get::<String, _>("status").as_str() {
            "broken" => TurnaroundLinkStatus::Broken,
            _ => TurnaroundLinkStatus::Active,
        },
        source: match r.get::<String, _>("source").as_str() {
            "manual" => TurnaroundLinkSource::Manual,
            _ => TurnaroundLinkSource::Auto,
        },
        broken_reason: r.try_get("broken_reason").unwrap_or(None),
        created_by: r.try_get("created_by").unwrap_or(None),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

const LINK_COLUMNS: &str =
    "id, inbound_flight_id, outbound_flight_id, status, source, broken_reason, created_by, created_at, updated_at";

#[async_trait]
impl TurnaroundLinkRepository for PgTurnaroundLinkRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<TurnaroundLink>, DomainError> {
        let row = sqlx::query("SELECT * FROM turnaround_links WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_link(&r)))
    }

    async fn create(&self, link: &TurnaroundLink) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO turnaround_links ({}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
            LINK_COLUMNS
        ))
        .bind(&link.id)
        .bind(&link.inbound_flight_id.0)
        .bind(&link.outbound_flight_id.0)
        .bind(link_status_str(link.status))
        .bind(link_source_str(link.source))
        .bind(&link.broken_reason)
        .bind(&link.created_by)
        .bind(link.created_at)
        .bind(link.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, link: &TurnaroundLink) -> Result<(), DomainError> {
        sqlx::query("UPDATE turnaround_links SET status=$2, broken_reason=$3, updated_at=NOW() WHERE id=$1")
            .bind(&link.id)
            .bind(link_status_str(link.status))
            .bind(&link.broken_reason)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn find_active_by_outbound(&self, outbound_flight_id: &str) -> Result<Option<TurnaroundLink>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM turnaround_links WHERE outbound_flight_id=$1 AND status='active' ORDER BY created_at DESC LIMIT 1",
        )
        .bind(outbound_flight_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_link(&r)))
    }

    async fn find_active_by_inbound(&self, inbound_flight_id: &str) -> Result<Option<TurnaroundLink>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM turnaround_links WHERE inbound_flight_id=$1 AND status='active' ORDER BY created_at DESC LIMIT 1",
        )
        .bind(inbound_flight_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_link(&r)))
    }

    async fn list_by_flight(&self, flight_id: &str) -> Result<Vec<TurnaroundLink>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM turnaround_links WHERE inbound_flight_id=$1 OR outbound_flight_id=$1 ORDER BY created_at DESC",
        )
        .bind(flight_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_link).collect())
    }

    async fn find_candidates_for_outbound(
        &self,
        registration: &str,
        outbound_flight_id: &str,
        outbound_sched_departure: Option<DateTime<Utc>>,
        window_minutes: i64,
    ) -> Result<Vec<(String, DateTime<Utc>)>, DomainError> {
        let rows = sqlx::query(
            "SELECT f.flight_id, f.actual_arrival \
             FROM flights f \
             WHERE f.registration = $1 \
               AND f.flight_id <> $2 \
               AND EXISTS ( \
                   SELECT 1 FROM flight_legs fl \
                   WHERE fl.flight_id = f.flight_id AND fl.leg_type = 'inbound' \
               ) \
               AND f.status IN (2, 8, 10) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM turnaround_links tl \
                   WHERE tl.inbound_flight_id = f.flight_id AND tl.outbound_flight_id = $2 AND tl.status = 'active' \
               ) \
             ORDER BY f.actual_arrival DESC",
        )
        .bind(registration)
        .bind(outbound_flight_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut candidates = Vec::new();
        for row in rows {
            let arrival: Option<DateTime<Utc>> = row.try_get("actual_arrival").unwrap_or(None);
            let inbound_id: String = row.get("flight_id");
            if let (Some(arrival), Some(dep)) = (arrival, outbound_sched_departure) {
                if dep.signed_duration_since(arrival).num_minutes() <= window_minutes
                    && dep.signed_duration_since(arrival).num_minutes() >= -60
                {
                    candidates.push((inbound_id, arrival));
                }
            } else {
                candidates.push((inbound_id, chrono::Utc::now()));
            }
        }
        Ok(candidates)
    }

    async fn list_outbound_for_autolink(
        &self,
        limit: i64,
    ) -> Result<Vec<(String, String, Option<DateTime<Utc>>)>, DomainError> {
        let limit = limit.clamp(1, 500);
        // status < 7: 未起飞 (Departed=7)；排除 draft；需要 registration + outbound flight_leg
        let rows = sqlx::query(
            "SELECT f.flight_id, f.registration, f.scheduled_departure \
             FROM flights f \
             WHERE f.registration IS NOT NULL \
               AND btrim(f.registration) <> '' \
               AND EXISTS ( \
                   SELECT 1 FROM flight_legs fl \
                   WHERE fl.flight_id = f.flight_id AND fl.leg_type = 'outbound' \
               ) \
               AND COALESCE(f.is_draft, FALSE) = FALSE \
               AND f.status < 7 \
               AND NOT EXISTS ( \
                   SELECT 1 FROM turnaround_links tl \
                   WHERE tl.outbound_flight_id = f.flight_id AND tl.status = 'active' \
               ) \
             ORDER BY f.scheduled_departure NULLS LAST \
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let flight_id: String = row.get("flight_id");
                let registration: Option<String> = row.try_get("registration").unwrap_or(None);
                let registration = registration?.trim().to_string();
                if registration.is_empty() {
                    return None;
                }
                let scheduled_departure: Option<DateTime<Utc>> = row.try_get("scheduled_departure").unwrap_or(None);
                Some((flight_id, registration, scheduled_departure))
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// ResourceAdjustmentSuggestion
// ---------------------------------------------------------------------------

pub struct PgResourceAdjustmentSuggestionRepository {
    pool: PgPool,
}

impl PgResourceAdjustmentSuggestionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_suggestion(r: &sqlx::postgres::PgRow) -> ResourceAdjustmentSuggestion {
    ResourceAdjustmentSuggestion {
        id: r.get("id"),
        flight_id: FlightId(r.get("flight_id")),
        kind: match r.get::<String, _>("kind").as_str() {
            "gate" => SuggestionKind::Gate,
            _ => SuggestionKind::Stand,
        },
        current_value: r.try_get("current_value").unwrap_or(None),
        suggested_value: r.get("suggested_value"),
        status: match r.get::<String, _>("status").as_str() {
            "accepted_executed" => SuggestionStatus::AcceptedExecuted,
            "rejected" => SuggestionStatus::Rejected,
            "expired" => SuggestionStatus::Expired,
            _ => SuggestionStatus::Pending,
        },
        reason: r.try_get("reason").unwrap_or(None),
        payload: r.try_get("payload").unwrap_or(serde_json::json!({})),
        created_by: r.get("created_by"),
        decided_by: r.try_get("decided_by").unwrap_or(None),
        decided_at: r.try_get("decided_at").unwrap_or(None),
        expires_at: r.try_get("expires_at").unwrap_or(None),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

#[async_trait]
impl ResourceAdjustmentSuggestionRepository for PgResourceAdjustmentSuggestionRepository {
    async fn create(&self, suggestion: &ResourceAdjustmentSuggestion) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO resource_adjustment_suggestions \
             (id, flight_id, kind, current_value, suggested_value, status, reason, payload, created_by, decided_by, decided_at, expires_at, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(&suggestion.id)
        .bind(&suggestion.flight_id.0)
        .bind(suggestion_kind_str(suggestion.kind))
        .bind(&suggestion.current_value)
        .bind(&suggestion.suggested_value)
        .bind(suggestion_status_str(suggestion.status))
        .bind(&suggestion.reason)
        .bind(&suggestion.payload)
        .bind(&suggestion.created_by)
        .bind(&suggestion.decided_by)
        .bind(suggestion.decided_at)
        .bind(suggestion.expires_at)
        .bind(suggestion.created_at)
        .bind(suggestion.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update_status(
        &self,
        id: &str,
        status: &str,
        decided_by: Option<&str>,
        decided_at: Option<DateTime<Utc>>,
    ) -> Result<Option<ResourceAdjustmentSuggestion>, DomainError> {
        let row = sqlx::query(
            "UPDATE resource_adjustment_suggestions SET status=$2, decided_by=$3, decided_at=$4, updated_at=NOW() \
             WHERE id=$1 RETURNING *",
        )
        .bind(id)
        .bind(status)
        .bind(decided_by)
        .bind(decided_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_suggestion(&r)))
    }

    async fn find_pending(
        &self,
        flight_id: &str,
        kind: &str,
    ) -> Result<Vec<ResourceAdjustmentSuggestion>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM resource_adjustment_suggestions \
             WHERE flight_id=$1 AND kind=$2 AND status='pending' ORDER BY created_at DESC",
        )
        .bind(flight_id)
        .bind(kind)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_suggestion).collect())
    }

    async fn list(
        &self,
        flight_id: Option<&str>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ResourceAdjustmentSuggestion>, DomainError> {
        let mut query = String::from("SELECT * FROM resource_adjustment_suggestions WHERE 1=1");
        if let Some(f) = flight_id {
            query.push_str(" AND flight_id = '");
            query.push_str(&f.replace('\'', "''"));
            query.push('\'');
        }
        if let Some(s) = status {
            query.push_str(" AND status = '");
            query.push_str(&s.replace('\'', "''"));
            query.push('\'');
        }
        query.push_str(" ORDER BY created_at DESC LIMIT ");
        query.push_str(&limit.to_string());
        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_suggestion).collect())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<ResourceAdjustmentSuggestion>, DomainError> {
        let row = sqlx::query("SELECT * FROM resource_adjustment_suggestions WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_suggestion(&r)))
    }
}

// ---------------------------------------------------------------------------
// 枚举映射
// ---------------------------------------------------------------------------

fn occupation_kind_str(kind: OccupationKind) -> &'static str {
    match kind {
        OccupationKind::Normal => "normal",
        OccupationKind::Moving => "moving",
    }
}

fn occupation_status_str(status: OccupationStatus) -> &'static str {
    match status {
        OccupationStatus::Active => "active",
        OccupationStatus::Released => "released",
        OccupationStatus::Expired => "expired",
    }
}

fn assignment_status_str(status: AssignmentStatus) -> &'static str {
    match status {
        AssignmentStatus::Active => "active",
        AssignmentStatus::Released => "released",
        AssignmentStatus::Expired => "expired",
    }
}

fn link_status_str(status: TurnaroundLinkStatus) -> &'static str {
    match status {
        TurnaroundLinkStatus::Active => "active",
        TurnaroundLinkStatus::Broken => "broken",
    }
}

fn link_source_str(source: TurnaroundLinkSource) -> &'static str {
    match source {
        TurnaroundLinkSource::Auto => "auto",
        TurnaroundLinkSource::Manual => "manual",
    }
}

fn suggestion_kind_str(kind: SuggestionKind) -> &'static str {
    match kind {
        SuggestionKind::Stand => "stand",
        SuggestionKind::Gate => "gate",
    }
}

fn suggestion_status_str(status: SuggestionStatus) -> &'static str {
    match status {
        SuggestionStatus::Pending => "pending",
        SuggestionStatus::AcceptedExecuted => "accepted_executed",
        SuggestionStatus::Rejected => "rejected",
        SuggestionStatus::Expired => "expired",
    }
}
