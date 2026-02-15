//! WebSocket upgrade handler and per-connection event loop.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tokio::time;

use crate::AppState;

use super::events::{
    ClientMessage, EventName, GatewayMessage, HeartbeatPayload, IdentifyPayload, ResumePayload,
    OP_HEARTBEAT, OP_IDENTIFY, OP_RESUME,
};
use super::fanout::BroadcastPayload;
use super::handler::{handle_identify, HEARTBEAT_INTERVAL_MS};
use super::registry::SessionRegistry;
use super::resume::handle_resume;
use super::session::GatewaySession;

/// Close codes (4000-range for application-level).
const CLOSE_UNKNOWN_ERROR: u16 = 4000;
const CLOSE_UNKNOWN_OPCODE: u16 = 4001;
const CLOSE_NOT_AUTHENTICATED: u16 = 4003;
const CLOSE_AUTH_FAILED: u16 = 4004;
const CLOSE_SESSION_TIMEOUT: u16 = 4009;

/// Timeout for receiving IDENTIFY/RESUME after connection (seconds).
const IDENTIFY_TIMEOUT_SECS: u64 = 10;

/// The initial opcode parsed from the client's first message.
enum InitialOp {
    Identify(IdentifyPayload),
    Resume(ResumePayload),
}

pub fn router() -> Router<AppState> {
    Router::new().route("/gateway", get(ws_upgrade))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_connection(socket, state))
}

async fn handle_connection(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Step 1: Wait for IDENTIFY or RESUME within timeout.
    let initial_result = time::timeout(Duration::from_secs(IDENTIFY_TIMEOUT_SECS), async {
        while let Some(msg) = ws_rx.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    tracing::debug!(?e, "ws read error during identify");
                    return Err("read error");
                }
            };

            let text = match msg {
                Message::Text(t) => t,
                Message::Close(_) => return Err("client closed"),
                Message::Ping(_) | Message::Pong(_) => continue,
                _ => continue,
            };

            let client_msg: ClientMessage = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(_) => {
                    let _ = send_close(&mut ws_tx, CLOSE_UNKNOWN_ERROR, "Invalid JSON").await;
                    return Err("invalid json");
                }
            };

            match client_msg.op {
                OP_IDENTIFY => {
                    let payload: IdentifyPayload = serde_json::from_value(client_msg.d)
                        .map_err(|_| "invalid identify payload")?;
                    return Ok(InitialOp::Identify(payload));
                }
                OP_RESUME => {
                    let payload: ResumePayload = serde_json::from_value(client_msg.d)
                        .map_err(|_| "invalid resume payload")?;
                    return Ok(InitialOp::Resume(payload));
                }
                _ => {
                    let _ =
                        send_close(&mut ws_tx, CLOSE_NOT_AUTHENTICATED, "Expected IDENTIFY or RESUME")
                            .await;
                    return Err("expected identify or resume");
                }
            }
        }
        Err("connection closed before identify")
    })
    .await;

    let initial_op = match initial_result {
        Ok(Ok(op)) => op,
        Ok(Err(reason)) => {
            tracing::debug!(%reason, "initial handshake failed");
            let _ = send_close(&mut ws_tx, CLOSE_AUTH_FAILED, reason).await;
            return;
        }
        Err(_timeout) => {
            let _ = send_close(&mut ws_tx, CLOSE_SESSION_TIMEOUT, "Handshake timeout").await;
            return;
        }
    };

    match initial_op {
        InitialOp::Identify(payload) => {
            handle_identify_path(&state, payload, ws_tx, ws_rx).await;
        }
        InitialOp::Resume(payload) => {
            handle_resume_path(&state, payload, ws_tx, ws_rx).await;
        }
    }
}

/// IDENTIFY path — same as before, plus registry integration.
async fn handle_identify_path(
    state: &AppState,
    payload: IdentifyPayload,
    mut ws_tx: futures_util::stream::SplitSink<WebSocket, Message>,
    ws_rx: futures_util::stream::SplitStream<WebSocket>,
) {
    let (session, ready_msg) = match handle_identify(state, payload).await {
        Ok(result) => result,
        Err(reason) => {
            tracing::debug!(%reason, "identify handler failed");
            let _ = send_close(&mut ws_tx, CLOSE_AUTH_FAILED, reason).await;
            return;
        }
    };

    tracing::info!(
        session_id = %session.session_id,
        user_id = %session.user_id,
        communities = session.communities.len(),
        "gateway session established"
    );

    // Send READY.
    let ready_json = serde_json::to_string(&ready_msg).unwrap();
    if ws_tx.send(Message::Text(ready_json.into())).await.is_err() {
        return;
    }

    // Run the main event loop.
    let session = Arc::new(session);
    let broadcast_rx = state.broadcast.subscribe();
    let registry = state.sessions.clone();
    run_session(session.clone(), ws_tx, ws_rx, broadcast_rx, registry.clone()).await;

    // Mark session as disconnected for resume support.
    registry.mark_disconnected(&session.session_id);

    tracing::info!(
        session_id = %session.session_id,
        user_id = %session.user_id,
        "gateway session ended"
    );
}

/// RESUME path — validate, replay missed events, then enter normal event loop.
async fn handle_resume_path(
    state: &AppState,
    payload: ResumePayload,
    mut ws_tx: futures_util::stream::SplitSink<WebSocket, Message>,
    ws_rx: futures_util::stream::SplitStream<WebSocket>,
) {
    let (session, replay_events) = match handle_resume(state, payload).await {
        Ok(result) => result,
        Err(reason) => {
            tracing::debug!(%reason, "resume handler failed");
            let reconnect = GatewayMessage::reconnect(reason);
            let json = serde_json::to_string(&reconnect).unwrap();
            let _ = ws_tx.send(Message::Text(json.into())).await;
            let _ = send_close(&mut ws_tx, CLOSE_AUTH_FAILED, reason).await;
            return;
        }
    };

    tracing::info!(
        session_id = %session.session_id,
        user_id = %session.user_id,
        replayed = replay_events.len(),
        "gateway session resumed"
    );

    // Subscribe to broadcasts before sending replayed events so we don't miss
    // anything that arrives concurrently.
    let broadcast_rx = state.broadcast.subscribe();

    // Replay missed events.
    for entry in &replay_events {
        let msg = GatewayMessage::dispatch(&entry.event_name, entry.seq, entry.data.clone());
        let json = serde_json::to_string(&msg).unwrap();
        if ws_tx.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

    // Send RESUMED dispatch.
    let session = Arc::new(session);
    let seq = session.next_seq();
    let resumed_msg = GatewayMessage::dispatch(EventName::RESUMED, seq, serde_json::json!({}));
    let json = serde_json::to_string(&resumed_msg).unwrap();
    if ws_tx.send(Message::Text(json.into())).await.is_err() {
        return;
    }

    // Enter the normal event loop.
    let registry = state.sessions.clone();
    run_session(session.clone(), ws_tx, ws_rx, broadcast_rx, registry.clone()).await;

    // Mark session as disconnected again.
    registry.mark_disconnected(&session.session_id);

    tracing::info!(
        session_id = %session.session_id,
        user_id = %session.user_id,
        "gateway session ended (after resume)"
    );
}

/// Main session event loop: read client messages, forward broadcasts, enforce heartbeat.
async fn run_session(
    session: Arc<GatewaySession>,
    mut ws_tx: futures_util::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures_util::stream::SplitStream<WebSocket>,
    mut broadcast_rx: broadcast::Receiver<Arc<BroadcastPayload>>,
    registry: Arc<SessionRegistry>,
) {
    // Heartbeat deadline: client must heartbeat within 1.5× the interval.
    let heartbeat_deadline = Duration::from_millis(HEARTBEAT_INTERVAL_MS * 3 / 2);
    let mut heartbeat_timer = time::interval(heartbeat_deadline);
    heartbeat_timer.tick().await; // First tick fires immediately; skip it.
    let mut got_heartbeat = true;

    loop {
        tokio::select! {
            // Client sends us a message.
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let client_msg: ClientMessage = match serde_json::from_str(&text) {
                            Ok(m) => m,
                            Err(_) => {
                                let _ = send_close(&mut ws_tx, CLOSE_UNKNOWN_ERROR, "Invalid JSON").await;
                                break;
                            }
                        };

                        match client_msg.op {
                            OP_HEARTBEAT => {
                                got_heartbeat = true;
                                let payload: HeartbeatPayload =
                                    serde_json::from_value(client_msg.d).unwrap_or(HeartbeatPayload { seq: 0 });
                                let ack = GatewayMessage::heartbeat_ack(payload.seq);
                                let json = serde_json::to_string(&ack).unwrap();
                                if ws_tx.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            OP_IDENTIFY => {
                                // Already identified.
                                let _ = send_close(&mut ws_tx, CLOSE_UNKNOWN_ERROR, "Already identified").await;
                                break;
                            }
                            _ => {
                                let _ = send_close(&mut ws_tx, CLOSE_UNKNOWN_OPCODE, "Unknown opcode").await;
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::debug!(?e, session_id = %session.session_id, "ws read error");
                        break;
                    }
                    _ => continue,
                }
            }

            // Broadcast event from the fanout hub.
            result = broadcast_rx.recv() => {
                match result {
                    Ok(payload) => {
                        if !session.is_subscribed(&payload.community_id) {
                            continue;
                        }

                        let seq = session.next_seq();
                        let msg = GatewayMessage::dispatch(&payload.event_name, seq, payload.data.clone());
                        let json = serde_json::to_string(&msg).unwrap();
                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }

                        // Write to replay buffer for resume support.
                        registry.append_event(
                            &session.session_id,
                            seq,
                            &payload.event_name,
                            payload.data.clone(),
                        );
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            session_id = %session.session_id,
                            skipped = n,
                            "gateway session lagged behind broadcast"
                        );
                        // Continue — we just drop the missed events.
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // Heartbeat timeout check.
            _ = heartbeat_timer.tick() => {
                if !got_heartbeat {
                    tracing::debug!(
                        session_id = %session.session_id,
                        "heartbeat timeout — closing connection"
                    );
                    let _ = send_close(&mut ws_tx, CLOSE_SESSION_TIMEOUT, "Heartbeat timeout").await;
                    break;
                }
                got_heartbeat = false;
            }
        }
    }
}

/// Send a WebSocket close frame with a code and reason.
async fn send_close(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    _code: u16,
    reason: &str,
) -> Result<(), axum::Error> {
    let close_msg = Message::Close(Some(axum::extract::ws::CloseFrame {
        code: _code,
        reason: reason.to_string().into(),
    }));
    ws_tx.send(close_msg).await
}
