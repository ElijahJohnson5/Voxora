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
    ClientMessage, GatewayMessage, HeartbeatPayload, IdentifyPayload, OP_HEARTBEAT, OP_IDENTIFY,
};
use super::fanout::BroadcastPayload;
use super::handler::{handle_identify, HEARTBEAT_INTERVAL_MS};
use super::session::GatewaySession;

/// Close codes (4000-range for application-level).
const CLOSE_UNKNOWN_ERROR: u16 = 4000;
const CLOSE_UNKNOWN_OPCODE: u16 = 4001;
const CLOSE_NOT_AUTHENTICATED: u16 = 4003;
const CLOSE_AUTH_FAILED: u16 = 4004;
const CLOSE_SESSION_TIMEOUT: u16 = 4009;

/// Timeout for receiving IDENTIFY after connection (seconds).
const IDENTIFY_TIMEOUT_SECS: u64 = 10;

pub fn router() -> Router<AppState> {
    Router::new().route("/gateway", get(ws_upgrade))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_connection(socket, state))
}

async fn handle_connection(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Step 1: Wait for IDENTIFY within timeout.
    let identify_result = time::timeout(Duration::from_secs(IDENTIFY_TIMEOUT_SECS), async {
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

            if client_msg.op != OP_IDENTIFY {
                let _ = send_close(&mut ws_tx, CLOSE_NOT_AUTHENTICATED, "Expected IDENTIFY").await;
                return Err("expected identify");
            }

            let payload: IdentifyPayload =
                serde_json::from_value(client_msg.d).map_err(|_| "invalid identify payload")?;

            return Ok(payload);
        }
        Err("connection closed before identify")
    })
    .await;

    let identify_payload = match identify_result {
        Ok(Ok(p)) => p,
        Ok(Err(reason)) => {
            tracing::debug!(%reason, "identify failed");
            let _ = send_close(&mut ws_tx, CLOSE_AUTH_FAILED, reason).await;
            return;
        }
        Err(_timeout) => {
            let _ = send_close(&mut ws_tx, CLOSE_SESSION_TIMEOUT, "IDENTIFY timeout").await;
            return;
        }
    };

    // Step 2: Process IDENTIFY — validate ticket, build READY.
    let (session, ready_msg) = match handle_identify(&state, identify_payload).await {
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

    // Step 3: Run the main event loop.
    let session = Arc::new(session);
    let broadcast_rx = state.broadcast.subscribe();
    run_session(session, ws_tx, ws_rx, broadcast_rx).await;
}

/// Main session event loop: read client messages, forward broadcasts, enforce heartbeat.
async fn run_session(
    session: Arc<GatewaySession>,
    mut ws_tx: futures_util::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures_util::stream::SplitStream<WebSocket>,
    mut broadcast_rx: broadcast::Receiver<Arc<BroadcastPayload>>,
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

    tracing::info!(
        session_id = %session.session_id,
        user_id = %session.user_id,
        "gateway session ended"
    );
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
