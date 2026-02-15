//! Per-connection gateway session state.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

/// State for a single WebSocket connection.
pub struct GatewaySession {
    /// Unique session identifier (`gw_` prefixed ULID).
    pub session_id: String,
    /// Authenticated user ID.
    pub user_id: String,
    /// Authenticated username (cached at IDENTIFY time).
    pub username: String,
    /// Community IDs this user is a member of (populated at IDENTIFY).
    pub communities: HashSet<String>,
    /// Monotonically increasing sequence number for dispatch events.
    seq: AtomicU64,
}

impl GatewaySession {
    pub fn new(session_id: String, user_id: String, username: String, communities: HashSet<String>) -> Self {
        Self {
            session_id,
            user_id,
            username,
            communities,
            seq: AtomicU64::new(0),
        }
    }

    /// Restore a session with a given sequence number (used on RESUME).
    pub fn with_seq(
        session_id: String,
        user_id: String,
        username: String,
        communities: HashSet<String>,
        seq: u64,
    ) -> Self {
        Self {
            session_id,
            user_id,
            username,
            communities,
            seq: AtomicU64::new(seq),
        }
    }

    /// Get the next sequence number for a dispatch event.
    pub fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Check whether this session should receive events for a given community.
    pub fn is_subscribed(&self, community_id: &str) -> bool {
        self.communities.contains(community_id)
    }
}
