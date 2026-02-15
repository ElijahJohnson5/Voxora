mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ---------------------------------------------------------------------------
// POST /api/v1/communities/:community_id/invites
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_invite_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "inv_create").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({}))
        .await;

    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["code"].as_str().unwrap().len(), 8);
    assert_eq!(body["community_id"], community_id);
    assert_eq!(body["inviter_id"], user_id);
    assert_eq!(body["use_count"], 0);
    assert!(body["max_uses"].is_null());
    assert!(body["expires_at"].is_null());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_invite_with_max_uses_and_max_age() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "inv_opts").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "max_uses": 10, "max_age_seconds": 3600 }))
        .await;

    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["max_uses"], 10);
    assert_eq!(body["max_age_seconds"], 3600);
    assert!(body["expires_at"].is_string());

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_invite_requires_invite_members_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "inv_owner").await;

    // Non-member tries to create invite.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "inv_outsider").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .json(&serde_json::json!({}))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:community_id/invites
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_invites_requires_manage_community() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community + invite.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "inv_list_owner").await;

    // Create an invite so there's something to list.
    server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await
        .assert_status(StatusCode::CREATED);

    // Another user joins via invite, then tries to list invites.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "inv_list_member")
            .await;

    // Create a second invite for the other user to accept.
    let invite_resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await;
    let invite_code = invite_resp.json::<serde_json::Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    // Accept invite.
    server
        .post(&format!("/api/v1/invites/{invite_code}/accept"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .await
        .assert_status(StatusCode::CREATED);

    // Regular member (no MANAGE_COMMUNITY) tries to list invites → 403.
    let resp = server
        .get(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:community_id/invites/:code
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_invite_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "inv_del").await;

    // Create invite.
    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({}))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let code = resp.json::<serde_json::Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete it.
    let resp = server
        .delete(&format!(
            "/api/v1/communities/{community_id}/invites/{code}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::NO_CONTENT);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn delete_nonexistent_invite_returns_404() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "inv_del404").await;

    let resp = server
        .delete(&format!(
            "/api/v1/communities/{community_id}/invites/NONEXIST"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

// ---------------------------------------------------------------------------
// POST /api/v1/invites/:code/accept
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accept_invite_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community + invite.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "inv_acc_owner").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let code = resp.json::<serde_json::Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    // Another user accepts.
    let joiner_id = voxora_common::id::prefixed_ulid("usr");
    let joiner_token =
        common::login_test_user(&server, &keys, &state.config, &joiner_id, "inv_joiner").await;

    let resp = server
        .post(&format!("/api/v1/invites/{code}/accept"))
        .add_header(AUTHORIZATION, format!("Bearer {joiner_token}"))
        .await;

    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["community_id"], community_id);
    assert_eq!(body["user_id"], joiner_id);

    // Verify use_count incremented (owner lists invites).
    let list_resp = server
        .get(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    let invites: Vec<serde_json::Value> = list_resp.json();
    let invite = invites.iter().find(|i| i["code"] == code).unwrap();
    assert_eq!(invite["use_count"], 1);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &joiner_id).await;
}

#[tokio::test]
async fn accept_invite_fails_when_max_uses_exceeded() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community + invite with max_uses=1.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "inv_max_owner").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/invites"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "max_uses": 1 }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let code = resp.json::<serde_json::Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    // First user joins successfully.
    let user1_id = voxora_common::id::prefixed_ulid("usr");
    let user1_token =
        common::login_test_user(&server, &keys, &state.config, &user1_id, "inv_max_u1").await;

    server
        .post(&format!("/api/v1/invites/{code}/accept"))
        .add_header(AUTHORIZATION, format!("Bearer {user1_token}"))
        .await
        .assert_status(StatusCode::CREATED);

    // Second user should fail.
    let user2_id = voxora_common::id::prefixed_ulid("usr");
    let user2_token =
        common::login_test_user(&server, &keys, &state.config, &user2_id, "inv_max_u2").await;

    let resp = server
        .post(&format!("/api/v1/invites/{code}/accept"))
        .add_header(AUTHORIZATION, format!("Bearer {user2_token}"))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &user1_id).await;
    common::cleanup_test_user(&state.db, &user2_id).await;
}
