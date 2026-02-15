mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ---------------------------------------------------------------------------
// POST /api/v1/communities/:community_id/channels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_channel_returns_channel_with_correct_fields() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "ch_creator").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "announcements",
            "topic": "Important news",
            "nsfw": false
        }))
        .await;

    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert!(body["id"].as_str().unwrap().starts_with("ch_"));
    assert_eq!(body["community_id"], community_id);
    assert_eq!(body["name"], "announcements");
    assert_eq!(body["topic"], "Important news");
    assert_eq!(body["type"], 0);
    assert_eq!(body["nsfw"], false);
    assert_eq!(body["position"], 0);
    assert_eq!(body["slowmode_seconds"], 0);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_channel_requires_auth() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "ch_noauth").await;

    // No auth header.
    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .json(&serde_json::json!({ "name": "test" }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_channel_requires_manage_channels_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "ch_owner").await;

    // Non-member tries to create channel.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "ch_outsider").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .json(&serde_json::json!({ "name": "hijack" }))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

#[tokio::test]
async fn create_channel_validates_name_empty() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "ch_val").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "   " }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_channel_returns_404_for_nonexistent_community() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = common::login_test_user(&server, &keys, &state.config, &user_id, "ch_404").await;

    let resp = server
        .post("/api/v1/communities/com_DOES_NOT_EXIST/channels")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "orphan" }))
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);

    common::cleanup_test_user(&state.db, &user_id).await;
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:community_id/channels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_channels_in_community_is_public() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "ch_lister").await;

    // List without auth.
    let resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    resp.assert_status_ok();

    let body: Vec<serde_json::Value> = resp.json();
    // Should have at least the default #general channel.
    assert!(!body.is_empty());
    assert_eq!(body[0]["name"], "general");

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

// ---------------------------------------------------------------------------
// GET /api/v1/channels/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_channel_by_id() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, _token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "ch_getter")
            .await;

    // Get channel by ID (public).
    let resp = server
        .get(&format!("/api/v1/channels/{channel_id}"))
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["id"], channel_id);
    assert_eq!(body["name"], "general");

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn get_channel_returns_404_for_nonexistent() {
    let (app, _state, _keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/v1/channels/ch_DOES_NOT_EXIST").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/channels/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_channel_by_owner_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "ch_updater").await;

    // Create a channel to update.
    let create_resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "original" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let channel_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Update.
    let resp = server
        .patch(&format!("/api/v1/channels/{channel_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "renamed",
            "topic": "New topic",
            "nsfw": true,
            "slowmode_seconds": 5
        }))
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["name"], "renamed");
    assert_eq!(body["topic"], "New topic");
    assert_eq!(body["nsfw"], true);
    assert_eq!(body["slowmode_seconds"], 5);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn update_channel_by_non_member_returns_403() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, _owner_token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &owner_id, "ch_upd_owner")
            .await;

    // Non-member tries to update.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "ch_upd_other").await;

    let resp = server
        .patch(&format!("/api/v1/channels/{channel_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .json(&serde_json::json!({ "name": "hijacked" }))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_channel_by_owner_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &user_id, "ch_deleter").await;

    // Create a non-default channel to delete.
    let create_resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "doomed" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let channel_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete.
    let resp = server
        .delete(&format!("/api/v1/channels/{channel_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify 404 after deletion.
    let get_resp = server
        .get(&format!("/api/v1/channels/{channel_id}"))
        .await;
    get_resp.assert_status(StatusCode::NOT_FOUND);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn delete_default_channel_returns_400() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, token) =
        common::setup_community_and_channel(&server, &keys, &state.config, &user_id, "ch_del_def")
            .await;

    // The default channel is the one from setup_community_and_channel.
    let resp = server
        .delete(&format!("/api/v1/channels/{channel_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);

    let body: serde_json::Value = resp.json();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("default channel"));

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}
