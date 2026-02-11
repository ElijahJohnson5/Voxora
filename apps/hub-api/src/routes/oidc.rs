use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use chrono::{Duration, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth::tokens::{
    self, generate_access_token, generate_opaque_token, generate_refresh_token, mint_id_token,
    AccessTokenData, AuthCodeData, ACCESS_TOKEN_TTL_SECS, REFRESH_TOKEN_TTL_DAYS,
};
use crate::db::schema::{sessions, users};
use crate::error::ApiError;
use crate::models::session::NewSession;
use crate::models::user::User;
use crate::AppState;

/// Allowed client_id for Phase 1.
const CLIENT_ID: &str = "voxora-web";

pub fn router() -> Router<AppState> {
    Router::new()
        // Discovery
        .route(
            "/.well-known/openid-configuration",
            get(openid_configuration),
        )
        // JWKS
        .route("/oidc/.well-known/jwks.json", get(jwks))
        // Authorization + login form submission
        .route("/oidc/authorize", get(authorize).post(authorize_submit))
        // Token endpoint
        .route("/oidc/token", post(token))
        // UserInfo
        .route("/oidc/userinfo", get(userinfo))
        // Revocation
        .route("/oidc/revoke", post(revoke))
}

// ===========================================================================
// GET /.well-known/openid-configuration
// ===========================================================================

async fn openid_configuration(State(state): State<AppState>) -> Json<serde_json::Value> {
    let hub = &state.config.hub_domain;
    Json(serde_json::json!({
        "issuer": hub,
        "authorization_endpoint": format!("{hub}/oidc/authorize"),
        "token_endpoint": format!("{hub}/oidc/token"),
        "userinfo_endpoint": format!("{hub}/oidc/userinfo"),
        "jwks_uri": format!("{hub}/oidc/.well-known/jwks.json"),
        "revocation_endpoint": format!("{hub}/oidc/revoke"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["EdDSA"],
        "scopes_supported": ["openid", "profile", "email", "pods", "offline_access"],
        "token_endpoint_auth_methods_supported": ["none"],
        "code_challenge_methods_supported": ["S256"]
    }))
}

// ===========================================================================
// GET /oidc/.well-known/jwks.json
// ===========================================================================

async fn jwks(State(state): State<AppState>) -> Json<serde_json::Value> {
    let keys = &state.keys;
    Json(serde_json::json!({
        "keys": [{
            "kty": "OKP",
            "crv": "Ed25519",
            "kid": keys.kid,
            "use": "sig",
            "x": keys.public_key_b64
        }]
    }))
}

// ===========================================================================
// GET /oidc/authorize  (renders a minimal login form for Phase 1)
// POST /oidc/authorize (processes the login form)
// ===========================================================================

#[derive(Debug, Deserialize)]
pub struct AuthorizeParams {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub nonce: Option<String>,
}

/// Render a minimal HTML login form.
/// The SPA would typically collect credentials and POST here.
async fn authorize(Query(params): Query<AuthorizeParams>) -> Response {
    // Validate basics
    if params.response_type != "code" {
        return (StatusCode::BAD_REQUEST, "unsupported response_type").into_response();
    }
    if params.client_id != CLIENT_ID {
        return (StatusCode::BAD_REQUEST, "unknown client_id").into_response();
    }
    if params.code_challenge.is_none() || params.code_challenge_method.as_deref() != Some("S256") {
        return (StatusCode::BAD_REQUEST, "PKCE with S256 is required").into_response();
    }

    // Render a simple login form that POSTs back to the same path with the OIDC params embedded.
    let html = format!(
        r#"<!DOCTYPE html>
<html><head><title>Voxora Login</title></head>
<body style="font-family:system-ui;max-width:400px;margin:80px auto">
<h2>Sign in to Voxora</h2>
<form method="POST" action="/oidc/authorize">
  <input type="hidden" name="response_type" value="{}" />
  <input type="hidden" name="client_id" value="{}" />
  <input type="hidden" name="redirect_uri" value="{}" />
  <input type="hidden" name="scope" value="{}" />
  <input type="hidden" name="state" value="{}" />
  <input type="hidden" name="code_challenge" value="{}" />
  <input type="hidden" name="code_challenge_method" value="S256" />
  <input type="hidden" name="nonce" value="{}" />
  <label>Username or email<br/><input name="login" required style="width:100%;padding:8px;margin:4px 0 12px" /></label>
  <label>Password<br/><input name="password" type="password" required style="width:100%;padding:8px;margin:4px 0 12px" /></label>
  <button type="submit" style="width:100%;padding:10px;cursor:pointer">Log in</button>
</form></body></html>"#,
        params.response_type,
        params.client_id,
        params.redirect_uri,
        params.scope.as_deref().unwrap_or("openid"),
        params.state.as_deref().unwrap_or(""),
        params.code_challenge.as_deref().unwrap_or(""),
        params.nonce.as_deref().unwrap_or("")
    );
    Html(html).into_response()
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeSubmit {
    // OIDC params echoed back
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub nonce: Option<String>,
    // User credentials
    pub login: String,
    pub password: String,
}

/// Process login form — validate credentials, generate auth code, redirect.
async fn authorize_submit(
    State(state): State<AppState>,
    Form(form): Form<AuthorizeSubmit>,
) -> Result<Response, ApiError> {
    if form.client_id != CLIENT_ID {
        return Err(ApiError::bad_request("unknown client_id"));
    }
    let code_challenge = form
        .code_challenge
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("code_challenge is required"))?;

    // Look up user by username (case-insensitive) or email.
    let login_lower = form.login.trim().to_lowercase();
    let mut conn = state.db.get().await?;

    let user: User = users::table
        .filter(
            users::username_lower
                .eq(&login_lower)
                .or(users::email.eq(&login_lower)),
        )
        .select(User::as_select())
        .first(&mut conn)
        .await
        .optional()
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::unauthorized("Invalid credentials"))?;

    // Verify password.
    let hash = user
        .password_hash
        .as_deref()
        .ok_or_else(|| ApiError::unauthorized("Invalid credentials"))?;
    verify_password(&form.password, hash)?;

    // Generate authorization code.
    let code = generate_opaque_token("hac", 32);

    let scopes: Vec<String> = form
        .scope
        .as_deref()
        .unwrap_or("openid")
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let code_data = AuthCodeData {
        user_id: user.id.clone(),
        client_id: form.client_id.clone(),
        redirect_uri: form.redirect_uri.clone(),
        code_challenge: code_challenge.to_string(),
        scopes,
        nonce: form.nonce.clone(),
    };

    tokens::store_auth_code(&mut state.redis.clone(), &code, &code_data).await?;

    // Build redirect URI with code + state.
    let sep = if form.redirect_uri.contains('?') {
        "&"
    } else {
        "?"
    };
    let mut redirect = format!("{}{}code={}", form.redirect_uri, sep, code);
    if let Some(ref st) = form.state {
        redirect.push_str(&format!("&state={}", st));
    }

    Ok(Redirect::to(&redirect).into_response())
}

// ===========================================================================
// POST /oidc/token
// ===========================================================================

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub code_verifier: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    pub scope: String,
}

async fn token(
    State(state): State<AppState>,
    Form(form): Form<TokenRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    match form.grant_type.as_str() {
        "authorization_code" => handle_authorization_code(state, form).await,
        "refresh_token" => handle_refresh_token(state, form).await,
        _ => Err(ApiError::bad_request("unsupported grant_type")),
    }
}

async fn handle_authorization_code(
    state: AppState,
    form: TokenRequest,
) -> Result<Json<TokenResponse>, ApiError> {
    let code = form
        .code
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("code is required"))?;
    let code_verifier = form
        .code_verifier
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("code_verifier is required"))?;
    let redirect_uri = form
        .redirect_uri
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("redirect_uri is required"))?;

    // Consume the auth code from Redis.
    let code_data = tokens::consume_auth_code(&mut state.redis.clone(), code)
        .await?
        .ok_or_else(|| ApiError::bad_request("invalid or expired code"))?;

    // PKCE verification: SHA256(code_verifier) == code_challenge
    verify_pkce(code_verifier, &code_data.code_challenge)?;

    // Validate redirect_uri matches what was stored.
    if redirect_uri != code_data.redirect_uri {
        return Err(ApiError::bad_request("redirect_uri mismatch"));
    }

    // Load user from DB.
    let mut conn = state.db.get().await?;
    let user: User = users::table
        .find(&code_data.user_id)
        .select(User::as_select())
        .first(&mut conn)
        .await
        .map_err(ApiError::from)?;

    // Generate tokens.
    let access_token = generate_access_token();
    let refresh_token = generate_refresh_token();

    // Store access token in Redis.
    let at_data = AccessTokenData {
        user_id: user.id.clone(),
        scopes: code_data.scopes.clone(),
    };
    tokens::store_access_token(&mut state.redis.clone(), &access_token, &at_data).await?;

    // Store refresh token in sessions table.
    let session = NewSession {
        id: voxora_common::id::prefixed_ulid(voxora_common::id::prefix::SESSION),
        user_id: user.id.clone(),
        refresh_token: refresh_token.clone(),
        user_agent: None,
        expires_at: Utc::now() + Duration::days(REFRESH_TOKEN_TTL_DAYS),
    };
    diesel::insert_into(sessions::table)
        .values(&session)
        .execute(&mut conn)
        .await
        .map_err(ApiError::from)?;

    // Mint ID token.
    let id_token = mint_id_token(
        &state.keys,
        &state.config.hub_domain,
        &code_data.client_id,
        &user.id,
        code_data.nonce.as_deref(),
        &code_data.scopes,
        &user.username,
        &user.display_name,
        user.avatar_url.as_deref(),
        user.email.as_deref(),
        user.email_verified,
    )?;

    let scope = code_data.scopes.join(" ");

    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECS,
        refresh_token: Some(refresh_token),
        id_token: Some(id_token),
        scope,
    }))
}

async fn handle_refresh_token(
    state: AppState,
    form: TokenRequest,
) -> Result<Json<TokenResponse>, ApiError> {
    let old_rt = form
        .refresh_token
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("refresh_token is required"))?;

    let mut conn = state.db.get().await?;

    // Look up session by refresh token.
    let session: crate::models::session::Session = sessions::table
        .filter(sessions::refresh_token.eq(old_rt))
        .filter(sessions::revoked.eq(false))
        .filter(sessions::expires_at.gt(Utc::now()))
        .select(crate::models::session::Session::as_select())
        .first(&mut conn)
        .await
        .optional()
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::unauthorized("invalid or expired refresh_token"))?;

    // Rotate: revoke old, create new session.
    diesel::update(sessions::table.find(&session.id))
        .set(sessions::revoked.eq(true))
        .execute(&mut conn)
        .await
        .map_err(ApiError::from)?;

    let new_rt = generate_refresh_token();
    let new_session = NewSession {
        id: voxora_common::id::prefixed_ulid(voxora_common::id::prefix::SESSION),
        user_id: session.user_id.clone(),
        refresh_token: new_rt.clone(),
        user_agent: None,
        expires_at: Utc::now() + Duration::days(REFRESH_TOKEN_TTL_DAYS),
    };
    diesel::insert_into(sessions::table)
        .values(&new_session)
        .execute(&mut conn)
        .await
        .map_err(ApiError::from)?;

    // Generate new access token.
    let access_token = generate_access_token();

    // Load user for scopes (reuse same scopes — for Phase 1 we default to full set).
    let user: User = users::table
        .find(&session.user_id)
        .select(User::as_select())
        .first(&mut conn)
        .await
        .map_err(ApiError::from)?;

    let scopes = vec![
        "openid".to_string(),
        "profile".to_string(),
        "email".to_string(),
        "pods".to_string(),
    ];

    let at_data = AccessTokenData {
        user_id: user.id.clone(),
        scopes: scopes.clone(),
    };
    tokens::store_access_token(&mut state.redis.clone(), &access_token, &at_data).await?;

    let scope = scopes.join(" ");

    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECS,
        refresh_token: Some(new_rt),
        id_token: None, // Not issued on refresh
        scope,
    }))
}

// ===========================================================================
// GET /oidc/userinfo
// ===========================================================================

async fn userinfo(
    State(state): State<AppState>,
    auth: crate::auth::middleware::AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut conn = state.db.get().await?;
    let user: User = users::table
        .find(&auth.user_id)
        .select(User::as_select())
        .first(&mut conn)
        .await
        .map_err(ApiError::from)?;

    let mut claims = serde_json::json!({ "sub": user.id });

    if auth.scopes.iter().any(|s| s == "profile") {
        claims["preferred_username"] = serde_json::json!(user.username);
        claims["name"] = serde_json::json!(user.display_name);
        if let Some(ref url) = user.avatar_url {
            claims["picture"] = serde_json::json!(url);
        }
    }

    if auth.scopes.iter().any(|s| s == "email") {
        if let Some(ref email) = user.email {
            claims["email"] = serde_json::json!(email);
        }
        claims["email_verified"] = serde_json::json!(user.email_verified);
    }

    Ok(Json(claims))
}

// ===========================================================================
// POST /oidc/revoke
// ===========================================================================

#[derive(Debug, Deserialize)]
pub struct RevokeRequest {
    pub token: String,
    #[serde(default)]
    pub token_type_hint: Option<String>,
}

async fn revoke(
    State(state): State<AppState>,
    Form(form): Form<RevokeRequest>,
) -> Result<StatusCode, ApiError> {
    let hint = form.token_type_hint.as_deref().unwrap_or("access_token");

    match hint {
        "access_token" => {
            tokens::delete_access_token(&mut state.redis.clone(), &form.token).await?;
        }
        "refresh_token" => {
            let mut conn = state.db.get().await?;
            diesel::update(sessions::table.filter(sessions::refresh_token.eq(&form.token)))
                .set(sessions::revoked.eq(true))
                .execute(&mut conn)
                .await
                .map_err(ApiError::from)?;
        }
        _ => {
            // Try both.
            tokens::delete_access_token(&mut state.redis.clone(), &form.token).await?;
            let mut conn = state.db.get().await?;
            diesel::update(sessions::table.filter(sessions::refresh_token.eq(&form.token)))
                .set(sessions::revoked.eq(true))
                .execute(&mut conn)
                .await
                .map_err(ApiError::from)?;
        }
    }

    // Per RFC 7009, always return 200 even if token was invalid.
    Ok(StatusCode::OK)
}

// ===========================================================================
// Helpers
// ===========================================================================

/// Verify a PKCE code_verifier against the stored code_challenge (S256 method).
fn verify_pkce(code_verifier: &str, code_challenge: &str) -> Result<(), ApiError> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let hash = Sha256::digest(code_verifier.as_bytes());
    let computed = URL_SAFE_NO_PAD.encode(hash);

    if computed != code_challenge {
        return Err(ApiError::bad_request("PKCE verification failed"));
    }
    Ok(())
}

/// Verify a password against an Argon2id hash.
fn verify_password(password: &str, hash: &str) -> Result<(), ApiError> {
    use argon2::Argon2;
    use password_hash::{PasswordHash, PasswordVerifier};

    let parsed = PasswordHash::new(hash).map_err(|_| ApiError::internal("invalid hash format"))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| ApiError::unauthorized("Invalid credentials"))
}
