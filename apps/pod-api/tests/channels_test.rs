mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

/// Helper: create a community and return (community_id, default_channel_id, token).
async fn setup_community(
    server: &TestServer,
    keys: &common::TestSigningKeys,
    config: &pod_api::config::Config,
    user_id: &str,
    username: &str,
) -> String {
    let token = common::login_test_user(server, keys, config, user_id, username).await;
    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Channel Test Community" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    token
}

// ---------------------------------------------------------------------------
// POST /api/v1/communities/:community_id/channels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_channel_returns_channel_with_correct_fields() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = setup_community(&server, &keys, &state.config, &user_id, "ch_creator").await;

    // Get community id from list.
    let list_resp = server
        .get("/api/v1/communities")
        .await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community_id = communities
        .iter()
        .find(|c| c["owner_id"] == user_id)
        .map(|c| c["id"].as_str().unwrap().to_string())
        .unwrap();

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

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_channel_requires_auth() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let _token = setup_community(&server, &keys, &state.config, &user_id, "ch_noauth").await;

    let list_resp = server.get("/api/v1/communities").await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community_id = communities
        .iter()
        .find(|c| c["owner_id"] == user_id)
        .map(|c| c["id"].as_str().unwrap().to_string())
        .unwrap();

    // No auth header.
    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .json(&serde_json::json!({ "name": "test" }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_channel_requires_manage_channels_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let _owner_token =
        setup_community(&server, &keys, &state.config, &owner_id, "ch_owner").await;

    let list_resp = server.get("/api/v1/communities").await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community_id = communities
        .iter()
        .find(|c| c["owner_id"] == owner_id)
        .map(|c| c["id"].as_str().unwrap().to_string())
        .unwrap();

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

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

#[tokio::test]
async fn create_channel_validates_name_empty() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = setup_community(&server, &keys, &state.config, &user_id, "ch_val").await;

    let list_resp = server.get("/api/v1/communities").await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community_id = communities
        .iter()
        .find(|c| c["owner_id"] == user_id)
        .map(|c| c["id"].as_str().unwrap().to_string())
        .unwrap();

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/channels"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "   " }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    // Cleanup.
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

    // Cleanup.
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
    let _token = setup_community(&server, &keys, &state.config, &user_id, "ch_lister").await;

    let list_resp = server.get("/api/v1/communities").await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community_id = communities
        .iter()
        .find(|c| c["owner_id"] == user_id)
        .map(|c| c["id"].as_str().unwrap().to_string())
        .unwrap();

    // List without auth.
    let resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    resp.assert_status_ok();

    let body: Vec<serde_json::Value> = resp.json();
    // Should have at least the default #general channel.
    assert!(!body.is_empty());
    assert_eq!(body[0]["name"], "general");

    // Cleanup.
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
    let _token = setup_community(&server, &keys, &state.config, &user_id, "ch_getter").await;

    let list_resp = server.get("/api/v1/communities").await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community_id = communities
        .iter()
        .find(|c| c["owner_id"] == user_id)
        .map(|c| c["id"].as_str().unwrap().to_string())
        .unwrap();

    // Get channel list to find a channel id.
    let channels_resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    let channels: Vec<serde_json::Value> = channels_resp.json();
    let channel_id = channels[0]["id"].as_str().unwrap();

    // Get channel by ID (public).
    let resp = server
        .get(&format!("/api/v1/channels/{channel_id}"))
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["id"], channel_id);
    assert_eq!(body["name"], "general");

    // Cleanup.
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
    let token = setup_community(&server, &keys, &state.config, &user_id, "ch_updater").await;

    let list_resp = server.get("/api/v1/communities").await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community_id = communities
        .iter()
        .find(|c| c["owner_id"] == user_id)
        .map(|c| c["id"].as_str().unwrap().to_string())
        .unwrap();

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

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn update_channel_by_non_member_returns_403() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let _owner_token =
        setup_community(&server, &keys, &state.config, &owner_id, "ch_upd_owner").await;

    let list_resp = server.get("/api/v1/communities").await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community_id = communities
        .iter()
        .find(|c| c["owner_id"] == owner_id)
        .map(|c| c["id"].as_str().unwrap().to_string())
        .unwrap();

    // Get a channel id.
    let channels_resp = server
        .get(&format!("/api/v1/communities/{community_id}/channels"))
        .await;
    let channels: Vec<serde_json::Value> = channels_resp.json();
    let channel_id = channels[0]["id"].as_str().unwrap();

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

    // Cleanup.
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
    let token = setup_community(&server, &keys, &state.config, &user_id, "ch_deleter").await;

    let list_resp = server.get("/api/v1/communities").await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community_id = communities
        .iter()
        .find(|c| c["owner_id"] == user_id)
        .map(|c| c["id"].as_str().unwrap().to_string())
        .unwrap();

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

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn delete_default_channel_returns_400() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = setup_community(&server, &keys, &state.config, &user_id, "ch_del_def").await;

    let list_resp = server.get("/api/v1/communities").await;
    let communities: Vec<serde_json::Value> = list_resp.json();
    let community = communities
        .iter()
        .find(|c| c["owner_id"] == user_id)
        .unwrap();
    let community_id = community["id"].as_str().unwrap().to_string();
    let default_channel_id = community["default_channel"].as_str().unwrap();

    // Try to delete the default channel.
    let resp = server
        .delete(&format!("/api/v1/channels/{default_channel_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);

    let body: serde_json::Value = resp.json();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("default channel"));

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}
