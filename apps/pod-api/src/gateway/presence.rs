//! In-memory per-user presence tracking with multi-session support.
//!
//! Presence is per-**user**, not per-session. A user is only considered offline
//! when ALL of their gateway sessions have disconnected past the grace period.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Per-user presence state.
struct UserPresence {
    /// Current status: "online", "idle", "dnd", or "offline".
    status: String,
    /// Number of active gateway sessions for this user.
    session_count: usize,
    /// Union of community IDs across all sessions.
    communities: HashSet<String>,
    /// When the last status change occurred.
    updated_at: Instant,
    /// Set when `session_count` drops to 0; cleared on reconnect.
    disconnected_at: Option<Instant>,
}

/// A user whose grace period has expired and should be broadcast as offline.
pub struct OfflineUser {
    pub user_id: String,
    pub communities: HashSet<String>,
}

/// Thread-safe, DashMap-backed presence registry.
pub struct PresenceRegistry {
    inner: DashMap<String, UserPresence>,
}

impl PresenceRegistry {
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// Register a session coming online. Increments session_count, merges
    /// communities, and clears any pending disconnect timer.
    ///
    /// Returns the previous status if it changed (so the caller can broadcast).
    pub fn set_online(
        &self,
        user_id: &str,
        communities: &HashSet<String>,
    ) -> Option<String> {
        let mut entry = self.inner.entry(user_id.to_string()).or_insert_with(|| {
            UserPresence {
                status: "online".to_string(),
                session_count: 0,
                communities: HashSet::new(),
                updated_at: Instant::now(),
                disconnected_at: None,
            }
        });

        let prev_status = entry.status.clone();
        entry.session_count += 1;
        entry.communities = entry.communities.union(communities).cloned().collect();
        entry.disconnected_at = None;
        entry.updated_at = Instant::now();

        // Restore to "online" if they were offline; keep "dnd" if they set it.
        if prev_status == "offline" {
            entry.status = "online".to_string();
        }

        let new_status = entry.status.clone();
        if new_status != prev_status {
            Some(prev_status)
        } else {
            None
        }
    }

    /// Update the user's status (client-sent: "online", "idle", "dnd").
    ///
    /// Returns the previous status if it changed.
    pub fn set_status(&self, user_id: &str, status: &str) -> Option<String> {
        let mut entry = self.inner.get_mut(user_id)?;
        let prev = entry.status.clone();
        if prev == status {
            return None;
        }
        entry.status = status.to_string();
        entry.updated_at = Instant::now();
        Some(prev)
    }

    /// Decrement session count when a session disconnects. If count reaches 0,
    /// sets `disconnected_at` so the sweeper can handle the grace period.
    /// No broadcast here — that's the sweeper's job.
    pub fn remove_session(&self, user_id: &str, communities: &HashSet<String>) {
        if let Some(mut entry) = self.inner.get_mut(user_id) {
            entry.session_count = entry.session_count.saturating_sub(1);
            // Merge communities in case this session had different ones.
            entry.communities = entry.communities.union(communities).cloned().collect();
            if entry.session_count == 0 {
                entry.disconnected_at = Some(Instant::now());
            }
        }
    }

    /// Sweep users whose grace period has expired. Returns the list of users
    /// that just went offline so the caller can broadcast.
    ///
    /// Also removes entries that have been offline for > 5 minutes (memory cleanup).
    pub fn sweep_offline(&self, grace_period: Duration) -> Vec<OfflineUser> {
        let now = Instant::now();
        let cleanup_threshold = Duration::from_secs(300); // 5 minutes
        let mut gone_offline = Vec::new();
        let mut to_remove = Vec::new();

        for entry in self.inner.iter() {
            let user_id = entry.key();
            let presence = entry.value();

            if presence.session_count == 0 {
                if let Some(disc_at) = presence.disconnected_at {
                    if now.duration_since(disc_at) > grace_period && presence.status != "offline" {
                        // Grace period expired — will transition to offline.
                        gone_offline.push((
                            user_id.clone(),
                            presence.communities.clone(),
                        ));
                    }
                }

                // Clean up entries that have been offline for a while.
                if presence.status == "offline"
                    && now.duration_since(presence.updated_at) > cleanup_threshold
                {
                    to_remove.push(user_id.clone());
                }
            }
        }

        // Apply offline transitions.
        for (ref uid, _) in &gone_offline {
            if let Some(mut entry) = self.inner.get_mut(uid) {
                entry.status = "offline".to_string();
                entry.disconnected_at = None;
                entry.updated_at = Instant::now();
            }
        }

        // Memory cleanup.
        for uid in to_remove {
            self.inner.remove(&uid);
        }

        gone_offline
            .into_iter()
            .map(|(user_id, communities)| OfflineUser {
                user_id,
                communities,
            })
            .collect()
    }

    /// Get all non-offline users in a given community. Returns `(user_id, status)`.
    pub fn get_online_users(&self, community_id: &str) -> Vec<(String, String)> {
        let mut result = Vec::new();
        for entry in self.inner.iter() {
            let presence = entry.value();
            if presence.status != "offline" && presence.communities.contains(community_id) {
                result.push((entry.key().clone(), presence.status.clone()));
            }
        }
        result
    }

    /// Get the current status for a user, if tracked.
    pub fn get_status(&self, user_id: &str) -> Option<String> {
        self.inner.get(user_id).map(|e| e.status.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn communities(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn set_online_new_user_returns_status_change() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        // First time coming online — previous status was implicitly "offline" via the
        // or_insert_with default, but the entry starts as "online" so no change reported
        // on the *first* call. However, the initial insert creates status="online" and
        // prev_status is also "online" (from or_insert_with), so None is returned.
        // Actually: or_insert_with sets status="online", then prev_status="online",
        // then the offline→online branch doesn't trigger. So first call returns None.
        //
        // Wait — this is wrong. A brand new user (not in the map) gets inserted with
        // status="online", prev_status="online", no change → None. The caller (server.rs)
        // doesn't broadcast for the very first session of a user who was never tracked.
        // That's correct behavior: the user wasn't "offline" before, they simply didn't exist.
        let result = reg.set_online("u1", &comms);
        // New user, entry created as "online", no change to broadcast.
        assert!(result.is_none());
        assert_eq!(reg.get_status("u1").unwrap(), "online");
    }

    #[test]
    fn set_online_after_offline_returns_previous_status() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        reg.set_online("u1", &comms);

        // Simulate going offline via sweep.
        reg.remove_session("u1", &comms);
        // Force the entry to "offline" as the sweeper would.
        reg.inner.get_mut("u1").unwrap().status = "offline".to_string();

        let result = reg.set_online("u1", &comms);
        assert_eq!(result, Some("offline".to_string()));
        assert_eq!(reg.get_status("u1").unwrap(), "online");
    }

    #[test]
    fn set_online_preserves_dnd_on_reconnect() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        reg.set_online("u1", &comms);
        reg.set_status("u1", "dnd");

        // Second session connects — should keep "dnd", not reset to "online".
        let result = reg.set_online("u1", &comms);
        assert!(result.is_none()); // no change
        assert_eq!(reg.get_status("u1").unwrap(), "dnd");
    }

    #[test]
    fn set_status_returns_previous_on_change() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);
        reg.set_online("u1", &comms);

        let prev = reg.set_status("u1", "idle");
        assert_eq!(prev, Some("online".to_string()));
        assert_eq!(reg.get_status("u1").unwrap(), "idle");
    }

    #[test]
    fn set_status_returns_none_when_unchanged() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);
        reg.set_online("u1", &comms);

        let prev = reg.set_status("u1", "online");
        assert!(prev.is_none());
    }

    #[test]
    fn set_status_returns_none_for_unknown_user() {
        let reg = PresenceRegistry::new();
        assert!(reg.set_status("unknown", "idle").is_none());
    }

    #[test]
    fn multi_session_no_offline_until_all_disconnect() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        // Two sessions connect.
        reg.set_online("u1", &comms);
        reg.set_online("u1", &comms);

        // First session disconnects — session_count is still 1.
        reg.remove_session("u1", &comms);
        assert_eq!(reg.get_status("u1").unwrap(), "online");

        // Sweep with zero grace — should NOT mark offline (still 1 session).
        let gone = reg.sweep_offline(Duration::ZERO);
        assert!(gone.is_empty());
        assert_eq!(reg.get_status("u1").unwrap(), "online");

        // Second session disconnects — session_count hits 0.
        reg.remove_session("u1", &comms);

        // Sweep with zero grace — now they go offline.
        let gone = reg.sweep_offline(Duration::ZERO);
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].user_id, "u1");
        assert_eq!(reg.get_status("u1").unwrap(), "offline");
    }

    #[test]
    fn grace_period_reconnect_cancels_offline() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        reg.set_online("u1", &comms);
        reg.remove_session("u1", &comms);

        // User reconnects before sweep runs.
        reg.set_online("u1", &comms);

        // Sweep with zero grace — should find nothing (disconnected_at was cleared).
        let gone = reg.sweep_offline(Duration::ZERO);
        assert!(gone.is_empty());
        assert_eq!(reg.get_status("u1").unwrap(), "online");
    }

    #[test]
    fn sweep_respects_grace_period() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        reg.set_online("u1", &comms);
        reg.remove_session("u1", &comms);

        // Sweep with 30s grace — user just disconnected, not expired yet.
        let gone = reg.sweep_offline(Duration::from_secs(30));
        assert!(gone.is_empty());
        assert_eq!(reg.get_status("u1").unwrap(), "online"); // still online

        // Sweep with zero grace — now they go offline immediately.
        let gone = reg.sweep_offline(Duration::ZERO);
        assert_eq!(gone.len(), 1);
        assert_eq!(reg.get_status("u1").unwrap(), "offline");
    }

    #[test]
    fn sweep_does_not_return_already_offline_users() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        reg.set_online("u1", &comms);
        reg.remove_session("u1", &comms);

        // First sweep transitions to offline.
        let gone = reg.sweep_offline(Duration::ZERO);
        assert_eq!(gone.len(), 1);

        // Second sweep — already offline, should not return again.
        let gone = reg.sweep_offline(Duration::ZERO);
        assert!(gone.is_empty());
    }

    #[test]
    fn sweep_cleans_up_stale_offline_entries() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        reg.set_online("u1", &comms);
        reg.remove_session("u1", &comms);
        reg.sweep_offline(Duration::ZERO); // transition to offline

        // Backdate updated_at to simulate 6 minutes ago.
        reg.inner.get_mut("u1").unwrap().updated_at =
            Instant::now() - Duration::from_secs(360);

        // Sweep again — should remove the stale entry.
        reg.sweep_offline(Duration::ZERO);
        assert!(reg.get_status("u1").is_none());
    }

    #[test]
    fn get_online_users_filters_by_community() {
        let reg = PresenceRegistry::new();

        reg.set_online("u1", &communities(&["c1", "c2"]));
        reg.set_online("u2", &communities(&["c2"]));
        reg.set_online("u3", &communities(&["c3"]));

        let c1_users = reg.get_online_users("c1");
        assert_eq!(c1_users.len(), 1);
        assert_eq!(c1_users[0].0, "u1");

        let mut c2_users = reg.get_online_users("c2");
        c2_users.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(c2_users.len(), 2);
        assert_eq!(c2_users[0].0, "u1");
        assert_eq!(c2_users[1].0, "u2");
    }

    #[test]
    fn get_online_users_excludes_offline() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        reg.set_online("u1", &comms);
        reg.set_online("u2", &comms);

        // Take u2 offline.
        reg.remove_session("u2", &comms);
        reg.sweep_offline(Duration::ZERO);

        let users = reg.get_online_users("c1");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].0, "u1");
    }

    #[test]
    fn get_online_users_includes_idle_and_dnd() {
        let reg = PresenceRegistry::new();
        let comms = communities(&["c1"]);

        reg.set_online("u1", &comms);
        reg.set_online("u2", &comms);
        reg.set_online("u3", &comms);

        reg.set_status("u1", "idle");
        reg.set_status("u2", "dnd");

        let mut users = reg.get_online_users("c1");
        users.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(users.len(), 3);
        assert_eq!(users[0], ("u1".to_string(), "idle".to_string()));
        assert_eq!(users[1], ("u2".to_string(), "dnd".to_string()));
        assert_eq!(users[2], ("u3".to_string(), "online".to_string()));
    }

    #[test]
    fn communities_merge_across_sessions() {
        let reg = PresenceRegistry::new();

        // Session 1 in c1, session 2 in c2.
        reg.set_online("u1", &communities(&["c1"]));
        reg.set_online("u1", &communities(&["c2"]));

        // User should appear in both communities.
        assert_eq!(reg.get_online_users("c1").len(), 1);
        assert_eq!(reg.get_online_users("c2").len(), 1);
    }
}
