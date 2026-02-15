mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

/// Helper: create a community, get its default channel, send a message, return IDs + token.
async fn setup_with_message(
    server: &TestServer,
    keys: &common::TestSigningKeys,
    config: &pod_api::config::Config,
    user_id: &str,
    username: &str,
) -> (String, String, String, String) {
    let token = common::login_test_user(server, keys, config, user_id, username).await;

    // Create community.
    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Pin Test Community" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let community: serde_json::Value = resp.json();
    let community_id = community["id"].as_str().unwrap().to_string();

    // Get default channel.
    let channels_resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    let channels: Vec<serde_json::Value> = channels_resp.json();
    let channel_id = channels[0]["id"].as_str().unwrap().to_string();

    // Send a message.
    let msg_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Pin me!" }))
        .await;
    msg_resp.assert_status(StatusCode::CREATED);
    let msg: serde_json::Value = msg_resp.json();
    let message_id = msg["id"].as_str().unwrap().to_string();

    (community_id, channel_id, message_id, token)
}

/// Helper: send a message in a channel, return message ID as string.
async fn send_message(
    server: &TestServer,
    channel_id: &str,
    token: &str,
    content: &str,
) -> String {
    let resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": content }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let msg: serde_json::Value = resp.json();
    msg["id"].as_str().unwrap().to_string()
}

// ===========================================================================
// PUT /api/v1/channels/:channel_id/pins/:message_id
// ===========================================================================

#[tokio::test]
async fn pin_message_success() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, token) =
        setup_with_message(&server, &keys, &state.config, &user_id, "pin_ok").await;

    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/pins/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["pinned"], true);
    assert_eq!(body["id"].as_str().unwrap(), message_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn pin_message_requires_auth() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, _token) =
        setup_with_message(&server, &keys, &state.config, &user_id, "pin_noauth").await;

    // No auth header.
    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/pins/{message_id}"
        ))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn pin_message_requires_manage_messages_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community + message.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, _owner_token) =
        setup_with_message(&server, &keys, &state.config, &owner_id, "pin_owner").await;

    // Non-member tries to pin.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "pin_outsider").await;

    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/pins/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

#[tokio::test]
async fn pin_message_not_found() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token =
        common::login_test_user(&server, &keys, &state.config, &user_id, "pin_404").await;

    // Create community to get a valid channel.
    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Pin 404 Community" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let community: serde_json::Value = resp.json();
    let community_id = community["id"].as_str().unwrap().to_string();

    let channels_resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    let channels: Vec<serde_json::Value> = channels_resp.json();
    let channel_id = channels[0]["id"].as_str().unwrap().to_string();

    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/pins/9999999999"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn pin_message_wrong_channel() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, message_id, token) =
        setup_with_message(&server, &keys, &state.config, &user_id, "pin_wrongch").await;

    // Create a second channel.
    let ch_resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "second-channel" }))
        .await;
    ch_resp.assert_status(StatusCode::CREATED);
    let channel_b: serde_json::Value = ch_resp.json();
    let channel_b_id = channel_b["id"].as_str().unwrap().to_string();

    // Try to pin message (from channel_a) via channel_b.
    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_b_id}/pins/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn pin_message_max_50() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token =
        common::login_test_user(&server, &keys, &state.config, &user_id, "pin_max50").await;

    // Create community.
    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Pin Max Community" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let community: serde_json::Value = resp.json();
    let community_id = community["id"].as_str().unwrap().to_string();

    let channels_resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    let channels: Vec<serde_json::Value> = channels_resp.json();
    let channel_id = channels[0]["id"].as_str().unwrap().to_string();

    // Send 51 messages and pin the first 50.
    let mut message_ids = Vec::new();
    for i in 0..51 {
        let mid = send_message(&server, &channel_id, &token, &format!("msg {i}")).await;
        message_ids.push(mid);
    }

    for mid in &message_ids[..50] {
        let resp = server
            .put(&format!("/api/v1/channels/{channel_id}/pins/{mid}"))
            .add_header(AUTHORIZATION, format!("Bearer {token}"))
            .await;
        resp.assert_status_ok();
    }

    // 51st pin should fail with 400.
    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/pins/{}",
            message_ids[50]
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn pin_message_idempotent() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, token) =
        setup_with_message(&server, &keys, &state.config, &user_id, "pin_idem").await;

    let url = format!("/api/v1/channels/{channel_id}/pins/{message_id}");

    // Pin twice.
    let resp1 = server
        .put(&url)
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp1.assert_status_ok();

    let resp2 = server
        .put(&url)
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp2.assert_status_ok();

    // Both should indicate pinned.
    let body1: serde_json::Value = resp1.json();
    let body2: serde_json::Value = resp2.json();
    assert_eq!(body1["pinned"], true);
    assert_eq!(body2["pinned"], true);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

// ===========================================================================
// DELETE /api/v1/channels/:channel_id/pins/:message_id
// ===========================================================================

#[tokio::test]
async fn unpin_message_success() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, token) =
        setup_with_message(&server, &keys, &state.config, &user_id, "unpin_ok").await;

    let url = format!("/api/v1/channels/{channel_id}/pins/{message_id}");

    // Pin first.
    let resp = server
        .put(&url)
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status_ok();

    // Unpin.
    let resp = server
        .delete(&url)
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["pinned"], false);
    assert_eq!(body["id"].as_str().unwrap(), message_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn unpin_message_requires_auth() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, _token) =
        setup_with_message(&server, &keys, &state.config, &user_id, "unpin_noauth").await;

    // No auth header.
    let resp = server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/pins/{message_id}"
        ))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn unpin_message_requires_manage_messages_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community + message + pins it.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, owner_token) =
        setup_with_message(&server, &keys, &state.config, &owner_id, "unpin_owner").await;

    // Pin the message.
    server
        .put(&format!(
            "/api/v1/channels/{channel_id}/pins/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await
        .assert_status_ok();

    // Non-member tries to unpin.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "unpin_outsider").await;

    let resp = server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/pins/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

#[tokio::test]
async fn unpin_message_not_pinned() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, token) =
        setup_with_message(&server, &keys, &state.config, &user_id, "unpin_notpin").await;

    // Try to unpin a message that is not pinned.
    let resp = server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/pins/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

// ===========================================================================
// GET /api/v1/channels/:channel_id/pins
// ===========================================================================

#[tokio::test]
async fn list_pins_success() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token =
        common::login_test_user(&server, &keys, &state.config, &user_id, "list_pins").await;

    // Create community.
    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "List Pins Community" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let community: serde_json::Value = resp.json();
    let community_id = community["id"].as_str().unwrap().to_string();

    let channels_resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    let channels: Vec<serde_json::Value> = channels_resp.json();
    let channel_id = channels[0]["id"].as_str().unwrap().to_string();

    // Send 2 messages and pin both.
    let mid1 = send_message(&server, &channel_id, &token, "First pinned").await;
    let mid2 = send_message(&server, &channel_id, &token, "Second pinned").await;

    server
        .put(&format!("/api/v1/channels/{channel_id}/pins/{mid1}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    server
        .put(&format!("/api/v1/channels/{channel_id}/pins/{mid2}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    // List pins.
    let resp = server
        .get(&format!("/api/v1/channels/{channel_id}/pins"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status_ok();
    let pins: Vec<serde_json::Value> = resp.json();
    assert_eq!(pins.len(), 2);

    // Should be ordered by created_at desc (most recent first).
    let ids: Vec<&str> = pins.iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&mid1.as_str()));
    assert!(ids.contains(&mid2.as_str()));

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn list_pins_empty() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token =
        common::login_test_user(&server, &keys, &state.config, &user_id, "list_empty").await;

    // Create community.
    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Empty Pins Community" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let community: serde_json::Value = resp.json();
    let community_id = community["id"].as_str().unwrap().to_string();

    let channels_resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    let channels: Vec<serde_json::Value> = channels_resp.json();
    let channel_id = channels[0]["id"].as_str().unwrap().to_string();

    // List pins — should be empty.
    let resp = server
        .get(&format!("/api/v1/channels/{channel_id}/pins"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status_ok();
    let pins: Vec<serde_json::Value> = resp.json();
    assert!(pins.is_empty());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn list_pins_requires_view_channel() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let owner_token =
        common::login_test_user(&server, &keys, &state.config, &owner_id, "list_owner").await;

    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "name": "List Perms Community" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let community: serde_json::Value = resp.json();
    let community_id = community["id"].as_str().unwrap().to_string();

    let channels_resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    let channels: Vec<serde_json::Value> = channels_resp.json();
    let channel_id = channels[0]["id"].as_str().unwrap().to_string();

    // Non-member tries to list pins.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "list_outsider").await;

    let resp = server
        .get(&format!("/api/v1/channels/{channel_id}/pins"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}
