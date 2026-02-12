//! Integration tests for H-5: SIA (Signed Identity Assertion) issuance.

mod common;

use axum::http::StatusCode;
use axum_test::TestServer;

// =========================================================================
// POST /api/v1/oidc/sia — happy path
// =========================================================================

#[tokio::test]
async fn issue_sia_returns_valid_jwt() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "sia_test_password").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile", "email", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/oidc/sia")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "pod_id": pod_id }))
        .await;

    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let sia_jwt = body["sia"].as_str().expect("sia field must be a string");
    let expires_at = body["expires_at"]
        .as_str()
        .expect("expires_at field must be a string");

    // SIA should be a valid JWT
    assert!(!sia_jwt.is_empty());
    assert!(
        sia_jwt.split('.').count() == 3,
        "SIA must be a 3-part JWT"
    );

    // expires_at should be a valid RFC3339 timestamp
    assert!(
        chrono::DateTime::parse_from_rfc3339(expires_at).is_ok(),
        "expires_at must be RFC3339"
    );

    // Decode and validate the SIA JWT using our keys
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    validation.set_audience(&[&pod_id]);
    validation.set_issuer(&[&state.config.hub_domain]);

    let decoded = jsonwebtoken::decode::<hub_api::auth::sia::SiaClaims>(
        sia_jwt,
        &state.keys.decoding,
        &validation,
    )
    .expect("SIA JWT must be valid");

    let claims = decoded.claims;
    assert_eq!(claims.sub, user.id);
    assert_eq!(claims.aud, pod_id);
    assert_eq!(claims.iss, state.config.hub_domain);
    assert_eq!(claims.username, user.username);
    assert!(claims.jti.starts_with("sia_"), "jti must have sia_ prefix");
    assert_eq!(claims.hub_version, 1);
    assert_eq!(claims.email.as_deref(), Some(user.email.as_str()));

    // Verify header has correct typ
    let header = jsonwebtoken::decode_header(sia_jwt).unwrap();
    assert_eq!(header.typ.as_deref(), Some("voxora-sia+jwt"));
    assert_eq!(header.kid.as_deref(), Some(state.keys.kid.as_str()));

    // TTL should be ~5 minutes
    let ttl = claims.exp - claims.iat;
    assert_eq!(ttl, 300, "SIA TTL must be 300 seconds");

    // Cleanup
    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// Auth checks
// =========================================================================

#[tokio::test]
async fn sia_requires_bearer_token() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/oidc/sia")
        .json(&serde_json::json!({ "pod_id": "pod_01TEST" }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sia_requires_pods_scope() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "scope_test_pw123").await;

    // Token WITHOUT pods scope
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "profile"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/oidc/sia")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "pod_id": "pod_01TESTPOD" }))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// Pod validation
// =========================================================================

#[tokio::test]
async fn sia_rejects_nonexistent_pod() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "no_pod_test_pw1").await;
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/oidc/sia")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "pod_id": "pod_01NONEXISTENT000000000000" }))
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn sia_rejects_invalid_pod_id_format() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "bad_pod_id_pw12").await;
    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/oidc/sia")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "pod_id": "not_a_pod_id" }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn sia_rejects_inactive_pod() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "inactive_pod_pw1").await;
    let pod_id = common::create_test_pod(&state.db, &user.id).await;

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

    let token = common::store_test_access_token(
        state.kv.as_ref(),
        &user.id,
        &["openid", "pods"],
    )
    .await;

    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/v1/oidc/sia")
        .authorization_bearer(&token)
        .json(&serde_json::json!({ "pod_id": pod_id }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not active"));

    common::cleanup_test_pod(&state.db, &pod_id).await;
    common::cleanup_test_user(&state.db, &user.id).await;
}
