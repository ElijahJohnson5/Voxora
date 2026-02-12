mod common;

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::time;
use tokio_tungstenite::tungstenite;

/// Helper: start an actual TCP server for WebSocket testing.
/// Returns (addr, state, keys). The server runs in the background.
async fn start_ws_server() -> (SocketAddr, pod_api::AppState, common::TestSigningKeys) {
    let (state, keys) = common::test_state().await;
    let app = pod_api::routes::router().with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (addr, state, keys)
}

/// Helper: log in a user and return the ws_ticket.
async fn login_and_get_ticket(
    addr: SocketAddr,
    keys: &common::TestSigningKeys,
    config: &pod_api::config::Config,
    user_id: &str,
    username: &str,
) -> String {
    let sia = common::mint_test_sia(
        keys,
        &config.hub_url,
        user_id,
        &config.pod_id,
        username,
        username,
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/v1/auth/login"))
        .json(&serde_json::json!({ "sia": sia }))
        .send()
        .await
        .expect("login request");

    let body: serde_json::Value = resp.json().await.expect("parse login response");
    body["ws_ticket"]
        .as_str()
        .expect("ws_ticket present")
        .to_string()
}

/// Helper: connect to the gateway WebSocket and send IDENTIFY.
/// Returns the WebSocket stream after receiving READY.
async fn connect_and_identify(
    addr: SocketAddr,
    ticket: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://{addr}/gateway");
    let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");

    let (mut write, mut read) = ws_stream.split();

    // Send IDENTIFY.
    let identify = serde_json::json!({
        "op": 2,
        "d": { "ticket": ticket }
    });
    write
        .send(tungstenite::Message::Text(identify.to_string().into()))
        .await
        .expect("send identify");

    // Read READY.
    let msg = time::timeout(Duration::from_secs(5), read.next())
        .await
        .expect("timeout waiting for READY")
        .expect("stream ended")
        .expect("ws read error");

    let text = msg.into_text().expect("not text");
    let ready: serde_json::Value = serde_json::from_str(&text).expect("parse READY");
    assert_eq!(ready["op"], 0, "READY should be op=0 (DISPATCH)");
    assert_eq!(ready["t"], "READY");
    assert!(ready["s"].as_u64().unwrap() > 0);

    // Reunite the stream.
    read.reunite(write).expect("reunite")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gateway_identify_returns_ready() {
    let (addr, state, keys) = start_ws_server().await;
    let user_id = voxora_common::id::prefixed_ulid("usr");

    let ticket = login_and_get_ticket(addr, &keys, &state.config, &user_id, "gw_user1").await;

    let url = format!("ws://{addr}/gateway");
    let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");

    let (mut write, mut read) = ws_stream.split();

    // Send IDENTIFY.
    let identify = serde_json::json!({
        "op": 2,
        "d": { "ticket": ticket }
    });
    write
        .send(tungstenite::Message::Text(identify.to_string().into()))
        .await
        .expect("send identify");

    // Read READY.
    let msg = time::timeout(Duration::from_secs(5), read.next())
        .await
        .expect("timeout")
        .expect("stream ended")
        .expect("read error");

    let text = msg.into_text().expect("not text");
    let ready: serde_json::Value = serde_json::from_str(&text).expect("parse READY");
    assert_eq!(ready["op"], 0);
    assert_eq!(ready["t"], "READY");
    assert_eq!(ready["s"], 1);

    let d = &ready["d"];
    assert!(d["session_id"].as_str().unwrap().starts_with("gw_"));
    assert_eq!(d["user"]["id"], user_id);
    assert_eq!(d["user"]["username"], "gw_user1");
    assert!(d["heartbeat_interval"].as_u64().unwrap() > 0);
    assert!(d["communities"].is_array());

    // Cleanup.
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn gateway_rejects_invalid_ticket() {
    let (addr, _state, _keys) = start_ws_server().await;

    let url = format!("ws://{addr}/gateway");
    let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");

    let (mut write, mut read) = ws_stream.split();

    // Send IDENTIFY with a bad ticket.
    let identify = serde_json::json!({
        "op": 2,
        "d": { "ticket": "wst_bogus" }
    });
    write
        .send(tungstenite::Message::Text(identify.to_string().into()))
        .await
        .expect("send identify");

    // Should get a close frame.
    let msg = time::timeout(Duration::from_secs(5), read.next())
        .await
        .expect("timeout")
        .expect("stream ended")
        .expect("read error");

    match msg {
        tungstenite::Message::Close(Some(frame)) => {
            assert_eq!(
                frame.code,
                tungstenite::protocol::frame::coding::CloseCode::from(4004)
            );
        }
        tungstenite::Message::Close(None) => {
            // Also acceptable.
        }
        other => {
            panic!("Expected Close frame, got: {other:?}");
        }
    }
}

#[tokio::test]
async fn gateway_ticket_is_single_use() {
    let (addr, state, keys) = start_ws_server().await;
    let user_id = voxora_common::id::prefixed_ulid("usr");

    let ticket = login_and_get_ticket(addr, &keys, &state.config, &user_id, "gw_single").await;

    // First connection should succeed.
    let ws = connect_and_identify(addr, &ticket).await;
    drop(ws);

    // Small delay.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Second connection with the same ticket should fail.
    let url = format!("ws://{addr}/gateway");
    let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");

    let (mut write, mut read) = ws_stream.split();

    let identify = serde_json::json!({
        "op": 2,
        "d": { "ticket": ticket }
    });
    write
        .send(tungstenite::Message::Text(identify.to_string().into()))
        .await
        .expect("send identify");

    let msg = time::timeout(Duration::from_secs(5), read.next())
        .await
        .expect("timeout")
        .expect("stream ended")
        .expect("read error");

    match msg {
        tungstenite::Message::Close(Some(frame)) => {
            assert_eq!(
                frame.code,
                tungstenite::protocol::frame::coding::CloseCode::from(4004)
            );
        }
        tungstenite::Message::Close(None) => {}
        other => panic!("Expected Close frame, got: {other:?}"),
    }

    // Cleanup.
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn gateway_heartbeat_returns_ack() {
    let (addr, state, keys) = start_ws_server().await;
    let user_id = voxora_common::id::prefixed_ulid("usr");

    let ticket = login_and_get_ticket(addr, &keys, &state.config, &user_id, "gw_hb").await;
    let ws = connect_and_identify(addr, &ticket).await;
    let (mut write, mut read) = ws.split();

    // Send HEARTBEAT (op=1).
    let heartbeat = serde_json::json!({
        "op": 1,
        "d": { "seq": 1 }
    });
    write
        .send(tungstenite::Message::Text(heartbeat.to_string().into()))
        .await
        .expect("send heartbeat");

    // Read HEARTBEAT_ACK (op=6).
    let msg = time::timeout(Duration::from_secs(5), read.next())
        .await
        .expect("timeout")
        .expect("stream ended")
        .expect("read error");

    let text = msg.into_text().expect("not text");
    let ack: serde_json::Value = serde_json::from_str(&text).expect("parse ack");
    assert_eq!(ack["op"], 6);
    assert_eq!(ack["d"]["ack"], 1);

    // Cleanup.
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn gateway_unknown_opcode_closes_connection() {
    let (addr, state, keys) = start_ws_server().await;
    let user_id = voxora_common::id::prefixed_ulid("usr");

    let ticket = login_and_get_ticket(addr, &keys, &state.config, &user_id, "gw_unk").await;
    let ws = connect_and_identify(addr, &ticket).await;
    let (mut write, mut read) = ws.split();

    // Send an unknown opcode (op=99).
    let unknown = serde_json::json!({ "op": 99, "d": {} });
    write
        .send(tungstenite::Message::Text(unknown.to_string().into()))
        .await
        .expect("send unknown");

    // Should get a close frame.
    let msg = time::timeout(Duration::from_secs(5), read.next())
        .await
        .expect("timeout")
        .expect("stream ended")
        .expect("read error");

    match msg {
        tungstenite::Message::Close(Some(frame)) => {
            assert_eq!(
                frame.code,
                tungstenite::protocol::frame::coding::CloseCode::from(4001)
            );
        }
        tungstenite::Message::Close(None) => {}
        other => panic!("Expected Close frame, got: {other:?}"),
    }

    // Cleanup.
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn gateway_receives_message_create_event() {
    let (addr, state, keys) = start_ws_server().await;
    let user_id = voxora_common::id::prefixed_ulid("usr");

    // Login to get both a PAT and a WS ticket.
    let sia = common::mint_test_sia(
        &keys,
        &state.config.hub_url,
        &user_id,
        &state.config.pod_id,
        "gw_msg_user",
        "gw_msg_user",
    );
    let client = reqwest::Client::new();
    let login_resp = client
        .post(format!("http://{addr}/api/v1/auth/login"))
        .json(&serde_json::json!({ "sia": sia }))
        .send()
        .await
        .unwrap();
    let login_body: serde_json::Value = login_resp.json().await.unwrap();
    let token = login_body["access_token"].as_str().unwrap().to_string();
    let ticket = login_body["ws_ticket"].as_str().unwrap().to_string();

    // Create a community and get the default channel.
    let create_resp = client
        .post(format!("http://{addr}/api/v1/communities"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "GW Test Community" }))
        .send()
        .await
        .unwrap();
    let community: serde_json::Value = create_resp.json().await.unwrap();
    let community_id = community["id"].as_str().unwrap().to_string();
    let channel_id = community["channels"][0]["id"].as_str().unwrap().to_string();

    // Connect to the gateway.
    let ws = connect_and_identify(addr, &ticket).await;
    let (_write, mut read) = ws.split();

    // Send a message via REST.
    let _msg_resp = client
        .post(format!(
            "http://{addr}/api/v1/channels/{channel_id}/messages"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Hello from gateway test!" }))
        .send()
        .await
        .unwrap();

    // The gateway should dispatch MESSAGE_CREATE.
    let msg = time::timeout(Duration::from_secs(5), read.next())
        .await
        .expect("timeout waiting for MESSAGE_CREATE")
        .expect("stream ended")
        .expect("read error");

    let text = msg.into_text().expect("not text");
    let event: serde_json::Value = serde_json::from_str(&text).expect("parse event");
    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "MESSAGE_CREATE");
    assert_eq!(
        event["d"]["content"].as_str().unwrap(),
        "Hello from gateway test!"
    );
    assert_eq!(event["d"]["channel_id"].as_str().unwrap(), channel_id);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn gateway_ready_includes_community_data() {
    let (addr, state, keys) = start_ws_server().await;
    let user_id = voxora_common::id::prefixed_ulid("usr");

    // Login to get token.
    let sia = common::mint_test_sia(
        &keys,
        &state.config.hub_url,
        &user_id,
        &state.config.pod_id,
        "gw_ready_user",
        "gw_ready_user",
    );
    let client = reqwest::Client::new();
    let login_resp = client
        .post(format!("http://{addr}/api/v1/auth/login"))
        .json(&serde_json::json!({ "sia": sia }))
        .send()
        .await
        .unwrap();
    let login_body: serde_json::Value = login_resp.json().await.unwrap();
    let token = login_body["access_token"].as_str().unwrap().to_string();
    let ticket = login_body["ws_ticket"].as_str().unwrap().to_string();

    // Create a community.
    let create_resp = client
        .post(format!("http://{addr}/api/v1/communities"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "GW Ready Community",
            "description": "Test"
        }))
        .send()
        .await
        .unwrap();
    let community: serde_json::Value = create_resp.json().await.unwrap();
    let community_id = community["id"].as_str().unwrap().to_string();

    // Connect and identify — the READY should include our community.
    let url = format!("ws://{addr}/gateway");
    let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");

    let (mut write, mut read) = ws_stream.split();

    let identify = serde_json::json!({
        "op": 2,
        "d": { "ticket": ticket }
    });
    write
        .send(tungstenite::Message::Text(identify.to_string().into()))
        .await
        .expect("send identify");

    let msg = time::timeout(Duration::from_secs(5), read.next())
        .await
        .expect("timeout")
        .expect("stream ended")
        .expect("read error");

    let text = msg.into_text().expect("not text");
    let ready: serde_json::Value = serde_json::from_str(&text).expect("parse READY");
    assert_eq!(ready["t"], "READY");

    let communities = ready["d"]["communities"].as_array().unwrap();
    assert!(!communities.is_empty(), "READY should include communities");

    let c = &communities[0];
    assert_eq!(c["name"].as_str().unwrap(), "GW Ready Community");
    assert!(c["channels"].is_array());
    assert!(!c["channels"].as_array().unwrap().is_empty());
    assert!(c["roles"].is_array());
    assert!(!c["roles"].as_array().unwrap().is_empty());

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}
