//! Integration tests for H-6: Pod Registry.

mod common;

use axum::http::StatusCode;
use axum_test::TestServer;

// =========================================================================
// POST /api/v1/pods/register — happy path
// =========================================================================

#[tokio::test]
async fn register_pod_returns_credentials() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "reg_pod_pw_123").await;
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/pods/register")
        .authorization_bearer(&token)
        .json(&serde_json::json!({
            "name": "My Test Pod",
            "url": "https://pod.example.com",
            "description": "A test pod",
            "public": true,
            "capabilities": ["text"],
            "max_members": 500,
            "version": "1.0.0"
        }))
        .await;

    resp.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = resp.json();

    let pod_id = body["pod_id"].as_str().expect("pod_id present");
    assert!(pod_id.starts_with("pod_"), "pod_id must have pod_ prefix");

    let client_id = body["client_id"].as_str().expect("client_id present");
    assert!(
        client_id.starts_with("pod_client_"),
        "client_id must have pod_client_ prefix"
    );

    let client_secret = body["client_secret"]
        .as_str()
        .expect("client_secret present");
    assert!(
        client_secret.starts_with("vxs_"),
        "client_secret must have vxs_ prefix"
    );

    assert_eq!(body["status"].as_str(), Some("active"));
    assert!(body["registered_at"].as_str().is_some());

    // Cleanup
    common::cleanup_test_pod(&state.db, pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// POST /api/v1/pods/register — requires auth
// =========================================================================

#[tokio::test]
async fn register_pod_requires_auth() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/pods/register")
        .json(&serde_json::json!({
            "name": "No Auth Pod",
            "url": "https://pod.example.com"
        }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}

// =========================================================================
// POST /api/v1/pods/register — validation errors
// =========================================================================

#[tokio::test]
async fn register_pod_validates_fields() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "val_pod_pw_1234").await;
    let token =
        common::store_test_access_token(state.kv.as_ref(), &user.id, &["openid", "pods"]).await;

    let server = TestServer::new(app).unwrap();

    // Missing/empty name
    let resp = server
        .post("/api/v1/pods/register")
        .authorization_bearer(&token)
        .json(&serde_json::json!({
            "name": "",
            "url": "https://pod.example.com"
        }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["error"]["code"].as_str(), Some("VALIDATION_ERROR"));

    // Invalid URL
    let resp = server
        .post("/api/v1/pods/register")
        .authorization_bearer(&token)
        .json(&serde_json::json!({
            "name": "Good Name",
            "url": "ftp://nope.example.com"
        }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert!(body["error"]["details"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["field"] == "url"));

    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// GET /api/v1/pods — list pods
// =========================================================================

#[tokio::test]
async fn list_pods_returns_active_public_pods() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "list_pods_pw_12").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;

    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/v1/pods").await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let data = body["data"].as_array().expect("data is an array");

    // Our test pod should appear (it's created as active).
    let found = data.iter().any(|p| p["id"].as_str() == Some(&pod_id));
    assert!(found, "test pod should appear in listing");

    // Verify no client_secret or client_id leaked in public response
    for pod in data {
        assert!(
            pod.get("client_secret").is_none(),
            "client_secret must not appear in listing"
        );
        assert!(
            pod.get("client_id").is_none(),
            "client_id must not appear in listing"
        );
    }

    assert!(body.get("has_more").is_some());

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// GET /api/v1/pods — pagination
// =========================================================================

#[tokio::test]
async fn list_pods_supports_limit() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "limit_pods_pw12").await;

    // Create 3 pods.
    let mut pod_ids = Vec::new();
    for _ in 0..3 {
        pod_ids.push(common::create_test_pod(&state.db, &user.id).await);
    }

    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/v1/pods?limit=2&sort=newest").await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let data = body["data"].as_array().unwrap();
    assert!(data.len() <= 2, "limit should cap results");

    for pod_id in &pod_ids {
        common::cleanup_test_pod(&state.db, pod_id).await;
    }
    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// GET /api/v1/pods/{pod_id} — details
// =========================================================================

#[tokio::test]
async fn get_pod_returns_details() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "get_pod_pw_1234").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;

    let server = TestServer::new(app).unwrap();

    let resp = server.get(&format!("/api/v1/pods/{pod_id}")).await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["id"].as_str(), Some(pod_id.as_str()));
    assert_eq!(body["owner_id"].as_str(), Some(user.id.as_str()));
    assert!(body.get("client_secret").is_none());
    assert!(body.get("client_id").is_none());

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn get_pod_returns_404_for_missing() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/api/v1/pods/pod_01NONEXISTENT000000000000")
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);
}

// =========================================================================
// POST /api/v1/pods/heartbeat — happy path
// =========================================================================

#[tokio::test]
async fn heartbeat_updates_pod() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "hb_pod_pw_12345").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;

    // Retrieve the client_secret so we can authenticate.
    let secret = {
        use diesel::prelude::*;
        use diesel_async::RunQueryDsl;
        let mut conn = state.db.get().await.unwrap();
        hub_api::db::schema::pods::table
            .find(&pod_id)
            .select(hub_api::db::schema::pods::client_secret)
            .first::<String>(&mut conn)
            .await
            .unwrap()
    };

    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/pods/heartbeat")
        .authorization_bearer(&secret)
        .json(&serde_json::json!({
            "member_count": 42,
            "online_count": 7,
            "community_count": 3,
            "version": "1.1.0"
        }))
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["ok"].as_bool(), Some(true));
    assert!(body["recorded_at"].as_str().is_some());

    // Verify the counters were updated in the database.
    {
        use diesel::prelude::*;
        use diesel_async::RunQueryDsl;
        let mut conn = state.db.get().await.unwrap();
        let pod: hub_api::models::pod::Pod = hub_api::db::schema::pods::table
            .find(&pod_id)
            .select(hub_api::models::pod::Pod::as_select())
            .first(&mut conn)
            .await
            .unwrap();

        assert_eq!(pod.member_count, 42);
        assert_eq!(pod.online_count, 7);
        assert_eq!(pod.community_count, 3);
        assert_eq!(pod.version.as_deref(), Some("1.1.0"));
        assert!(pod.last_heartbeat.is_some());
    }

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// POST /api/v1/pods/heartbeat — auth failures
// =========================================================================

#[tokio::test]
async fn heartbeat_rejects_missing_auth() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/pods/heartbeat")
        .json(&serde_json::json!({ "member_count": 1 }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn heartbeat_rejects_wrong_secret() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/pods/heartbeat")
        .authorization_bearer("vxs_wrong_secret")
        .json(&serde_json::json!({ "member_count": 1 }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}
