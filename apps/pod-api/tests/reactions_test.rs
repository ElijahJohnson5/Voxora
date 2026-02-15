mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ---------------------------------------------------------------------------
// PUT /api/v1/channels/:channel_id/messages/:message_id/reactions/:emoji
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_reaction_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, token) =
        common::setup_with_message(&server, &keys, &state.config, &user_id, "rxn_add").await;

    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/%F0%9F%91%8D"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["message_id"].as_str().unwrap(), message_id);
    assert_eq!(body["user_id"], user_id);
    assert!(body["created_at"].is_string());

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn add_reaction_requires_auth() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, _token) =
        common::setup_with_message(&server, &keys, &state.config, &user_id, "rxn_noauth").await;

    // No auth header.
    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/%F0%9F%91%8D"
        ))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn add_reaction_requires_use_reactions_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Owner creates community + message.
    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, _owner_token) =
        common::setup_with_message(&server, &keys, &state.config, &owner_id, "rxn_owner").await;

    // Non-member tries to react.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "rxn_outsider").await;

    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/%F0%9F%91%8D"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

#[tokio::test]
async fn add_reaction_returns_404_for_nonexistent_message() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token =
        common::login_test_user(&server, &keys, &state.config, &user_id, "rxn_404").await;

    // Create community to get a valid channel.
    let resp = server
        .post("/api/v1/communities")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Reaction 404 Community" }))
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
            "/api/v1/channels/{channel_id}/messages/9999999999/reactions/%F0%9F%91%8D"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn add_duplicate_reaction_is_idempotent() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, token) =
        common::setup_with_message(&server, &keys, &state.config, &user_id, "rxn_dup").await;

    let url = format!(
        "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/%F0%9F%91%8D"
    );

    // Add reaction twice.
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

    // Both should return the same reaction.
    let body1: serde_json::Value = resp1.json();
    let body2: serde_json::Value = resp2.json();
    assert_eq!(body1["created_at"], body2["created_at"]);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn add_reaction_enforces_max_20_unique_emoji() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, token) =
        common::setup_with_message(&server, &keys, &state.config, &user_id, "rxn_max").await;

    // Add 20 unique emoji.
    let emojis: Vec<String> = (0..20).map(|i| format!("e{i}")).collect();
    for emoji in &emojis {
        let resp = server
            .put(&format!(
                "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}"
            ))
            .add_header(AUTHORIZATION, format!("Bearer {token}"))
            .await;
        resp.assert_status_ok();
    }

    // 21st unique emoji should fail.
    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/e20"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);

    // But re-adding an existing emoji should still work.
    let resp = server
        .put(&format!(
            "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/e0"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status_ok();

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:channel_id/messages/:message_id/reactions/:emoji
// ---------------------------------------------------------------------------

#[tokio::test]
async fn remove_reaction_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, token) =
        common::setup_with_message(&server, &keys, &state.config, &user_id, "rxn_rm").await;

    let url = format!(
        "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/%F0%9F%91%8D"
    );

    // Add then remove.
    server
        .put(&url)
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await
        .assert_status_ok();

    let resp = server
        .delete(&url)
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone via list.
    let list_resp = server.get(&url).await;
    list_resp.assert_status_ok();
    let reactions: Vec<serde_json::Value> = list_resp.json();
    assert!(reactions.is_empty());

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn remove_nonexistent_reaction_returns_204() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, channel_id, message_id, token) =
        common::setup_with_message(&server, &keys, &state.config, &user_id, "rxn_rm_noop").await;

    // Remove a reaction that was never added.
    let resp = server
        .delete(&format!(
            "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/%F0%9F%91%8D"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::NO_CONTENT);

    // Cleanup.
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &user_id).await;
}
