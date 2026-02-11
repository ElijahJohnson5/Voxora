//! Integration tests for OIDC endpoints and the full authorization code + PKCE flow.
//!
//! These tests hit the real PostgreSQL and Redis instances configured in `.env`.

mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

/// Helper: compute S256 code_challenge from code_verifier.
fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

// =========================================================================
// Discovery + JWKS (stateless)
// =========================================================================

#[tokio::test]
async fn discovery_returns_valid_document() {
    let (app, _state) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/.well-known/openid-configuration").await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["response_types_supported"][0], "code");
    assert_eq!(body["grant_types_supported"][0], "authorization_code");
    assert_eq!(body["id_token_signing_alg_values_supported"][0], "EdDSA");
    assert_eq!(body["code_challenge_methods_supported"][0], "S256");
    assert!(body["issuer"].as_str().unwrap().starts_with("http"));
    assert!(body["token_endpoint"].as_str().is_some());
    assert!(body["jwks_uri"].as_str().is_some());
    assert!(body["userinfo_endpoint"].as_str().is_some());
}

#[tokio::test]
async fn jwks_returns_ed25519_key() {
    let (app, state) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/oidc/.well-known/jwks.json").await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    let key = &body["keys"][0];
    assert_eq!(key["kty"], "OKP");
    assert_eq!(key["crv"], "Ed25519");
    assert_eq!(key["use"], "sig");
    assert_eq!(key["kid"].as_str().unwrap(), state.keys.kid);
    assert_eq!(key["x"].as_str().unwrap(), state.keys.public_key_b64);
}

// =========================================================================
// GET /oidc/authorize — parameter validation
// =========================================================================

#[tokio::test]
async fn authorize_rejects_missing_pkce() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/oidc/authorize")
        .add_query_param("response_type", "code")
        .add_query_param("client_id", "voxora-web")
        .add_query_param("redirect_uri", "http://localhost:5173/callback")
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let text = resp.text();
    assert!(text.contains("PKCE"), "should mention PKCE requirement");
}

#[tokio::test]
async fn authorize_rejects_bad_response_type() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/oidc/authorize")
        .add_query_param("response_type", "token")
        .add_query_param("client_id", "voxora-web")
        .add_query_param("redirect_uri", "http://localhost:5173/callback")
        .add_query_param("code_challenge", "abc")
        .add_query_param("code_challenge_method", "S256")
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn authorize_rejects_unknown_client_id() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/oidc/authorize")
        .add_query_param("response_type", "code")
        .add_query_param("client_id", "unknown-app")
        .add_query_param("redirect_uri", "http://localhost:5173/callback")
        .add_query_param("code_challenge", "abc")
        .add_query_param("code_challenge_method", "S256")
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn authorize_renders_login_form() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/oidc/authorize")
        .add_query_param("response_type", "code")
        .add_query_param("client_id", "voxora-web")
        .add_query_param("redirect_uri", "http://localhost:5173/callback")
        .add_query_param("scope", "openid profile email")
        .add_query_param("state", "st123")
        .add_query_param(
            "code_challenge",
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        )
        .add_query_param("code_challenge_method", "S256")
        .await;

    resp.assert_status_ok();
    let html = resp.text();
    assert!(html.contains("<form"), "should contain a login form");
    assert!(html.contains("voxora-web"));
}

// =========================================================================
// POST /oidc/authorize — credential validation
// =========================================================================

#[tokio::test]
async fn authorize_submit_rejects_wrong_password() {
    let (app, state) = common::test_app().await;
    let user = common::create_test_user(&state.db, "correctpassword").await;
    let server = TestServer::new(app).unwrap();

    let challenge = pkce_challenge("my_verifier_string_that_is_long_enough");

    let resp = server
        .post("/oidc/authorize")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "response_type=code&client_id=voxora-web&redirect_uri=http://localhost:5173/cb\
             &scope=openid&code_challenge={challenge}&code_challenge_method=S256\
             &login={}&password=wrongpassword",
                user.username
            )
            .into(),
        )
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    common::cleanup_test_user(&state.db, &user.id).await;
}

#[tokio::test]
async fn authorize_submit_rejects_unknown_user() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let challenge = pkce_challenge("verifier123456789012345678901234");

    let resp = server
        .post("/oidc/authorize")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "response_type=code&client_id=voxora-web&redirect_uri=http://localhost:5173/cb\
             &scope=openid&code_challenge={challenge}&code_challenge_method=S256\
             &login=nonexistentuser99999&password=doesntmatter"
            )
            .into(),
        )
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Full Authorization Code + PKCE flow
// =========================================================================

#[tokio::test]
async fn full_auth_code_flow() {
    let (app, state) = common::test_app().await;
    let password = "test_password_123";
    let user = common::create_test_user(&state.db, password).await;
    let server = TestServer::new(app).unwrap();

    let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let code_challenge = pkce_challenge(code_verifier);
    let redirect_uri = "http://localhost:5173/callback";

    // --- Step 1: POST /oidc/authorize → get auth code via redirect ---
    let resp = server
        .post("/oidc/authorize")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "response_type=code&client_id=voxora-web&redirect_uri={redirect_uri}\
             &scope=openid+profile+email&state=s1&code_challenge={code_challenge}\
             &code_challenge_method=S256&nonce=n1&login={}&password={password}",
                user.username
            )
            .into(),
        )
        .await;

    resp.assert_status(StatusCode::SEE_OTHER);
    let location = resp.header("location").to_str().unwrap().to_string();
    assert!(location.starts_with(redirect_uri), "redirect to client");
    assert!(location.contains("code=hac_"), "must contain auth code");
    assert!(location.contains("state=s1"), "must echo state");

    // Extract the code from the redirect URL
    let code = location
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();

    // --- Step 2: POST /oidc/token (authorization_code grant) ---
    let resp = server
        .post("/oidc/token")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "grant_type=authorization_code&code={code}&redirect_uri={redirect_uri}\
             &code_verifier={code_verifier}&client_id=voxora-web"
            )
            .into(),
        )
        .await;

    resp.assert_status_ok();
    let token_body: serde_json::Value = resp.json();

    assert_eq!(token_body["token_type"], "Bearer");
    assert_eq!(token_body["expires_in"], 900);
    assert!(token_body["scope"].as_str().unwrap().contains("openid"));

    let access_token = token_body["access_token"].as_str().unwrap();
    let refresh_token = token_body["refresh_token"].as_str().unwrap();
    let id_token = token_body["id_token"].as_str().unwrap();

    assert!(access_token.starts_with("hat_"));
    assert!(refresh_token.starts_with("hrt_"));

    // Verify the ID token can be decoded with our keys
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    validation.set_audience(&["voxora-web"]);
    validation.set_issuer(&[&state.config.hub_domain]);
    let id_claims = jsonwebtoken::decode::<hub_api::auth::tokens::IdTokenClaims>(
        id_token,
        &state.keys.decoding,
        &validation,
    )
    .expect("ID token verification must succeed");
    assert_eq!(id_claims.claims.sub, user.id);
    assert_eq!(
        id_claims.claims.preferred_username.as_deref(),
        Some(user.username.as_str())
    );
    assert_eq!(id_claims.claims.nonce.as_deref(), Some("n1"));

    // --- Step 3: GET /oidc/userinfo with the access token ---
    let resp = server
        .get("/oidc/userinfo")
        .authorization_bearer(access_token)
        .await;

    resp.assert_status_ok();
    let userinfo: serde_json::Value = resp.json();
    assert_eq!(userinfo["sub"], user.id);
    assert_eq!(userinfo["preferred_username"], user.username);
    assert_eq!(userinfo["email"], user.email);

    // --- Step 4: POST /oidc/token (refresh_token grant) ---
    let resp = server
        .post("/oidc/token")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!("grant_type=refresh_token&refresh_token={refresh_token}&client_id=voxora-web")
                .into(),
        )
        .await;

    resp.assert_status_ok();
    let refresh_body: serde_json::Value = resp.json();
    let new_at = refresh_body["access_token"].as_str().unwrap();
    let new_rt = refresh_body["refresh_token"].as_str().unwrap();
    assert!(new_at.starts_with("hat_"));
    assert!(new_rt.starts_with("hrt_"));
    assert_ne!(new_at, access_token, "new access token must differ");
    assert_ne!(new_rt, refresh_token, "refresh token must rotate");
    // No id_token on refresh
    assert!(refresh_body.get("id_token").is_none() || refresh_body["id_token"].is_null());

    // --- Step 5: Old refresh token should now be revoked ---
    let resp = server
        .post("/oidc/token")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!("grant_type=refresh_token&refresh_token={refresh_token}&client_id=voxora-web")
                .into(),
        )
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    // --- Step 6: Userinfo works with the new access token ---
    let resp = server
        .get("/oidc/userinfo")
        .authorization_bearer(new_at)
        .await;

    resp.assert_status_ok();

    // --- Step 7: Revoke the new access token ---
    let resp = server
        .post("/oidc/revoke")
        .content_type("application/x-www-form-urlencoded")
        .bytes(format!("token={new_at}&token_type_hint=access_token").into())
        .await;

    resp.assert_status_ok();

    // --- Step 8: Userinfo should now fail ---
    let resp = server
        .get("/oidc/userinfo")
        .authorization_bearer(new_at)
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    // --- Step 9: Revoke the new refresh token ---
    let resp = server
        .post("/oidc/revoke")
        .content_type("application/x-www-form-urlencoded")
        .bytes(format!("token={new_rt}&token_type_hint=refresh_token").into())
        .await;

    resp.assert_status_ok();

    // Using the revoked refresh token should fail
    let resp = server
        .post("/oidc/token")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!("grant_type=refresh_token&refresh_token={new_rt}&client_id=voxora-web").into(),
        )
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);

    // Cleanup
    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// Auth code is single-use
// =========================================================================

#[tokio::test]
async fn auth_code_is_single_use() {
    let (app, state) = common::test_app().await;
    let password = "single_use_test_pw";
    let user = common::create_test_user(&state.db, password).await;
    let server = TestServer::new(app).unwrap();

    let code_verifier = "single-use-verifier-at-least-43-chars!!!!!!";
    let code_challenge = pkce_challenge(code_verifier);
    let redirect_uri = "http://localhost:5173/callback";

    // Get auth code
    let resp = server
        .post("/oidc/authorize")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "response_type=code&client_id=voxora-web&redirect_uri={redirect_uri}\
             &scope=openid&code_challenge={code_challenge}&code_challenge_method=S256\
             &login={}&password={password}",
                user.username
            )
            .into(),
        )
        .await;

    let location = resp.header("location").to_str().unwrap().to_string();
    let code = location
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();

    // First exchange — should succeed
    let resp = server
        .post("/oidc/token")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "grant_type=authorization_code&code={code}&redirect_uri={redirect_uri}\
             &code_verifier={code_verifier}&client_id=voxora-web"
            )
            .into(),
        )
        .await;
    resp.assert_status_ok();

    // Second exchange — same code must fail
    let resp = server
        .post("/oidc/token")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "grant_type=authorization_code&code={code}&redirect_uri={redirect_uri}\
             &code_verifier={code_verifier}&client_id=voxora-web"
            )
            .into(),
        )
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// PKCE verification
// =========================================================================

#[tokio::test]
async fn wrong_code_verifier_fails_pkce() {
    let (app, state) = common::test_app().await;
    let password = "pkce_test_password";
    let user = common::create_test_user(&state.db, password).await;
    let server = TestServer::new(app).unwrap();

    let real_verifier = "real-verifier-with-enough-chars-for-test!!";
    let code_challenge = pkce_challenge(real_verifier);
    let redirect_uri = "http://localhost:5173/callback";

    let resp = server
        .post("/oidc/authorize")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "response_type=code&client_id=voxora-web&redirect_uri={redirect_uri}\
             &scope=openid&code_challenge={code_challenge}&code_challenge_method=S256\
             &login={}&password={password}",
                user.username
            )
            .into(),
        )
        .await;

    let location = resp.header("location").to_str().unwrap().to_string();
    let code = location
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();

    // Use a wrong verifier
    let resp = server
        .post("/oidc/token")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "grant_type=authorization_code&code={code}&redirect_uri={redirect_uri}\
             &code_verifier=wrong-verifier-that-does-not-match!!!&client_id=voxora-web"
            )
            .into(),
        )
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert!(
        body["error"]["message"].as_str().unwrap().contains("PKCE"),
        "error message should mention PKCE"
    );

    common::cleanup_test_user(&state.db, &user.id).await;
}

// =========================================================================
// Token endpoint — parameter validation
// =========================================================================

#[tokio::test]
async fn token_rejects_unsupported_grant_type() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/oidc/token")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            "grant_type=client_credentials&client_id=voxora-web"
                .to_string()
                .into(),
        )
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("grant_type"));
}

#[tokio::test]
async fn token_rejects_missing_code() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/oidc/token")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            "grant_type=authorization_code&redirect_uri=http://x&code_verifier=abc&client_id=voxora-web"
                .to_string()
                .into(),
        )
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);
}

// =========================================================================
// Userinfo — auth checks
// =========================================================================

#[tokio::test]
async fn userinfo_requires_bearer_token() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/oidc/userinfo").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn userinfo_rejects_invalid_token() {
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/oidc/userinfo")
        .authorization_bearer("hat_invalid_token_that_does_not_exist")
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Revoke — edge cases
// =========================================================================

#[tokio::test]
async fn revoke_succeeds_for_unknown_token() {
    // Per RFC 7009, revoke should always return 200
    let (app, _) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/oidc/revoke")
        .content_type("application/x-www-form-urlencoded")
        .bytes("token=hat_nonexistent_token_12345".to_string().into())
        .await;

    resp.assert_status_ok();
}

#[tokio::test]
async fn login_with_email_works() {
    let (app, state) = common::test_app().await;
    let password = "email_login_test_pw";
    let user = common::create_test_user(&state.db, password).await;
    let server = TestServer::new(app).unwrap();

    let code_verifier = "email-login-verifier-that-is-long-enough-43";
    let code_challenge = pkce_challenge(code_verifier);
    let redirect_uri = "http://localhost:5173/callback";

    // Login with email instead of username
    let resp = server
        .post("/oidc/authorize")
        .content_type("application/x-www-form-urlencoded")
        .bytes(
            format!(
                "response_type=code&client_id=voxora-web&redirect_uri={redirect_uri}\
             &scope=openid+profile+email&code_challenge={code_challenge}\
             &code_challenge_method=S256&login={}&password={password}",
                user.email
            )
            .into(),
        )
        .await;

    resp.assert_status(StatusCode::SEE_OTHER);
    let location_hdr = resp.header("location");
    let location = location_hdr.to_str().unwrap();
    assert!(
        location.contains("code=hac_"),
        "should get auth code when logging in by email"
    );

    common::cleanup_test_user(&state.db, &user.id).await;
}
