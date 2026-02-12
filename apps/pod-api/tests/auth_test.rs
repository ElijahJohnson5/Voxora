mod common;

use axum_test::TestServer;

// ---------------------------------------------------------------------------
// POST /api/v1/auth/login
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_with_valid_sia_returns_tokens_and_user() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let sia = common::mint_test_sia(
        &keys,
        &state.config.hub_url,
        &user_id,
        &state.config.pod_id,
        "testuser",
        "Test User",
    );

    let resp = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia }))
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["expires_in"], 3600);

    // Tokens have correct prefixes.
    let access_token = body["access_token"].as_str().unwrap();
    let refresh_token = body["refresh_token"].as_str().unwrap();
    let ws_ticket = body["ws_ticket"].as_str().unwrap();
    assert!(access_token.starts_with("pat_"), "PAT prefix");
    assert!(refresh_token.starts_with("prt_"), "PRT prefix");
    assert!(ws_ticket.starts_with("wst_"), "WST prefix");

    // User info is returned.
    assert_eq!(body["user"]["id"], user_id);
    assert_eq!(body["user"]["username"], "testuser");
    assert_eq!(body["user"]["display_name"], "Test User");

    // ws_url is present.
    assert!(body["ws_url"].as_str().unwrap().contains("gateway"));

    // Cleanup.
    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn login_upserts_existing_user() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");

    // First login creates the user.
    let sia1 = common::mint_test_sia(
        &keys,
        &state.config.hub_url,
        &user_id,
        &state.config.pod_id,
        "original_name",
        "Original Name",
    );
    let resp1 = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia1 }))
        .await;
    resp1.assert_status_ok();
    assert_eq!(resp1.json::<serde_json::Value>()["user"]["username"], "original_name");

    // Second login with updated profile upserts.
    let sia2 = common::mint_test_sia(
        &keys,
        &state.config.hub_url,
        &user_id,
        &state.config.pod_id,
        "updated_name",
        "Updated Name",
    );
    let resp2 = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia2 }))
        .await;
    resp2.assert_status_ok();
    assert_eq!(resp2.json::<serde_json::Value>()["user"]["username"], "updated_name");

    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn login_rejects_expired_sia() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let sia = common::mint_expired_sia(
        &keys,
        &state.config.hub_url,
        "usr_expired",
        &state.config.pod_id,
    );

    let resp = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia }))
        .await;

    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_rejects_wrong_audience() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let sia = common::mint_test_sia(
        &keys,
        &state.config.hub_url,
        "usr_wrong_aud",
        "pod_WRONG_POD_ID", // Wrong pod ID.
        "testuser",
        "Test User",
    );

    let resp = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia }))
        .await;

    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_rejects_wrong_issuer() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let sia = common::mint_test_sia(
        &keys,
        "http://evil-hub.example.com", // Wrong issuer.
        "usr_wrong_iss",
        &state.config.pod_id,
        "testuser",
        "Test User",
    );

    let resp = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia }))
        .await;

    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_rejects_replay_jti() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let sia = common::mint_test_sia(
        &keys,
        &state.config.hub_url,
        &user_id,
        &state.config.pod_id,
        "replay_user",
        "Replay User",
    );

    // First use succeeds.
    let resp1 = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia }))
        .await;
    resp1.assert_status_ok();

    // Replay is rejected.
    let resp2 = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia }))
        .await;
    resp2.assert_status(axum::http::StatusCode::UNAUTHORIZED);

    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn login_rejects_invalid_jwt() {
    let (app, _state, _keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": "not.a.valid.jwt" }))
        .await;

    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// POST /api/v1/auth/refresh
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refresh_rotates_tokens() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let sia = common::mint_test_sia(
        &keys,
        &state.config.hub_url,
        &user_id,
        &state.config.pod_id,
        "refresh_user",
        "Refresh User",
    );

    // Login to get a refresh token.
    let login_resp = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia }))
        .await;
    login_resp.assert_status_ok();

    let login_body: serde_json::Value = login_resp.json();
    let refresh_token = login_body["refresh_token"].as_str().unwrap();

    // Use the refresh token.
    let refresh_resp = server
        .post("/api/v1/auth/refresh")
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .await;
    refresh_resp.assert_status_ok();

    let refresh_body: serde_json::Value = refresh_resp.json();
    assert!(refresh_body["access_token"].as_str().unwrap().starts_with("pat_"));
    assert!(refresh_body["refresh_token"].as_str().unwrap().starts_with("prt_"));
    assert_eq!(refresh_body["token_type"], "Bearer");
    assert_eq!(refresh_body["expires_in"], 3600);

    // New refresh token is different from the old one.
    assert_ne!(refresh_body["refresh_token"].as_str().unwrap(), refresh_token);

    // Old refresh token is consumed — using it again fails.
    let replay_resp = server
        .post("/api/v1/auth/refresh")
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .await;
    replay_resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);

    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn refresh_rejects_invalid_token() {
    let (app, _state, _keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/auth/refresh")
        .json(&serde_json::json!({ "refresh_token": "prt_fake_token" }))
        .await;

    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}
