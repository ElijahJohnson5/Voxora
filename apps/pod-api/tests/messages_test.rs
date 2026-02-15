mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ---------------------------------------------------------------------------
// POST /api/v1/channels/:channel_id/messages
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_message_returns_correct_fields() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "msg_sender").await;

    let resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Hello, world!" }))
        .await;

    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();

    // Snowflake ID should be a parseable positive integer (serialized as string).
    assert!(body["id"].as_str().unwrap().parse::<i64>().unwrap() > 0);
    assert_eq!(body["channel_id"], channel_id);
    assert_eq!(body["author_id"], user_id);
    assert_eq!(body["content"], "Hello, world!");
    assert_eq!(body["type"], 0);
    assert_eq!(body["flags"], 0);
    assert!(body["reply_to"].is_null());
    assert!(body["edited_at"].is_null());
    assert_eq!(body["pinned"], false);
    assert!(body["created_at"].is_string());

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn send_message_requires_auth() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, _token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "msg_noauth").await;

    // No auth header.
    let resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .json(&serde_json::json!({ "content": "Hello" }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn send_message_requires_send_messages_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, _owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "msg_owner").await;

    // Non-member tries to send.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "msg_outsider").await;

    let resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .json(&serde_json::json!({ "content": "Intruder!" }))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

#[tokio::test]
async fn send_message_validates_empty_content() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "msg_empty").await;

    let resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "   " }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn send_message_validates_content_too_long() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "msg_long").await;

    let long_content = "a".repeat(4001);
    let resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": long_content }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn send_message_with_reply_to() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "msg_reply").await;

    // Send first message.
    let resp1 = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Original message" }))
        .await;
    resp1.assert_status(StatusCode::CREATED);
    let msg1: serde_json::Value = resp1.json();
    let msg1_id = msg1["id"].as_str().unwrap();

    // Reply to it.
    let resp2 = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({
            "content": "This is a reply",
            "reply_to": msg1_id
        }))
        .await;

    resp2.assert_status(StatusCode::CREATED);
    let msg2: serde_json::Value = resp2.json();
    assert_eq!(msg2["reply_to"].as_str().unwrap(), msg1_id);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn send_message_returns_404_for_nonexistent_channel() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token =
        common::login_test_user(&server, &keys, &state.config, &user_id, "msg_404").await;

    let resp = server
        .post("/api/v1/channels/ch_DOES_NOT_EXIST/messages")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Orphaned" }))
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);

    // Cleanup.
    common::cleanup_test_user(&state.db, &user_id).await;
}

// ---------------------------------------------------------------------------
// GET /api/v1/channels/:channel_id/messages
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_messages_in_chronological_order() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "msg_list").await;

    // Send 3 messages.
    for i in 1..=3 {
        let resp = server
            .post(&format!("/api/v1/channels/{channel_id}/messages"))
            .add_header(AUTHORIZATION, format!("Bearer {token}"))
            .json(&serde_json::json!({ "content": format!("Message {i}") }))
            .await;
        resp.assert_status(StatusCode::CREATED);
    }

    // List (no auth required).
    let resp = server
        .get(&format!("/api/v1/channels/{channel_id}/messages"))
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 3);
    // Should be in ascending (chronological) order.
    assert_eq!(data[0]["content"], "Message 1");
    assert_eq!(data[1]["content"], "Message 2");
    assert_eq!(data[2]["content"], "Message 3");
    assert_eq!(body["has_more"], false);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn list_messages_with_before_cursor_and_has_more() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "msg_cursor").await;

    // Send 5 messages and collect IDs.
    let mut msg_ids = Vec::new();
    for i in 1..=5 {
        let resp = server
            .post(&format!("/api/v1/channels/{channel_id}/messages"))
            .add_header(AUTHORIZATION, format!("Bearer {token}"))
            .json(&serde_json::json!({ "content": format!("Msg {i}") }))
            .await;
        resp.assert_status(StatusCode::CREATED);
        let msg: serde_json::Value = resp.json();
        msg_ids.push(msg["id"].as_str().unwrap().to_string());
    }

    // Fetch with before=msg_ids[4] (the 5th message) and limit=2.
    let resp = server
        .get(&format!(
            "/api/v1/channels/{channel_id}/messages?before={}&limit=2",
            msg_ids[4]
        ))
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 2);
    // Should return messages 3 and 4 (before message 5), in ascending order.
    assert_eq!(data[0]["content"], "Msg 3");
    assert_eq!(data[1]["content"], "Msg 4");
    assert_eq!(body["has_more"], true);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/channels/:channel_id/messages/:message_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn edit_message_by_author_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "msg_editor").await;

    // Send a message.
    let send_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Original" }))
        .await;
    send_resp.assert_status(StatusCode::CREATED);
    let msg: serde_json::Value = send_resp.json();
    let msg_id = msg["id"].as_str().unwrap();
    assert!(msg["edited_at"].is_null());

    // Edit it.
    let resp = server
        .patch(&format!(
            "/api/v1/channels/{channel_id}/messages/{msg_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Edited content" }))
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["content"], "Edited content");
    assert!(body["edited_at"].is_string()); // edited_at should now be set.

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn edit_message_by_non_author_returns_403() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "msg_edit_owner")
            .await;

    // Owner sends a message.
    let send_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "content": "Owner's message" }))
        .await;
    send_resp.assert_status(StatusCode::CREATED);
    let msg: serde_json::Value = send_resp.json();
    let msg_id = msg["id"].as_str().unwrap();

    // Another user tries to edit.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "msg_edit_other").await;

    let resp = server
        .patch(&format!(
            "/api/v1/channels/{channel_id}/messages/{msg_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .json(&serde_json::json!({ "content": "Hijacked!" }))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:channel_id/messages/:message_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_message_by_author_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "msg_deleter").await;

    // Send a message.
    let send_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "To be deleted" }))
        .await;
    send_resp.assert_status(StatusCode::CREATED);
    let msg: serde_json::Value = send_resp.json();
    let msg_id = msg["id"].as_str().unwrap();

    // Delete it.
    let resp = server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/messages/{msg_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone (list should be empty).
    let list_resp = server
        .get(&format!("/api/v1/channels/{channel_id}/messages"))
        .await;
    list_resp.assert_status_ok();
    let body: serde_json::Value = list_resp.json();
    let data = body["data"].as_array().unwrap();
    assert!(data.is_empty());

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}
