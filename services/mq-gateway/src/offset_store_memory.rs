use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::offset_store::{ConsumerOffset, OffsetStore, OffsetStoreError};

pub struct MemoryOffsetStore {
    data: Mutex<HashMap<ConsumerOffset, i64>>,
}

impl MemoryOffsetStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemoryOffsetStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OffsetStore for MemoryOffsetStore {
    async fn load(&self, key: &ConsumerOffset) -> Result<Option<i64>, OffsetStoreError> {
        let data = self.data.lock().map_err(|e| {
            OffsetStoreError::Unavailable(format!("memory offset store poisoned: {e}"))
        })?;
        Ok(data.get(key).copied())
    }

    async fn save(&self, key: &ConsumerOffset, offset: i64) -> Result<(), OffsetStoreError> {
        let mut data = self.data.lock().map_err(|e| {
            OffsetStoreError::Unavailable(format!("memory offset store poisoned: {e}"))
        })?;
        data.insert(key.clone(), offset);
        Ok(())
    }
}
