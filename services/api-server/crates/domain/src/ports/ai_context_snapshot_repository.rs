use async_trait::async_trait;
use serde_json::Value;

use crate::error::DomainError;

/// Snapshot-backed object types that may be embedded in an AI context envelope.
///
/// Keeping this list semantic prevents application code from passing table names
/// or SQL predicates through the port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiContextSnapshotKind {
    Stand,
    Team,
    Equipment,
    Terminal,
    Gate,
    BaggageCarousel,
    StandOccupation,
    GateAssignment,
    CarouselAssignment,
    Department,
    EquipmentType,
    Aircraft,
    TurnaroundLink,
    Qualification,
    TaskType,
    Personnel,
}

#[async_trait]
pub trait AiContextSnapshotRepository: Send + Sync {
    async fn load_snapshot(&self, kind: AiContextSnapshotKind, object_id: &str) -> Result<Option<Value>, DomainError>;
}
