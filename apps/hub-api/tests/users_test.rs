//! Integration tests for H-7: User Profiles.

mod common;

use axum::http::StatusCode;
use axum_test::TestServer;

// =========================================================================
// GET /api/v1/users/@me — current user
// =========================================================================

#[tokio::test]
async fn get_me_returns_current_user() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "get_me_pw_12345").await;
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "email"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/api/v1/users/@me")
        .authorization_bearer(&token)
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["id"].as_str(), Some(user.id.as_str()));
    assert_eq!(body["username"].as_str(), Some(user.username.as_str()));
    assert_eq!(body["email"].as_str(), Some(user.email.as_str()));
    // Full response includes email, email_verified, status, updated_at
    assert!(body.get("email_verified").is_some());
    assert!(body.get("status").is_some());
    assert!(body.get("updated_at").is_some());

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_me_requires_auth() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/v1/users/@me").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

// =========================================================================
// PATCH /api/v1/users/@me — update profile
// =========================================================================

#[tokio::test]
async fn update_me_changes_display_name() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "update_dn_pw_12").await;
    let token =
        common::store_test_access_token(state.kv.as_ref(), &user.id, &["openid", "profile"]).await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "display_name": "New Display Name" }))
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["display_name"].as_str(), Some("New Display Name"));
    assert_eq!(body["id"].as_str(), Some(user.id.as_str()));

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_me_changes_avatar_url() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "update_av_pw_12").await;
    let token =
        common::store_test_access_token(state.kv.as_ref(), &user.id, &["openid", "profile"]).await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "avatar_url": "https://example.com/avatar.png" }))
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(
        body["avatar_url"].as_str(),
        Some("https://example.com/avatar.png")
    );

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_me_clears_avatar_with_empty_string() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "clr_avatar_pw12").await;
    let token =
        common::store_test_access_token(state.kv.as_ref(), &user.id, &["openid", "profile"]).await;

    let server = TestServer::new(app).unwrap();

    // First set an avatar
    server
        .patch("/api/v1/users/@me")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "avatar_url": "https://example.com/a.png" }))
        .await;

    // Then clear it
    let resp = server
        .patch("/api/v1/users/@me")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "avatar_url": "" }))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert!(body["avatar_url"].is_null());

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_me_validates_display_name_length() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "val_dn_pw_12345").await;
    let token =
        common::store_test_access_token(state.kv.as_ref(), &user.id, &["openid", "profile"]).await;

    let server = TestServer::new(app).unwrap();

    // Empty display name
    let resp = server
        .patch("/api/v1/users/@me")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "display_name": "" }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"].as_str(), Some("VALIDATION_ERROR"));

    // Too long display name (>64 chars)
    let long_name = "a".repeat(65);
    let resp = server
        .patch("/api/v1/users/@me")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "display_name": long_name }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_me_validates_avatar_url_scheme() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "val_av_pw_12345").await;
    let token =
        common::store_test_access_token(state.kv.as_ref(), &user.id, &["openid", "profile"]).await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "avatar_url": "ftp://bad-scheme.com/img.png" }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_me_requires_auth() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me")
        .json(&serde_json::json!({ "display_name": "Anon" }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_me_with_no_changes_returns_current() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "noop_upd_pw_123").await;
    let token =
        common::store_test_access_token(state.kv.as_ref(), &user.id, &["openid", "profile"]).await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me")
        .authorization_bearer(&token)
        .json(&serde_json::json!({}))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["id"].as_str(), Some(user.id.as_str()));

    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// GET /api/v1/users/{user_id} — public profile
// =========================================================================

#[tokio::test]
async fn get_user_returns_public_profile() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pub_prof_pw_123").await;

    let server = TestServer::new(app).unwrap();

    let resp = server.get(&format!("/api/v1/users/{}", user.id)).await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["id"].as_str(), Some(user.id.as_str()));
    assert_eq!(body["username"].as_str(), Some(user.username.as_str()));
    assert!(body.get("display_name").is_some());
    assert!(body.get("created_at").is_some());

    // Public profile must NOT include private fields
    assert!(
        body.get("email").is_none(),
        "email must not appear in public profile"
    );
    assert!(
        body.get("email_verified").is_none(),
        "email_verified must not appear in public profile"
    );
    assert!(
        body.get("status").is_none(),
        "status must not appear in public profile"
    );
    assert!(
        body.get("updated_at").is_none(),
        "updated_at must not appear in public profile"
    );

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_user_does_not_require_auth() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "no_auth_prof_12").await;

    let server = TestServer::new(app).unwrap();

    // No Authorization header
    let resp = server.get(&format!("/api/v1/users/{}", user.id)).await;
    resp.assert_status_ok();

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_user_returns_404_for_unknown() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/api/v1/users/usr_01NONEXISTENT000000000000")
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);
}

// =========================================================================
// GET /api/v1/users/@me/pods — bookmarked pods
// =========================================================================

#[tokio::test]
async fn get_my_pods_returns_bookmarked_pods() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "my_pods_pw_1234").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;

    // Create a bookmark
    common::create_test_bookmark(&state.db, &user.id, &pod_id).await;

    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/api/v1/users/@me/pods")
        .authorization_bearer(&token)
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let data = body["data"].as_array().expect("data array");
    assert!(!data.is_empty(), "should have at least one bookmarked pod");

    let found = data.iter().any(|p| p["id"].as_str() == Some(&pod_id));
    assert!(found, "bookmarked pod should appear in results");

    // No sensitive fields leaked
    for pod in data {
        assert!(pod.get("client_secret").is_none());
        assert!(pod.get("client_id").is_none());
    }

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_my_pods_returns_empty_when_none() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "no_pods_pw_1234").await;
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/api/v1/users/@me/pods")
        .authorization_bearer(&token)
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let data = body["data"].as_array().expect("data array");
    assert!(data.is_empty(), "should have no bookmarked pods");

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_my_pods_excludes_inactive_pods() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "inact_pod_pw_12").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;

    // Bookmark it
    common::create_test_bookmark(&state.db, &user.id, &pod_id).await;

    // Set pod to inactive
    {
        use diesel::prelude::*;
        use diesel_async::RunQueryDsl;
        let mut conn = state.db.get().await.unwrap();
        diesel::update(
            hub_api::db::schema::pods::table.filter(hub_api::db::schema::pods::id.eq(&pod_id)),
        )
        .set(hub_api::db::schema::pods::status.eq("suspended"))
        .execute(&mut conn)
        .await
        .unwrap();
    }

    let token =
        common::store_test_access_token(state.kv.as_ref(), &user.id, &["openid", "pods"]).await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/api/v1/users/@me/pods")
        .authorization_bearer(&token)
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let data = body["data"].as_array().unwrap();
    let found = data.iter().any(|p| p["id"].as_str() == Some(&pod_id));
    assert!(!found, "inactive pods should not appear in results");

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_my_pods_requires_auth() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/v1/users/@me/pods").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}
