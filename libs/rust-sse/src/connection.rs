//! Connection management module
//! 
//! Uses DashMap for lock-free concurrent access to connection state.

use dashmap::DashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Connection information
pub struct ConnectionInfo {
    pub created_at: u64,
    pub last_heartbeat: u64,
}

/// Lock-free connection manager using DashMap
pub struct ConnectionManager {
    connections: DashMap<String, ConnectionInfo>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        ConnectionManager {
            connections: DashMap::new(),
        }
    }

    /// Register a new connection
    pub fn register(&self, client_id: String) {
        let now = Self::current_timestamp();
        self.connections.insert(client_id, ConnectionInfo {
            created_at: now,
            last_heartbeat: now,
        });
    }

    /// Remove a connection, returns true if it existed
    pub fn remove(&self, client_id: &str) -> bool {
        self.connections.remove(client_id).is_some()
    }

    /// Check if a connection exists
    pub fn contains(&self, client_id: &str) -> bool {
        self.connections.contains_key(client_id)
    }

    /// Update heartbeat timestamp
    pub fn update_heartbeat(&self, client_id: &str) -> bool {
        if let Some(mut conn) = self.connections.get_mut(client_id) {
            conn.last_heartbeat = Self::current_timestamp();
            true
        } else {
            false
        }
    }

    /// Get current connection count
    pub fn count(&self) -> usize {
        self.connections.len()
    }

    /// Get list of stale connections (read-only, does not remove them)
    pub fn get_stale_connections(&self, threshold_secs: u64) -> Vec<String> {
        let now = Self::current_timestamp();
        let mut stale = Vec::new();

        for entry in self.connections.iter() {
            if now - entry.value().last_heartbeat > threshold_secs {
                stale.push(entry.key().clone());
            }
        }

        stale
    }

    /// Atomically detect and remove stale connections, returning the removed client IDs
    pub fn cleanup_stale_connections(&self, threshold_secs: u64) -> Vec<String> {
        let now = Self::current_timestamp();
        let mut removed = Vec::new();

        self.connections.retain(|key, conn| {
            if now - conn.last_heartbeat > threshold_secs {
                removed.push(key.clone());
                false
            } else {
                true
            }
        });

        removed
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
