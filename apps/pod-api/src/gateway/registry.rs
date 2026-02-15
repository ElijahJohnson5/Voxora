//! Session registry with per-session replay buffers for gateway resume.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use parking_lot::Mutex;
use serde_json::Value;

/// Maximum number of events stored in a session's replay buffer.
const MAX_REPLAY_BUFFER: usize = 1000;

/// Sessions disconnected longer than this are eligible for cleanup.
const SESSION_TTL: Duration = Duration::from_secs(5 * 60);

/// A single event stored in the replay buffer.
#[derive(Debug, Clone)]
pub struct ReplayEntry {
    pub seq: u64,
    pub event_name: String,
    pub data: Value,
}

/// Per-session metadata and replay buffer.
pub struct SessionEntry {
    pub session_id: String,
    pub user_id: String,
    pub username: String,
    pub communities: HashSet<String>,
    pub seq: u64,
    pub replay_buffer: VecDeque<ReplayEntry>,
    pub disconnected_at: Option<Instant>,
}

/// Shared registry of all gateway sessions.
///
/// Uses `DashMap` for shard-level concurrency and `parking_lot::Mutex` per
/// entry for non-poisoning, fast locking.
pub struct SessionRegistry {
    sessions: Arc<DashMap<String, Mutex<SessionEntry>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }

    /// Register a new session after IDENTIFY.
    pub fn register(&self, session_id: String, user_id: String, username: String, communities: HashSet<String>) {
        let entry = SessionEntry {
            session_id: session_id.clone(),
            user_id,
            username,
            communities,
            seq: 0,
            replay_buffer: VecDeque::new(),
            disconnected_at: None,
        };
        self.sessions.insert(session_id, Mutex::new(entry));
    }

    /// Append a dispatched event to the session's replay buffer.
    /// Evicts the oldest entry if the buffer exceeds capacity.
    pub fn append_event(&self, session_id: &str, seq: u64, event_name: &str, data: Value) {
        if let Some(entry) = self.sessions.get(session_id) {
            let mut e = entry.lock();
            e.seq = seq;
            e.replay_buffer.push_back(ReplayEntry {
                seq,
                event_name: event_name.to_string(),
                data,
            });
            while e.replay_buffer.len() > MAX_REPLAY_BUFFER {
                e.replay_buffer.pop_front();
            }
        }
    }

    /// Mark a session as disconnected (sets `disconnected_at`).
    pub fn mark_disconnected(&self, session_id: &str) {
        if let Some(entry) = self.sessions.get(session_id) {
            let mut e = entry.lock();
            e.disconnected_at = Some(Instant::now());
        }
    }

    /// Mark a session as connected (clears `disconnected_at`).
    pub fn mark_connected(&self, session_id: &str) {
        if let Some(entry) = self.sessions.get(session_id) {
            let mut e = entry.lock();
            e.disconnected_at = None;
        }
    }

    /// Return all buffered events with `seq > after_seq`.
    ///
    /// Returns `None` if the session doesn't exist or the requested seq is
    /// before the start of the buffer (events were evicted).
    pub fn replay_after(&self, session_id: &str, after_seq: u64) -> Option<Vec<ReplayEntry>> {
        let entry = self.sessions.get(session_id)?;
        let e = entry.lock();

        // If the buffer is empty, only valid if after_seq matches current seq.
        if e.replay_buffer.is_empty() {
            return if after_seq == e.seq {
                Some(Vec::new())
            } else if after_seq == 0 && e.seq == 0 {
                Some(Vec::new())
            } else {
                None
            };
        }

        // Check if the requested seq is before the buffer start.
        let buffer_start_seq = e.replay_buffer.front().unwrap().seq;
        if after_seq < buffer_start_seq.saturating_sub(1) {
            return None; // Too old — client must re-IDENTIFY.
        }

        let events: Vec<ReplayEntry> = e
            .replay_buffer
            .iter()
            .filter(|entry| entry.seq > after_seq)
            .cloned()
            .collect();

        Some(events)
    }

    /// Read session metadata for resume validation.
    pub fn get_session_info(
        &self,
        session_id: &str,
    ) -> Option<(String, String, HashSet<String>, u64)> {
        let entry = self.sessions.get(session_id)?;
        let e = entry.lock();
        Some((e.user_id.clone(), e.username.clone(), e.communities.clone(), e.seq))
    }

    /// Remove sessions that have been disconnected longer than the TTL.
    /// Returns the number of sessions removed.
    pub fn cleanup_expired(&self) -> usize {
        let now = Instant::now();
        let before = self.sessions.len();
        self.sessions.retain(|_, entry| {
            let e = entry.lock();
            match e.disconnected_at {
                Some(at) => now.duration_since(at) < SESSION_TTL,
                None => true, // Still connected.
            }
        });
        before - self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry_with_session() -> (SessionRegistry, String) {
        let registry = SessionRegistry::new();
        let session_id = "gw_test_session".to_string();
        let mut communities = HashSet::new();
        communities.insert("comm1".to_string());
        registry.register(session_id.clone(), "user1".to_string(), "testuser".to_string(), communities);
        (registry, session_id)
    }

    #[test]
    fn register_and_get_session_info() {
        let (registry, session_id) = make_registry_with_session();
        let (user_id, username, communities, seq) = registry.get_session_info(&session_id).unwrap();
        assert_eq!(user_id, "user1");
        assert_eq!(username, "testuser");
        assert!(communities.contains("comm1"));
        assert_eq!(seq, 0);
    }

    #[test]
    fn get_session_info_returns_none_for_unknown() {
        let registry = SessionRegistry::new();
        assert!(registry.get_session_info("bogus").is_none());
    }

    #[test]
    fn append_event_and_replay() {
        let (registry, session_id) = make_registry_with_session();

        registry.append_event(&session_id, 1, "MESSAGE_CREATE", serde_json::json!({"a": 1}));
        registry.append_event(&session_id, 2, "MESSAGE_CREATE", serde_json::json!({"a": 2}));
        registry.append_event(&session_id, 3, "MESSAGE_UPDATE", serde_json::json!({"a": 3}));

        // Replay after seq 0 → all events.
        let events = registry.replay_after(&session_id, 0).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[2].seq, 3);

        // Replay after seq 2 → only event 3.
        let events = registry.replay_after(&session_id, 2).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, 3);

        // Replay after seq 3 → nothing.
        let events = registry.replay_after(&session_id, 3).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn replay_evicts_oldest_when_over_capacity() {
        let (registry, session_id) = make_registry_with_session();

        // Fill the buffer beyond capacity.
        for i in 1..=(MAX_REPLAY_BUFFER + 50) {
            registry.append_event(
                &session_id,
                i as u64,
                "EVENT",
                serde_json::json!({"i": i}),
            );
        }

        // Buffer should have exactly MAX_REPLAY_BUFFER entries.
        let entry = registry.sessions.get(&session_id).unwrap();
        let e = entry.lock();
        assert_eq!(e.replay_buffer.len(), MAX_REPLAY_BUFFER);
        // First entry should be seq 51 (first 50 evicted).
        assert_eq!(e.replay_buffer.front().unwrap().seq, 51);
        drop(e);
        drop(entry);

        // Replay from seq 0 should fail (too old).
        assert!(registry.replay_after(&session_id, 0).is_none());

        // Replay from seq 50 should work (just at the boundary).
        let events = registry.replay_after(&session_id, 50).unwrap();
        assert_eq!(events.len(), MAX_REPLAY_BUFFER);
    }

    #[test]
    fn mark_disconnected_and_connected() {
        let (registry, session_id) = make_registry_with_session();

        // Initially connected (disconnected_at = None).
        let entry = registry.sessions.get(&session_id).unwrap();
        assert!(entry.lock().disconnected_at.is_none());
        drop(entry);

        registry.mark_disconnected(&session_id);
        let entry = registry.sessions.get(&session_id).unwrap();
        assert!(entry.lock().disconnected_at.is_some());
        drop(entry);

        registry.mark_connected(&session_id);
        let entry = registry.sessions.get(&session_id).unwrap();
        assert!(entry.lock().disconnected_at.is_none());
    }

    #[test]
    fn cleanup_expired_removes_old_sessions() {
        let registry = SessionRegistry::new();
        let mut communities = HashSet::new();
        communities.insert("c".to_string());

        // Create two sessions.
        registry.register("s1".to_string(), "u1".to_string(), "user1".to_string(), communities.clone());
        registry.register("s2".to_string(), "u2".to_string(), "user2".to_string(), communities);

        // Mark s1 as disconnected a long time ago.
        registry.mark_disconnected("s1");
        {
            let entry = registry.sessions.get("s1").unwrap();
            let mut e = entry.lock();
            e.disconnected_at = Some(Instant::now() - Duration::from_secs(600));
        }

        // s2 is still connected.
        let removed = registry.cleanup_expired();
        assert_eq!(removed, 1);
        assert!(registry.get_session_info("s1").is_none());
        assert!(registry.get_session_info("s2").is_some());
    }

    #[test]
    fn replay_empty_buffer_at_seq_zero() {
        let (registry, session_id) = make_registry_with_session();
        let events = registry.replay_after(&session_id, 0).unwrap();
        assert!(events.is_empty());
    }
}
