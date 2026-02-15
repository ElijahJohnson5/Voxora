mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ===========================================================================
// Helpers
// ===========================================================================

/// Fetch audit log for a community.
async fn get_audit_log(
    server: &TestServer,
    community_id: &str,
    token: &str,
    extra_params: &str,
) -> serde_json::Value {
    let url = if extra_params.is_empty() {
        format!("/api/v1/communities/{community_id}/audit-log")
    } else {
        format!("/api/v1/communities/{community_id}/audit-log?{extra_params}")
    };
    let resp = server
        .get(&url)
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status_ok();
    resp.json()
}

// ===========================================================================
// GET /api/v1/communities/:community_id/audit-log — Endpoint tests
// ===========================================================================

#[tokio::test]
async fn list_audit_log_success() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_ok").await;

    // Send a message and pin it (creates an audit entry).
    let msg_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Pin me" }))
        .await;
    msg_resp.assert_status(StatusCode::CREATED);
    let message_id = msg_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .put(&format!("/api/v1/channels/{channel_id}/pins/{message_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    // Query audit log.
    let body = get_audit_log(&server, &community_id, &token, "").await;
    let entries = body["data"].as_array().unwrap();
    assert!(!entries.is_empty());

    let entry = &entries[0];
    assert_eq!(entry["action"], "message.pin");
    assert_eq!(entry["actor_id"], user_id);
    assert_eq!(entry["community_id"], community_id);
    assert_eq!(entry["target_type"], "message");
    assert_eq!(entry["target_id"], message_id);
    assert!(entry["id"].as_str().unwrap().starts_with("aud_"));

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn list_audit_log_requires_auth() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, _token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_noauth").await;

    // No auth header.
    let resp = server
        .get(&format!("/api/v1/communities/{community_id}/audit-log"))
        .await;
    resp.assert_status(StatusCode::UNAUTHORIZED);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn list_audit_log_requires_view_audit_log_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, _owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "audit_perm_own").await;

    // Non-member tries to query audit log.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "audit_outsider").await;

    let resp = server
        .get(&format!("/api/v1/communities/{community_id}/audit-log"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

#[tokio::test]
async fn list_audit_log_filter_by_action() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_filt_act").await;

    // Pin a message.
    let msg_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Pin me" }))
        .await;
    let message_id = msg_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .put(&format!("/api/v1/channels/{channel_id}/pins/{message_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    // Unpin the message.
    server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/pins/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    // Filter by action=message.pin → only pin entry.
    let body = get_audit_log(&server, &community_id, &token, "action=message.pin").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["action"], "message.pin");

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn list_audit_log_filter_by_user_id() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_filt_uid").await;

    // Pin a message (creates audit entry from user_id).
    let msg_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Pin me" }))
        .await;
    let message_id = msg_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .put(&format!("/api/v1/channels/{channel_id}/pins/{message_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    // Filter by user_id.
    let body = get_audit_log(
        &server,
        &community_id,
        &token,
        &format!("user_id={user_id}"),
    )
    .await;
    let entries = body["data"].as_array().unwrap();
    assert!(!entries.is_empty());
    for entry in entries {
        assert_eq!(entry["actor_id"], user_id);
    }

    // Filter by a different user_id → empty.
    let body = get_audit_log(
        &server,
        &community_id,
        &token,
        "user_id=usr_nonexistent",
    )
    .await;
    let entries = body["data"].as_array().unwrap();
    assert!(entries.is_empty());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn list_audit_log_pagination() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_page").await;

    // Create 3 audit entries by pinning/unpinning.
    let msg_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "Pin me" }))
        .await;
    let message_id = msg_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Pin (entry 1).
    server
        .put(&format!("/api/v1/channels/{channel_id}/pins/{message_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    // Unpin (entry 2).
    server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/pins/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    // Pin again (entry 3).
    server
        .put(&format!("/api/v1/channels/{channel_id}/pins/{message_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    // Fetch first page with limit=2.
    let body = get_audit_log(&server, &community_id, &token, "limit=2").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(body["has_more"], true);

    // Use last entry's ID as cursor.
    let cursor = entries.last().unwrap()["id"].as_str().unwrap();
    let body = get_audit_log(
        &server,
        &community_id,
        &token,
        &format!("limit=2&before={cursor}"),
    )
    .await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(body["has_more"], false);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn list_audit_log_empty() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_empty").await;

    // No audit entries yet.
    let body = get_audit_log(&server, &community_id, &token, "").await;
    let entries = body["data"].as_array().unwrap();
    assert!(entries.is_empty());
    assert_eq!(body["has_more"], false);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

// ===========================================================================
// Audit logging in moderation endpoints
// ===========================================================================

#[tokio::test]
async fn channel_create_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_ch_cre").await;

    // Create a new channel.
    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "audit-channel" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let new_channel_id = resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let body = get_audit_log(&server, &community_id, &token, "action=channel.create").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "channel");
    assert_eq!(entries[0]["target_id"], new_channel_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn channel_update_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_ch_upd").await;

    // Create a channel to update.
    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "original" }))
        .await;
    let channel_id = resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Update channel.
    server
        .patch(&format!("/api/v1/channels/{channel_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "renamed", "topic": "new topic" }))
        .await
        .assert_status_ok();

    let body = get_audit_log(&server, &community_id, &token, "action=channel.update").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "channel");
    assert_eq!(entries[0]["target_id"], channel_id);

    // Check changes JSON.
    let changes = &entries[0]["changes"];
    assert!(changes["name"].is_object());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn channel_delete_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_ch_del").await;

    // Create a non-default channel to delete.
    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "deleteme" }))
        .await;
    let channel_id = resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .delete(&format!("/api/v1/channels/{channel_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let body = get_audit_log(&server, &community_id, &token, "action=channel.delete").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "channel");
    assert_eq!(entries[0]["target_id"], channel_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn ban_member_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "audit_ban_own").await;

    let member_id = voxora_common::id::prefixed_ulid("usr");
    let _member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "audit_ban_mem",
    )
    .await;

    // Ban the member.
    server
        .put(&format!(
            "/api/v1/communities/{community_id}/bans/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "reason": "spam" }))
        .await
        .assert_status_ok();

    let body = get_audit_log(&server, &community_id, &owner_token, "action=member.ban").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "user");
    assert_eq!(entries[0]["target_id"], member_id);
    assert_eq!(entries[0]["reason"], "spam");

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn unban_member_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "audit_unban_o").await;

    let member_id = voxora_common::id::prefixed_ulid("usr");
    let _member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "audit_unban_m",
    )
    .await;

    // Ban then unban.
    server
        .put(&format!(
            "/api/v1/communities/{community_id}/bans/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await
        .assert_status_ok();

    server
        .delete(&format!(
            "/api/v1/communities/{community_id}/bans/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let body = get_audit_log(&server, &community_id, &owner_token, "action=member.unban").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "user");
    assert_eq!(entries[0]["target_id"], member_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn role_create_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_role_cr").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/roles"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Moderator" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let role_id = resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let body = get_audit_log(&server, &community_id, &token, "action=role.create").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "role");
    assert_eq!(entries[0]["target_id"], role_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn role_update_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_role_up").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/roles"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "OldName" }))
        .await;
    let role_id = resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .patch(&format!(
            "/api/v1/communities/{community_id}/roles/{role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "NewName" }))
        .await
        .assert_status_ok();

    let body = get_audit_log(&server, &community_id, &token, "action=role.update").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "role");
    assert_eq!(entries[0]["target_id"], role_id);

    let changes = &entries[0]["changes"];
    assert!(changes["name"].is_object());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn role_delete_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_role_dl").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/roles"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "ToDelete" }))
        .await;
    let role_id = resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .delete(&format!(
            "/api/v1/communities/{community_id}/roles/{role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let body = get_audit_log(&server, &community_id, &token, "action=role.delete").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "role");
    assert_eq!(entries[0]["target_id"], role_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn member_kick_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "audit_kick_o").await;

    let member_id = voxora_common::id::prefixed_ulid("usr");
    let _member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "audit_kick_m",
    )
    .await;

    // Kick the member.
    server
        .delete(&format!(
            "/api/v1/communities/{community_id}/members/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let body = get_audit_log(&server, &community_id, &owner_token, "action=member.kick").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "user");
    assert_eq!(entries[0]["target_id"], member_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn member_role_update_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "audit_mrol_o").await;

    // Create a role.
    let role_resp = server
        .post(&format!("/api/v1/communities/{community_id}/roles"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "name": "TestRole" }))
        .await;
    let role_id = role_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Add a member.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let _member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "audit_mrol_m",
    )
    .await;

    // Assign role to member.
    server
        .patch(&format!(
            "/api/v1/communities/{community_id}/members/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "roles": [role_id] }))
        .await
        .assert_status_ok();

    let body = get_audit_log(
        &server,
        &community_id,
        &owner_token,
        "action=member.role_update",
    )
    .await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "user");
    assert_eq!(entries[0]["target_id"], member_id);

    let changes = &entries[0]["changes"];
    assert!(changes["roles"].is_object());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn invite_create_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_inv_cr").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({}))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let invite_code = resp.json::<serde_json::Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    let body = get_audit_log(&server, &community_id, &token, "action=invite.create").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "invite");
    assert_eq!(entries[0]["target_id"], invite_code);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn invite_delete_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_inv_dl").await;

    // Create invite.
    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({}))
        .await;
    let invite_code = resp.json::<serde_json::Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete invite.
    server
        .delete(&format!(
            "/api/v1/communities/{community_id}/invites/{invite_code}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let body = get_audit_log(&server, &community_id, &token, "action=invite.delete").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "invite");
    assert_eq!(entries[0]["target_id"], invite_code);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn community_update_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_com_up").await;

    server
        .patch(&format!("/api/v1/communities/{community_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Updated Name" }))
        .await
        .assert_status_ok();

    let body = get_audit_log(&server, &community_id, &token, "action=community.update").await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "community");
    assert_eq!(entries[0]["target_id"], community_id);

    let changes = &entries[0]["changes"];
    assert!(changes["name"].is_object());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn message_delete_by_mod_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "audit_mdel_o").await;

    // Have a member send a message.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "audit_mdel_m",
    )
    .await;

    let msg_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .json(&serde_json::json!({ "content": "Delete me" }))
        .await;
    let message_id = msg_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Owner (mod) deletes the message.
    server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/messages/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let body = get_audit_log(
        &server,
        &community_id,
        &owner_token,
        "action=message.delete",
    )
    .await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["target_type"], "message");
    assert_eq!(entries[0]["target_id"], message_id);
    assert_eq!(entries[0]["actor_id"], owner_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn message_delete_by_author_no_audit_log() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "audit_mdel_au").await;

    // Author sends and deletes own message.
    let msg_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "content": "My msg" }))
        .await;
    let message_id = msg_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/messages/{message_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    // No message.delete audit entry should exist.
    let body = get_audit_log(&server, &community_id, &token, "action=message.delete").await;
    let entries = body["data"].as_array().unwrap();
    assert!(entries.is_empty());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}
