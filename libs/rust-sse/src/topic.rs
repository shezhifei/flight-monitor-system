//! Topic subscription management module
//! 
//! Uses DashMap for lock-free pub/sub pattern.

use dashmap::DashMap;
use std::collections::HashSet;
use parking_lot::RwLock;

/// Lock-free topic subscriber using DashMap
pub struct TopicSubscriber {
    /// topic -> set of client_ids
    subscriptions: DashMap<String, RwLock<HashSet<String>>>,
    /// client_id -> set of topics
    client_topics: DashMap<String, RwLock<HashSet<String>>>,
}

impl TopicSubscriber {
    pub fn new() -> Self {
        TopicSubscriber {
            subscriptions: DashMap::new(),
            client_topics: DashMap::new(),
        }
    }

    /// Subscribe a client to a topic
    pub fn subscribe(&self, client_id: String, topic: String) -> bool {
        // Add to topic subscribers
        self.subscriptions
            .entry(topic.clone())
            .or_insert_with(|| RwLock::new(HashSet::new()))
            .write()
            .insert(client_id.clone());
        
        // Add to client's topics
        self.client_topics
            .entry(client_id)
            .or_insert_with(|| RwLock::new(HashSet::new()))
            .write()
            .insert(topic);
        
        true
    }

    /// Unsubscribe a client from a topic
    pub fn unsubscribe(&self, client_id: &str, topic: &str) -> bool {
        // Remove from topic
        if let Some(subs) = self.subscriptions.get(topic) {
            subs.write().remove(client_id);
        }
        
        // Remove from client's topics
        if let Some(topics) = self.client_topics.get(client_id) {
            topics.write().remove(topic);
        }
        
        true
    }

    /// Get all subscribers for a topic - snapshot read
    pub fn get_subscribers(&self, topic: &str) -> Vec<String> {
        self.subscriptions
            .get(topic)
            .map(|s| s.read().iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all topics a client is subscribed to
    pub fn get_client_topics(&self, client_id: &str) -> Vec<String> {
        self.client_topics
            .get(client_id)
            .map(|t| t.read().iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Remove a client from all subscriptions
    pub fn remove_client(&self, client_id: &str) {
        // Get client's topics first
        let topics: Vec<String> = self.client_topics
            .get(client_id)
            .map(|t| t.read().iter().cloned().collect())
            .unwrap_or_default();
        
        // Remove from each topic
        for topic in topics {
            if let Some(subs) = self.subscriptions.get(&topic) {
                subs.write().remove(client_id);
            }
        }
        
        // Remove client entry
        self.client_topics.remove(client_id);
    }

    /// Get subscriber count for a topic
    pub fn subscriber_count(&self, topic: &str) -> usize {
        self.subscriptions
            .get(topic)
            .map(|s| s.read().len())
            .unwrap_or(0)
    }
}

impl Default for TopicSubscriber {
    fn default() -> Self {
        Self::new()
    }
}
