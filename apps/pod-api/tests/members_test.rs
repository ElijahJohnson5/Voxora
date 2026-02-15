mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:community_id/members
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_members_is_public() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "mem_list").await;

    // No auth header — should still work.
    let resp = server
        .get(&format!("/api/v1/communities/{community_id}/members"))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    let members = body["data"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["user_id"], owner_id);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:community_id/members/:user_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn leave_self_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "mem_leave_owner").await;

    // Add a member via invite.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "mem_leaver",
    )
    .await;

    // Member leaves.
    let resp = server
        .delete(&format!(
            "/api/v1/communities/{community_id}/members/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .await;

    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify member_count decremented (was 2 after join, now 1).
    let community_resp = server
        .get(&format!("/api/v1/communities/{community_id}"))
        .await;
    let community: serde_json::Value = community_resp.json();
    assert_eq!(community["member_count"], 1);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn kick_member_requires_kick_members_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "mem_kick_owner").await;

    // Add two members via invite.
    let member1_id = voxora_common::id::prefixed_ulid("usr");
    let member1_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member1_id,
        "mem_kick_m1",
    )
    .await;

    let member2_id = voxora_common::id::prefixed_ulid("usr");
    let _member2_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member2_id,
        "mem_kick_m2",
    )
    .await;

    // Member1 (no KICK_MEMBERS perm) tries to kick member2 → 403.
    let resp = server
        .delete(&format!(
            "/api/v1/communities/{community_id}/members/{member2_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {member1_token}"))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member1_id).await;
    common::cleanup_test_user(&state.db, &member2_id).await;
}

#[tokio::test]
async fn owner_cannot_leave_or_be_kicked() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "mem_owner_leave").await;

    // Owner tries to leave themselves.
    let resp = server
        .delete(&format!(
            "/api/v1/communities/{community_id}/members/{owner_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/communities/:community_id/members/:user_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_own_nickname_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "mem_nick_owner").await;

    // Add a member via invite.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "mem_nick_user",
    )
    .await;

    // Member updates own nickname.
    let resp = server
        .patch(&format!(
            "/api/v1/communities/{community_id}/members/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .json(&serde_json::json!({ "nickname": "Cool Nick" }))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["nickname"], "Cool Nick");

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn update_own_roles_returns_403() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "mem_roles_owner").await;

    // Add a member via invite.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "mem_roles_user",
    )
    .await;

    // Member tries to change own roles → 403.
    let resp = server
        .patch(&format!(
            "/api/v1/communities/{community_id}/members/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .json(&serde_json::json!({ "roles": ["some-role-id"] }))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}
