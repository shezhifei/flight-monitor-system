//! Per-actor access control for AI copilot batches.
//!
//! Tracks a normalized set of actor keys (username, sub, email) and grants
//! read access to a batch when any key matches `created_by`. Operators
//! obtain unrestricted access via [`AiCopilotBatchAccess::unrestricted`].

use std::collections::HashSet;

use fms_domain::models::ai_copilot::AiCopilotBusinessCaseBatch;

#[derive(Debug, Clone, Default)]
pub struct AiCopilotBatchAccess {
    actor_keys: HashSet<String>,
    can_access_all: bool,
}

impl AiCopilotBatchAccess {
    pub fn for_actor_keys<I, S>(keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            actor_keys: keys
                .into_iter()
                .filter_map(|key| normalize_actor_key(&key.into()))
                .collect(),
            can_access_all: false,
        }
    }

    pub fn unrestricted() -> Self {
        Self {
            actor_keys: HashSet::new(),
            can_access_all: true,
        }
    }

    pub(crate) fn can_access(&self, batch: &AiCopilotBusinessCaseBatch) -> bool {
        self.can_access_all
            || normalize_actor_key(&batch.created_by)
                .map(|created_by| self.actor_keys.contains(&created_by))
                .unwrap_or(false)
    }

    pub(crate) fn can_access_all(&self) -> bool {
        self.can_access_all
    }
}

pub(crate) fn normalize_actor_key(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
