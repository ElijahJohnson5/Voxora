//! RESUME opcode handler — validates token, looks up session, returns replay events.

use crate::auth::tokens;
use crate::AppState;

use super::events::ResumePayload;
use super::registry::ReplayEntry;
use super::session::GatewaySession;

/// Process a RESUME opcode.
///
/// On success, returns a reconstructed `GatewaySession` and the list of events
/// to replay (everything the client missed since `payload.seq`).
///
/// On failure, returns a static error string that the caller sends as a
/// RECONNECT message before closing.
pub async fn handle_resume(
    state: &AppState,
    payload: ResumePayload,
) -> Result<(GatewaySession, Vec<ReplayEntry>), &'static str> {
    // 1. Validate the PAT (non-destructive lookup).
    let pat_data = tokens::lookup_pat(state.kv.as_ref(), &payload.token)
        .await
        .map_err(|_| "Token lookup failed")?
        .ok_or("Invalid or expired token")?;

    // 2. Look up the session in the registry.
    let (session_user_id, communities, seq) = state
        .sessions
        .get_session_info(&payload.session_id)
        .ok_or("Session not found")?;

    // 3. Verify the token's user matches the session's user.
    if pat_data.user_id != session_user_id {
        return Err("Token user mismatch");
    }

    // 4. Replay events after the client's last seq.
    let replay = state
        .sessions
        .replay_after(&payload.session_id, payload.seq)
        .ok_or("Sequence too old — please re-identify")?;

    // 5. Reconstruct the session with the registry's current seq.
    let session = GatewaySession::with_seq(
        payload.session_id.clone(),
        session_user_id,
        communities,
        seq,
    );

    // 6. Mark session as connected.
    state.sessions.mark_connected(&payload.session_id);

    Ok((session, replay))
}
