//! Pod Access Token (PAT), Refresh Token, and WebSocket ticket management.

use serde::{Deserialize, Serialize};

use crate::db::kv::KeyValueStore;
use crate::error::ApiError;

// ---------------------------------------------------------------------------
// Opaque token generation
// ---------------------------------------------------------------------------

/// Generate an opaque random token with the given prefix.
pub fn generate_opaque_token(prefix: &str, bytes: usize) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use rand::Rng;
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill(&mut buf[..]);
    format!("{}_{}", prefix, URL_SAFE_NO_PAD.encode(&buf))
}

// ---------------------------------------------------------------------------
// PAT (Pod Access Token) — 1-hour TTL
// ---------------------------------------------------------------------------

/// PAT TTL in seconds (1 hour).
pub const PAT_TTL_SECS: u64 = 3600;

/// Data stored alongside a PAT.
#[derive(Debug, Serialize, Deserialize)]
pub struct PatData {
    pub user_id: String,
}

pub fn generate_pat() -> String {
    generate_opaque_token("pat", 32)
}

pub async fn store_pat(
    kv: &dyn KeyValueStore,
    token: &str,
    data: &PatData,
) -> Result<(), ApiError> {
    let key = format!("pod:pat:{}", token);
    let value = serde_json::to_string(data).map_err(|_| ApiError::internal("serialization"))?;
    kv.set_ex(&key, &value, PAT_TTL_SECS).await
}

pub async fn lookup_pat(
    kv: &dyn KeyValueStore,
    token: &str,
) -> Result<Option<PatData>, ApiError> {
    let key = format!("pod:pat:{}", token);
    match kv.get(&key).await? {
        Some(v) => {
            let data: PatData =
                serde_json::from_str(&v).map_err(|_| ApiError::internal("corrupt token data"))?;
            Ok(Some(data))
        }
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Refresh Token — 30-day TTL
// ---------------------------------------------------------------------------

/// Refresh token TTL in seconds (30 days).
pub const REFRESH_TTL_SECS: u64 = 30 * 24 * 3600;

/// Data stored alongside a refresh token.
#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshData {
    pub user_id: String,
}

pub fn generate_refresh_token() -> String {
    generate_opaque_token("prt", 32)
}

pub async fn store_refresh_token(
    kv: &dyn KeyValueStore,
    token: &str,
    data: &RefreshData,
) -> Result<(), ApiError> {
    let key = format!("pod:rt:{}", token);
    let value = serde_json::to_string(data).map_err(|_| ApiError::internal("serialization"))?;
    kv.set_ex(&key, &value, REFRESH_TTL_SECS).await
}

pub async fn consume_refresh_token(
    kv: &dyn KeyValueStore,
    token: &str,
) -> Result<Option<RefreshData>, ApiError> {
    let key = format!("pod:rt:{}", token);
    let val = kv.get(&key).await?;
    if val.is_some() {
        let _ = kv.del(&key).await;
    }
    match val {
        Some(v) => {
            let data: RefreshData =
                serde_json::from_str(&v).map_err(|_| ApiError::internal("corrupt token data"))?;
            Ok(Some(data))
        }
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// WebSocket Ticket — 30-second TTL, single-use
// ---------------------------------------------------------------------------

/// WS ticket TTL in seconds.
pub const WS_TICKET_TTL_SECS: u64 = 30;

/// Data stored alongside a WS ticket.
#[derive(Debug, Serialize, Deserialize)]
pub struct WsTicketData {
    pub user_id: String,
}

pub fn generate_ws_ticket() -> String {
    generate_opaque_token("wst", 32)
}

pub async fn store_ws_ticket(
    kv: &dyn KeyValueStore,
    ticket: &str,
    data: &WsTicketData,
) -> Result<(), ApiError> {
    let key = format!("pod:wst:{}", ticket);
    let value = serde_json::to_string(data).map_err(|_| ApiError::internal("serialization"))?;
    kv.set_ex(&key, &value, WS_TICKET_TTL_SECS).await
}
