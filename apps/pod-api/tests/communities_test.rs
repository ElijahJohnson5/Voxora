mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ---------------------------------------------------------------------------
// POST /api/v1/communities
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_community_returns_community_with_channels_and_roles() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = common::login_test_user(&server, &keys, &state.config, &user_id, "creator").await;

    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Test Community",
            "description": "A test community"
        }))
        .await;

    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();

    // Community fields.
    assert!(body["id"].as_str().unwrap().starts_with("com_"));
    assert_eq!(body["name"], "Test Community");
    assert_eq!(body["description"], "A test community");
    assert_eq!(body["owner_id"], user_id);
    assert_eq!(body["member_count"], 1);
    assert!(body["default_channel"].as_str().is_some());

    // Channels.
    let channels = body["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["name"], "general");
    assert_eq!(channels[0]["type"], 0);

    // Roles.
    let roles = body["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0]["name"], "@everyone");
    assert_eq!(roles[0]["is_default"], true);

    // Cleanup.
    common::cleanup_community(&state.db, body["id"].as_str().unwrap()).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_community_requires_auth() {
    let (app, _state, _keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/communities")
        .json(&serde_json::json!({ "name": "No Auth Community" }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_community_validates_name_empty() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = common::login_test_user(&server, &keys, &state.config, &user_id, "validator").await;

    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "   " }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_community_validates_name_too_long() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token =
        common::login_test_user(&server, &keys, &state.config, &user_id, "validator2").await;

    let long_name = "a".repeat(101);
    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": long_name }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    common::cleanup_test_user(&state.db, &user_id).await;
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_communities_is_public() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Create a community to ensure list is non-empty.
    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = common::login_test_user(&server, &keys, &state.config, &user_id, "lister").await;

    let create_resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Listed Community" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let community_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // List without auth.
    let resp = server.get("/api/v1/communities").await;
    resp.assert_status_ok();

    let body: Vec<serde_json::Value> = resp.json();
    assert!(body.iter().any(|c| c["id"] == community_id));

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_community_returns_nested_data() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = common::login_test_user(&server, &keys, &state.config, &user_id, "getter").await;

    let create_resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Detailed Community" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let community_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // GET without auth.
    let resp = server
        .get(&format!("/api/v1/communities/{community_id}"))
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["id"], community_id);
    assert_eq!(body["name"], "Detailed Community");
    assert!(body["channels"].as_array().unwrap().len() >= 1);
    assert!(body["roles"].as_array().unwrap().len() >= 1);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn get_community_returns_404_for_nonexistent() {
    let (app, _state, _keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/api/v1/communities/com_DOES_NOT_EXIST")
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/communities/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_community_by_owner_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = common::login_test_user(&server, &keys, &state.config, &user_id, "updater").await;

    let create_resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Original Name" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let community_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = server
        .patch(&format!("/api/v1/communities/{community_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Updated Name",
            "description": "New description"
        }))
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["name"], "Updated Name");
    assert_eq!(body["description"], "New description");

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn update_community_by_non_member_returns_403() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let owner_token =
        common::login_test_user(&server, &keys, &state.config, &owner_id, "owner").await;

    let create_resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "name": "Owner's Community" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let community_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Non-member tries to update.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "outsider").await;

    let resp = server
        .patch(&format!("/api/v1/communities/{community_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .json(&serde_json::json!({ "name": "Hijacked" }))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_community_by_owner_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = common::login_test_user(&server, &keys, &state.config, &user_id, "deleter").await;

    let create_resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Doomed Community" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let community_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete.
    let resp = server
        .delete(&format!("/api/v1/communities/{community_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify 404 after deletion.
    let get_resp = server
        .get(&format!("/api/v1/communities/{community_id}"))
        .await;
    get_resp.assert_status(StatusCode::NOT_FOUND);

    // Cleanup user only (community already deleted).
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn delete_community_by_non_owner_returns_403() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let owner_token =
        common::login_test_user(&server, &keys, &state.config, &owner_id, "del_owner").await;

    let create_resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "name": "Protected Community" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let community_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Non-owner tries to delete.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "del_other").await;

    let resp = server
        .delete(&format!("/api/v1/communities/{community_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}
