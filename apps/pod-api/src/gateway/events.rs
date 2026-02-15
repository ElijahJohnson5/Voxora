//! Gateway opcodes, event types, and wire-format messages.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Opcodes
// ---------------------------------------------------------------------------

pub const OP_DISPATCH: u8 = 0;
pub const OP_HEARTBEAT: u8 = 1;
pub const OP_IDENTIFY: u8 = 2;
pub const OP_RESUME: u8 = 3;
pub const OP_HEARTBEAT_ACK: u8 = 6;
pub const OP_RECONNECT: u8 = 7;
pub const OP_PRESENCE_UPDATE: u8 = 9;

// ---------------------------------------------------------------------------
// Server → Client message
// ---------------------------------------------------------------------------

/// A message sent from the server to the client over WebSocket.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayMessage {
    pub op: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<u64>,
    pub d: Value,
}

impl GatewayMessage {
    /// Build a DISPATCH message (op=0).
    pub fn dispatch(event_name: &str, seq: u64, data: Value) -> Self {
        Self {
            op: OP_DISPATCH,
            t: Some(event_name.to_string()),
            s: Some(seq),
            d: data,
        }
    }

    /// Build a RECONNECT message (op=7) telling the client to re-IDENTIFY.
    pub fn reconnect(reason: &str) -> Self {
        Self {
            op: OP_RECONNECT,
            t: None,
            s: None,
            d: serde_json::json!({ "reason": reason }),
        }
    }

    /// Build a HEARTBEAT_ACK message (op=6).
    pub fn heartbeat_ack(seq: u64) -> Self {
        Self {
            op: OP_HEARTBEAT_ACK,
            t: None,
            s: None,
            d: serde_json::json!({ "ack": seq }),
        }
    }
}

// ---------------------------------------------------------------------------
// Client → Server message
// ---------------------------------------------------------------------------

/// A message received from the client over WebSocket.
#[derive(Debug, Deserialize)]
pub struct ClientMessage {
    pub op: u8,
    #[serde(default)]
    pub t: Option<String>,
    #[serde(default)]
    pub d: Value,
}

// ---------------------------------------------------------------------------
// IDENTIFY payload
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct IdentifyPayload {
    pub ticket: String,
}

// ---------------------------------------------------------------------------
// RESUME payload
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ResumePayload {
    pub session_id: String,
    pub token: String,
    pub seq: u64,
}

// ---------------------------------------------------------------------------
// HEARTBEAT payload
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct HeartbeatPayload {
    #[serde(default)]
    pub seq: u64,
}

// ---------------------------------------------------------------------------
// TYPING payload
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TypingPayload {
    pub channel_id: String,
}

// ---------------------------------------------------------------------------
// PRESENCE_UPDATE payload
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PresenceUpdatePayload {
    pub status: String,
}

// ---------------------------------------------------------------------------
// Dispatch event types
// ---------------------------------------------------------------------------

/// Event names dispatched to clients.
pub struct EventName;

impl EventName {
    pub const READY: &'static str = "READY";
    pub const MESSAGE_CREATE: &'static str = "MESSAGE_CREATE";
    pub const MESSAGE_UPDATE: &'static str = "MESSAGE_UPDATE";
    pub const MESSAGE_DELETE: &'static str = "MESSAGE_DELETE";
    pub const MESSAGE_REACTION_ADD: &'static str = "MESSAGE_REACTION_ADD";
    pub const MESSAGE_REACTION_REMOVE: &'static str = "MESSAGE_REACTION_REMOVE";
    pub const CHANNEL_CREATE: &'static str = "CHANNEL_CREATE";
    pub const CHANNEL_UPDATE: &'static str = "CHANNEL_UPDATE";
    pub const CHANNEL_DELETE: &'static str = "CHANNEL_DELETE";
    pub const COMMUNITY_UPDATE: &'static str = "COMMUNITY_UPDATE";
    pub const MEMBER_JOIN: &'static str = "MEMBER_JOIN";
    pub const MEMBER_LEAVE: &'static str = "MEMBER_LEAVE";
    pub const MEMBER_UPDATE: &'static str = "MEMBER_UPDATE";
    pub const RESUMED: &'static str = "RESUMED";
    pub const TYPING_START: &'static str = "TYPING_START";
    pub const CHANNEL_PINS_UPDATE: &'static str = "CHANNEL_PINS_UPDATE";
    pub const PRESENCE_UPDATE: &'static str = "PRESENCE_UPDATE";
}
