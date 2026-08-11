//! Rust SSE Hub - High-performance SSE broadcast module
//! 
//! This module provides lock-free connection management, topic subscription,
//! and atomic statistics for Python SSE applications via PyO3.

use pyo3::prelude::*;

mod connection;
mod topic;
mod stats;
mod dispatch;
mod timeline;
mod kpi;

use connection::ConnectionManager;
use topic::TopicSubscriber;
use stats::AtomicStats;
use std::sync::Arc;

/// High-performance SSE Hub implemented in Rust
/// 
/// Provides lock-free operations for:
/// - Connection management (DashMap)
/// - Topic subscriptions (DashMap)
/// - Statistics counting (Atomic)
#[pyclass]
pub struct RustSSEHub {
    connections: Arc<ConnectionManager>,
    topics: Arc<TopicSubscriber>,
    stats: Arc<AtomicStats>,
}

#[pymethods]
impl RustSSEHub {
    #[new]
    fn new() -> Self {
        RustSSEHub {
            connections: Arc::new(ConnectionManager::new()),
            topics: Arc::new(TopicSubscriber::new()),
            stats: Arc::new(AtomicStats::new()),
        }
    }

    /// Register a new connection
    fn register_connection(&self, client_id: String) -> bool {
        self.connections.register(client_id);
        self.stats.increment_connections();
        true
    }

    /// Remove a connection
    fn remove_connection(&self, client_id: String) -> bool {
        // Remove from topics first
        self.topics.remove_client(&client_id);
        // Then remove connection
        let removed = self.connections.remove(&client_id);
        if removed {
            self.stats.decrement_connections();
        }
        removed
    }

    /// Update heartbeat for a connection
    fn update_heartbeat(&self, client_id: String) -> bool {
        self.connections.update_heartbeat(&client_id)
    }

    /// Subscribe a client to a topic
    fn subscribe(&self, client_id: String, topic: String) -> bool {
        self.topics.subscribe(client_id, topic)
    }

    /// Unsubscribe a client from a topic
    fn unsubscribe(&self, client_id: String, topic: String) -> bool {
        self.topics.unsubscribe(&client_id, &topic)
    }

    /// Get all subscribers for a topic (lock-free snapshot)
    fn get_topic_subscribers(&self, topic: String) -> Vec<String> {
        self.topics.get_subscribers(&topic)
    }

    /// Get all topics a client is subscribed to
    fn get_client_topics(&self, client_id: String) -> Vec<String> {
        self.topics.get_client_topics(&client_id)
    }

    /// Get current connection count
    fn connection_count(&self) -> u64 {
        self.stats.connections()
    }

    /// Get statistics (connections, sent, failed) - atomic read
    fn get_stats(&self) -> (u64, u64, u64) {
        (
            self.stats.connections(),
            self.stats.messages_sent(),
            self.stats.messages_failed(),
        )
    }

    /// Increment sent message count (atomic, lock-free)
    fn increment_sent(&self, count: u64) {
        self.stats.add_sent(count);
    }

    /// Increment failed message count (atomic, lock-free)
    fn increment_failed(&self, count: u64) {
        self.stats.add_failed(count);
    }

    /// Check if a client is connected
    fn is_connected(&self, client_id: String) -> bool {
        self.connections.contains(&client_id)
    }

    /// Get list of stale connections (last heartbeat > threshold seconds ago)
    fn get_stale_connections(&self, threshold_secs: u64) -> Vec<String> {
        self.connections.get_stale_connections(threshold_secs)
    }
}

/// Python module definition
#[pymodule]
fn rust_sse(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RustSSEHub>()?;
    
    // Register dispatch calculator functions
    dispatch::register(m)?;
    
    // Register timeline layout functions
    timeline::register(m)?;
    
    // Register KPI functions
    kpi::register(m)?;
    
    Ok(())
}

