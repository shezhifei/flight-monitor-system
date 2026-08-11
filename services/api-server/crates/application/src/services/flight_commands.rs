//! Explicit flight write commands (ADR-0002 command boundary).
//!
//! `FlightCreateCommand` and `FlightUpdateCommand` are the **single accepted
//! write-intent boundary** for flight create/update. Routes and AI gateways must
//! build one of these and call `FlightService::execute_create` /
//! `FlightService::execute_update`; they must not reach for persistence APIs
//! directly.
//!
//! A command is cheap and side-effect free: it only captures the actor and the
//! validated intent. The structural invariants of the write (non-empty
//! `flight_id`, at least one touched field, no empty id) are encoded here so
//! they fail fast before the service is touched.

use crate::schemas::flight_schemas::{FlightCreate, FlightUpdate};

/// Validation error for a flight write command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlightCommandError {
    /// A create command carried an explicitly provided but blank `flight_id`.
    EmptyFlightId,
    /// An update command was built with a blank `flight_id`.
    MissingFlightId,
    /// An update command carried no touched fields (nothing to write).
    NoUpdateFields,
}

impl std::fmt::Display for FlightCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyFlightId => write!(f, "flight_id 不能为空字符串"),
            Self::MissingFlightId => write!(f, "更新命令必须携带非空 flight_id"),
            Self::NoUpdateFields => write!(f, "更新命令未携带任何变更字段"),
        }
    }
}

impl std::error::Error for FlightCommandError {}

/// Command to create a flight.
///
/// This is the only accepted write intent for `FlightService::execute_create`.
/// The `flight_id` may be omitted in the DTO (server-assigned) so the only
/// structural failure is an explicitly provided but blank id.
#[derive(Debug, Clone)]
pub struct FlightCreateCommand {
    pub dto: FlightCreate,
    pub actor_id: Option<String>,
}

impl FlightCreateCommand {
    /// Build the command. Always succeeds structurally; invoke [`validate`] if
    /// you want to enforce invariants before executing.
    pub fn new(dto: FlightCreate, actor_id: Option<String>) -> Self {
        Self { dto, actor_id }
    }

    /// Validate the structural invariants of this create command.
    pub fn validate(&self) -> Result<(), FlightCommandError> {
        if let Some(id) = &self.dto.flight_id {
            if id.trim().is_empty() {
                return Err(FlightCommandError::EmptyFlightId);
            }
        }
        Ok(())
    }
}

/// Command to update a flight (partial patch).
///
/// This is the only accepted write intent for `FlightService::execute_update`.
/// `flight_id` is mandatory and must be non-empty; the DTO must carry at least
/// one touched field.
#[derive(Debug, Clone)]
pub struct FlightUpdateCommand {
    pub flight_id: String,
    pub dto: FlightUpdate,
    pub actor_id: Option<String>,
}

impl FlightUpdateCommand {
    /// Build and validate. Fails fast if `flight_id` is empty.
    pub fn build(
        flight_id: impl Into<String>,
        dto: FlightUpdate,
        actor_id: Option<String>,
    ) -> Result<Self, FlightCommandError> {
        let command = Self::new(flight_id, dto, actor_id);
        command.validate()?;
        Ok(command)
    }

    /// Construct without validation (keeps existing callers compiling).
    pub fn new(flight_id: impl Into<String>, dto: FlightUpdate, actor_id: Option<String>) -> Self {
        Self {
            flight_id: flight_id.into(),
            dto,
            actor_id,
        }
    }

    /// Validate the structural invariants:
    /// * `flight_id` must be non-empty,
    /// * at least one field must be touched (a `Set`/`Clear` on any nullable
    ///   field, a top-level scalar, or an optimistic `expected_version`).
    pub fn validate(&self) -> Result<(), FlightCommandError> {
        if self.flight_id.trim().is_empty() {
            return Err(FlightCommandError::MissingFlightId);
        }
        if !self.has_touched_field() {
            return Err(FlightCommandError::NoUpdateFields);
        }
        Ok(())
    }

    /// Whether the DTO carries any write intent.
    fn has_touched_field(&self) -> bool {
        let d = &self.dto;
        d.expected_version.is_some()
            || d.status.is_some()
            || d.gate.is_touched()
            || d.terminal.is_touched()
            || d.stand.is_touched()
            || d.position.is_touched()
            || d.baggage_carousel.is_touched()
            || d.scheduled_departure.is_touched()
            || d.scheduled_arrival.is_touched()
            || d.estimated_departure.is_touched()
            || d.estimated_arrival.is_touched()
            || d.actual_departure.is_touched()
            || d.actual_arrival.is_touched()
            || d.cobt_time.is_touched()
            || d.aircraft_type_detail.is_touched()
            || d.registration.is_touched()
            || d.has_boarding_restriction.is_some()
            || d.is_quick_turnaround.is_some()
            || d.is_commercial_signed.is_some()
            || d.inbound_leg.is_touched()
            || d.outbound_leg.is_touched()
            || d.flight_remarks.is_touched()
            || d.load_planning_remarks.is_touched()
            || d.aircraft_maintenance_remarks.is_touched()
            || d.aircraft_check_remarks.is_touched()
    }
}
