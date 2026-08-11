//! System configuration / feature-flags persistence port.
//!
//! Abstracts load/replace access to the system config table so
//! `SystemFlagsService` does not depend on a concrete infra repository.

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::error::DomainError;

/// Port for system flags / config key-value storage.
#[async_trait]
pub trait SystemFlagsRepository: Send + Sync {
    /// Load the full config snapshot as a nested JSON object map.
    async fn load(&self) -> Result<Map<String, Value>, DomainError>;

    /// Replace all stored config rows with `snapshot`.
    async fn replace_all(&self, snapshot: &Map<String, Value>) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn assert_object_safe(_: &dyn SystemFlagsRepository) {}

        struct Stub;
        #[async_trait]
        impl SystemFlagsRepository for Stub {
            async fn load(&self) -> Result<Map<String, Value>, DomainError> {
                Ok(Map::new())
            }
            async fn replace_all(&self, _: &Map<String, Value>) -> Result<(), DomainError> {
                Ok(())
            }
        }

        assert_object_safe(&Stub);
    }
}
