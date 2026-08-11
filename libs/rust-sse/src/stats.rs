//! Atomic statistics module
//! 
//! Provides completely lock-free statistics counting using atomic operations.

use std::sync::atomic::{AtomicU64, Ordering};

/// Atomic statistics counter - completely lock-free
pub struct AtomicStats {
    connections: AtomicU64,
    messages_sent: AtomicU64,
    messages_failed: AtomicU64,
}

impl AtomicStats {
    pub fn new() -> Self {
        AtomicStats {
            connections: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            messages_failed: AtomicU64::new(0),
        }
    }

    /// Increment connection count
    pub fn increment_connections(&self) {
        self.connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement connection count
    pub fn decrement_connections(&self) {
        self.connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Add to sent message count
    pub fn add_sent(&self, count: u64) {
        self.messages_sent.fetch_add(count, Ordering::Relaxed);
    }

    /// Add to failed message count
    pub fn add_failed(&self, count: u64) {
        self.messages_failed.fetch_add(count, Ordering::Relaxed);
    }

    /// Get current connection count
    pub fn connections(&self) -> u64 {
        self.connections.load(Ordering::Relaxed)
    }

    /// Get total messages sent
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get total messages failed
    pub fn messages_failed(&self) -> u64 {
        self.messages_failed.load(Ordering::Relaxed)
    }

    /// Reset all counters
    pub fn reset(&self) {
        self.connections.store(0, Ordering::Relaxed);
        self.messages_sent.store(0, Ordering::Relaxed);
        self.messages_failed.store(0, Ordering::Relaxed);
    }
}

impl Default for AtomicStats {
    fn default() -> Self {
        Self::new()
    }
}
