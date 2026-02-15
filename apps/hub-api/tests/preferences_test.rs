mod common;

use axum_test::TestServer;

// =========================================================================
// GET /api/v1/users/@me/preferences
// =========================================================================

#[tokio::test]
async fn get_preferences_returns_defaults_for_new_user() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_a").await;
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();
    let resp = server
        .get("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["preferred_pods"], serde_json::json!([]));
    assert_eq!(body["max_preferred_pods"], 10);

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_preferences_returns_saved_pods() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_b").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;
    common::create_test_bookmark(&state.db, &user.id, &pod_id).await;

    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    // First set preferences via PATCH
    let patch_resp = server
        .patch("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "preferred_pods": [&pod_id] }))
        .await;
    patch_resp.assert_status_ok();

    // Then GET should return the saved pods
    let resp = server
        .get("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    let prefs = body["preferred_pods"].as_array().unwrap();
    assert_eq!(prefs.len(), 1);
    assert_eq!(prefs[0].as_str().unwrap(), pod_id);

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_preferences_requires_auth() {
    let (app, _state) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/v1/users/@me/preferences").await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

// =========================================================================
// PATCH /api/v1/users/@me/preferences
// =========================================================================

#[tokio::test]
async fn update_preferences_sets_preferred_pods() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_c").await;
    let pod1 = common::create_test_pod(&state.db, &user.id).await;
    let pod2 = common::create_test_pod(&state.db, &user.id).await;
    common::create_test_bookmark(&state.db, &user.id, &pod1).await;
    common::create_test_bookmark(&state.db, &user.id, &pod2).await;

    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "preferred_pods": [&pod1, &pod2] }))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    let prefs = body["preferred_pods"].as_array().unwrap();
    assert_eq!(prefs.len(), 2);
    assert!(prefs.iter().any(|p| p.as_str() == Some(&*pod1)));
    assert!(prefs.iter().any(|p| p.as_str() == Some(&*pod2)));
    assert_eq!(body["max_preferred_pods"], 10);

    common::cleanup_test_pod(&state.db, &pod1).await;
    common::cleanup_test_pod(&state.db, &pod2).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_preferences_requires_auth() {
    let (app, _state) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me/preferences")
        .json(&serde_json::json!({ "preferred_pods": [] }))
        .await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_preferences_validates_max_10() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_d").await;
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    // Create 11 fake pod IDs
    let ids: Vec<String> = (0..11).map(|i| format!("pod_fake_{i}")).collect();

    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "preferred_pods": ids }))
        .await;

    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_preferences_validates_pods_exist() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_e").await;
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "preferred_pods": ["pod_nonexistent_123"] }))
        .await;

    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "BAD_REQUEST");

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_preferences_validates_pods_bookmarked() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_f").await;
    // Pod exists but user has NOT bookmarked it
    let pod_id = common::create_test_pod(&state.db, &user.id).await;

    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "preferred_pods": [&pod_id] }))
        .await;

    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "BAD_REQUEST");

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_preferences_clears_with_empty_array() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_g").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;
    common::create_test_bookmark(&state.db, &user.id, &pod_id).await;

    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    // First, set a preferred pod
    let resp = server
        .patch("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "preferred_pods": [&pod_id] }))
        .await;
    resp.assert_status_ok();

    // Then clear with empty array
    let resp = server
        .patch("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "preferred_pods": [] }))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["preferred_pods"], serde_json::json!([]));

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn update_preferences_validates_inactive_pod() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_h").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;
    common::create_test_bookmark(&state.db, &user.id, &pod_id).await;

    // Mark the pod as inactive
    {
        use diesel::prelude::*;
        use diesel_async::RunQueryDsl;
        let mut conn = state.db.get().await.expect("pool");
        diesel::update(
            hub_api::db::schema::pods::table.filter(hub_api::db::schema::pods::id.eq(&pod_id)),
        )
        .set(hub_api::db::schema::pods::status.eq("inactive"))
        .execute(&mut conn)
        .await
        .expect("deactivate pod");
    }

    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .patch("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "preferred_pods": [&pod_id] }))
        .await;

    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"], "BAD_REQUEST");

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// GET /api/v1/users/@me/pods — Extended response
// =========================================================================

#[tokio::test]
async fn get_my_pods_includes_preferred_field() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_i").await;
    let pod1 = common::create_test_pod(&state.db, &user.id).await;
    let pod2 = common::create_test_pod(&state.db, &user.id).await;
    common::create_test_bookmark(&state.db, &user.id, &pod1).await;
    common::create_test_bookmark(&state.db, &user.id, &pod2).await;

    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    // Mark pod1 as preferred
    let resp = server
        .patch("/api/v1/users/@me/preferences")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "preferred_pods": [&pod1] }))
        .await;
    resp.assert_status_ok();

    // GET my pods
    let resp = server
        .get("/api/v1/users/@me/pods")
        .authorization_bearer(&token)
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let data = body["data"].as_array().expect("data array");

    let p1 = data.iter().find(|p| p["id"].as_str() == Some(&*pod1)).unwrap();
    let p2 = data.iter().find(|p| p["id"].as_str() == Some(&*pod2)).unwrap();

    assert_eq!(p1["preferred"], true);
    assert_eq!(p2["preferred"], false);

    common::cleanup_test_pod(&state.db, &pod1).await;
    common::cleanup_test_pod(&state.db, &pod2).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_my_pods_includes_relay_field() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "pref_pw_1234_j").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;
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

    for pod in data {
        assert_eq!(pod["relay"], false, "relay should be false for all pods in Phase 2");
    }

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}
