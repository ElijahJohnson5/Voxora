mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

/// Helper: create a community and return (community_id, token).
async fn setup_community(
    server: &TestServer,
    keys: &common::TestSigningKeys,
    config: &pod_api::config::Config,
    user_id: &str,
    username: &str,
) -> (String, String) {
    let token = common::login_test_user(server, keys, config, user_id, username).await;

    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Ban Test Community" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let community: serde_json::Value = resp.json();
    let community_id = community["id"].as_str().unwrap().to_string();

    (community_id, token)
}

/// Helper: create invite and have a user accept it, returning their token.
async fn join_via_invite(
    server: &TestServer,
    keys: &common::TestSigningKeys,
    config: &pod_api::config::Config,
    community_id: &str,
    owner_token: &str,
    joiner_id: &str,
    joiner_username: &str,
) -> String {
    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await;
    let code = resp.json::<serde_json::Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    let joiner_token =
        common::login_test_user(server, keys, config, joiner_id, joiner_username).await;

    server
        .post(&format!("/api/v1/invites/{code}/accept"))
        .add_header(AUTHORIZATION, format!("Bearer {joiner_token}"))
        .await
        .assert_status(StatusCode::CREATED);

    joiner_token
}

// ---------------------------------------------------------------------------
// PUT /api/v1/communities/:community_id/bans/:user_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ban_member_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        setup_community(&server, &keys, &state.config, &owner_id, "ban_owner").await;

    // Add a member.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let _member_token = join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "ban_target",
    )
    .await;

    // Ban the member.
    let resp = server
        .put(&format!(
            "/api/v1/communities/{community_id}/bans/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "reason": "Bad behavior" }))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["community_id"], community_id);
    assert_eq!(body["user_id"], member_id);
    assert_eq!(body["reason"], "Bad behavior");
    assert_eq!(body["banned_by"], owner_id);

    // Verify member was removed from community.
    let members_resp = server
        .get(&format!("/api/v1/communities/{community_id}/members"))
        .await;
    let members_body: serde_json::Value = members_resp.json();
    let members = members_body["data"].as_array().unwrap();
    assert!(!members.iter().any(|m| m["user_id"] == member_id.as_str()));

    // Verify member_count decremented (was 2, now 1).
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
async fn ban_requires_ban_members_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        setup_community(&server, &keys, &state.config, &owner_id, "ban_perm_owner").await;

    // Add two members.
    let member1_id = voxora_common::id::prefixed_ulid("usr");
    let member1_token = join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member1_id,
        "ban_perm_m1",
    )
    .await;

    let member2_id = voxora_common::id::prefixed_ulid("usr");
    let _member2_token = join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member2_id,
        "ban_perm_m2",
    )
    .await;

    // Member1 (no BAN_MEMBERS perm) tries to ban member2 -> 403.
    let resp = server
        .put(&format!(
            "/api/v1/communities/{community_id}/bans/{member2_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {member1_token}"))
        .json(&serde_json::json!({}))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member1_id).await;
    common::cleanup_test_user(&state.db, &member2_id).await;
}

#[tokio::test]
async fn cannot_ban_owner() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        setup_community(&server, &keys, &state.config, &owner_id, "ban_owner_prot").await;

    // Owner tries to ban themselves (also covers "cannot ban owner").
    let resp = server
        .put(&format!(
            "/api/v1/communities/{community_id}/bans/{owner_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:community_id/bans/:user_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unban_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        setup_community(&server, &keys, &state.config, &owner_id, "unban_owner").await;

    // Add and ban a member.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let _member_token = join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "unban_target",
    )
    .await;

    server
        .put(&format!(
            "/api/v1/communities/{community_id}/bans/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await
        .assert_status_ok();

    // Unban.
    let resp = server
        .delete(&format!(
            "/api/v1/communities/{community_id}/bans/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;

    resp.assert_status(StatusCode::NO_CONTENT);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

#[tokio::test]
async fn banned_user_cannot_accept_invite() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        setup_community(&server, &keys, &state.config, &owner_id, "ban_inv_owner").await;

    // Add and ban a member.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let member_token = join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "ban_inv_target",
    )
    .await;

    server
        .put(&format!(
            "/api/v1/communities/{community_id}/bans/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await
        .assert_status_ok();

    // Create a new invite.
    let invite_resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await;
    invite_resp.assert_status(StatusCode::CREATED);
    let code = invite_resp.json::<serde_json::Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    // Banned user tries to accept invite -> 403.
    let resp = server
        .post(&format!("/api/v1/invites/{code}/accept"))
        .add_header(AUTHORIZATION, format!("Bearer {member_token}"))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}
