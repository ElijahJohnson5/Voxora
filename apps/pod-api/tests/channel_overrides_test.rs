mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ---------------------------------------------------------------------------
// GET /api/v1/channels/:channel_id/overrides
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_overrides_requires_manage_channels() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "co_list_own")
            .await;

    // Add a member with no MANAGE_CHANNELS permission.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "co_list_mem",
    )
    .await;

    // Member -> 403.
    let resp = server
        .get(&format!("/api/v1/channels/{channel_id}/overrides"))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);

    // Owner -> 200 (empty list).
    let resp = server
        .get(&format!("/api/v1/channels/{channel_id}/overrides"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    resp.assert_status_ok();
    let overrides: Vec<serde_json::Value> = resp.json();
    assert!(overrides.is_empty());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

// ---------------------------------------------------------------------------
// PUT /api/v1/channels/:channel_id/overrides/:target_type/:target_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upsert_override_requires_manage_roles() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "co_up_own")
            .await;

    let member_id = voxora_common::id::prefixed_ulid("usr");
    let member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "co_up_mem",
    )
    .await;

    // Get the @everyone role ID.
    let roles_resp = server
        .get(&format!("/api/v1/communities/{community_id}/roles"))
        .await;
    let roles: Vec<serde_json::Value> = roles_resp.json();
    let everyone_role_id = roles
        .iter()
        .find(|r| r["is_default"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Member with no MANAGE_ROLES -> 403.
    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/overrides/role/{everyone_role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .json(&serde_json::json!({ "allow": 0, "deny": 2 }))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);

    // Owner -> 200.
    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/overrides/role/{everyone_role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "allow": 0, "deny": 2 }))
        .await;
    resp.assert_status_ok();
    let override_val: serde_json::Value = resp.json();
    assert_eq!(override_val["channel_id"], channel_id);
    assert_eq!(override_val["target_type"], 0); // role
    assert_eq!(override_val["target_id"], everyone_role_id);
    assert_eq!(override_val["allow"], 0);
    assert_eq!(override_val["deny"], 2);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn upsert_override_is_idempotent() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "co_idem_own")
            .await;

    let roles_resp = server
        .get(&format!("/api/v1/communities/{community_id}/roles"))
        .await;
    let roles: Vec<serde_json::Value> = roles_resp.json();
    let everyone_role_id = roles
        .iter()
        .find(|r| r["is_default"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create override.
    server
        .put(&format!(
            "/api/v1/channels/{channel_id}/overrides/role/{everyone_role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "allow": 0, "deny": 2 }))
        .await
        .assert_status_ok();

    // Update same override (upsert).
    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/overrides/role/{everyone_role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "allow": 1, "deny": 0 }))
        .await;
    resp.assert_status_ok();
    let updated: serde_json::Value = resp.json();
    assert_eq!(updated["allow"], 1);
    assert_eq!(updated["deny"], 0);

    // List overrides: should have exactly 1.
    let list_resp = server
        .get(&format!("/api/v1/channels/{channel_id}/overrides"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    let overrides: Vec<serde_json::Value> = list_resp.json();
    assert_eq!(overrides.len(), 1);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

#[tokio::test]
async fn invalid_target_type_returns_400() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "co_inv_own")
            .await;

    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/overrides/invalid/some_id"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "allow": 0, "deny": 2 }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:channel_id/overrides/:target_type/:target_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_override_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "co_del_own")
            .await;

    let roles_resp = server
        .get(&format!("/api/v1/communities/{community_id}/roles"))
        .await;
    let roles: Vec<serde_json::Value> = roles_resp.json();
    let everyone_role_id = roles
        .iter()
        .find(|r| r["is_default"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create override.
    server
        .put(&format!(
            "/api/v1/channels/{channel_id}/overrides/role/{everyone_role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "allow": 0, "deny": 2 }))
        .await
        .assert_status_ok();

    // Delete it.
    let resp = server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/overrides/role/{everyone_role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone.
    let list_resp = server
        .get(&format!("/api/v1/channels/{channel_id}/overrides"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    let overrides: Vec<serde_json::Value> = list_resp.json();
    assert!(overrides.is_empty());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

#[tokio::test]
async fn delete_nonexistent_override_returns_404() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "co_del404_own")
            .await;

    let resp = server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/overrides/role/nonexistent_role"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ---------------------------------------------------------------------------
// Channel-aware permission resolution
// ---------------------------------------------------------------------------

#[tokio::test]
async fn channel_override_deny_blocks_send_messages() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "co_deny_own")
            .await;

    // Add a member.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "co_deny_mem",
    )
    .await;

    // Member can send messages initially.
    let msg_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .json(&serde_json::json!({ "content": "Hello" }))
        .await;
    msg_resp.assert_status(StatusCode::CREATED);

    // Get @everyone role ID.
    let roles_resp = server
        .get(&format!("/api/v1/communities/{community_id}/roles"))
        .await;
    let roles: Vec<serde_json::Value> = roles_resp.json();
    let everyone_role_id = roles
        .iter()
        .find(|r| r["is_default"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Set channel override to deny SEND_MESSAGES (bit 1 = 2) on @everyone.
    server
        .put(&format!(
            "/api/v1/channels/{channel_id}/overrides/role/{everyone_role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "allow": 0, "deny": 2 }))
        .await
        .assert_status_ok();

    // Member can no longer send messages.
    let msg_resp2 = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .json(&serde_json::json!({ "content": "Blocked" }))
        .await;
    msg_resp2.assert_status(StatusCode::FORBIDDEN);

    // Owner (community owner) can still send (owner bypasses all checks).
    let msg_resp3 = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "content": "I can still send" }))
        .await;
    msg_resp3.assert_status(StatusCode::CREATED);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn user_override_grants_permission_despite_role_deny() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "co_usr_own")
            .await;

    // Add a member.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "co_usr_mem",
    )
    .await;

    // Get @everyone role ID.
    let roles_resp = server
        .get(&format!("/api/v1/communities/{community_id}/roles"))
        .await;
    let roles: Vec<serde_json::Value> = roles_resp.json();
    let everyone_role_id = roles
        .iter()
        .find(|r| r["is_default"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Deny SEND_MESSAGES on @everyone for this channel.
    server
        .put(&format!(
            "/api/v1/channels/{channel_id}/overrides/role/{everyone_role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "allow": 0, "deny": 2 }))
        .await
        .assert_status_ok();

    // But allow SEND_MESSAGES for this specific user.
    server
        .put(&format!(
            "/api/v1/channels/{channel_id}/overrides/user/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "allow": 2, "deny": 0 }))
        .await
        .assert_status_ok();

    // Member CAN send (user override allows it despite role deny).
    let msg_resp = server
        .post(&format!("/api/v1/channels/{channel_id}/messages"))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .json(&serde_json::json!({ "content": "Allowed by user override" }))
        .await;
    msg_resp.assert_status(StatusCode::CREATED);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}
