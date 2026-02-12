//! Broadcast hub for dispatching Gateway events to connected sessions.
//!
//! Uses a single `tokio::sync::broadcast` channel. Each connected session
//! subscribes and filters events locally by community membership. This is
//! efficient for Phase 1's single-process architecture.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

/// Capacity of the broadcast channel. Slow receivers that fall behind will
/// skip messages (RecvError::Lagged).
const BROADCAST_CAPACITY: usize = 4096;

/// A payload broadcast to all connected gateway sessions.
#[derive(Debug, Clone)]
pub struct BroadcastPayload {
    /// The community this event belongs to.
    pub community_id: String,
    /// The dispatch event name (e.g. "MESSAGE_CREATE").
    pub event_name: String,
    /// Serialized event data (serde_json::Value).
    pub data: Value,
}

/// The global broadcast hub. Cloneable — store in AppState.
#[derive(Clone)]
pub struct GatewayBroadcast {
    sender: broadcast::Sender<Arc<BroadcastPayload>>,
}

impl GatewayBroadcast {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { sender }
    }

    /// Subscribe to the broadcast channel. Each gateway session should call
    /// this once to get its own receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<BroadcastPayload>> {
        self.sender.subscribe()
    }

    /// Dispatch an event to all connected sessions.
    pub fn dispatch(&self, payload: BroadcastPayload) {
        // send() returns Err if there are no receivers — that's fine.
        let _ = self.sender.send(Arc::new(payload));
    }
}
