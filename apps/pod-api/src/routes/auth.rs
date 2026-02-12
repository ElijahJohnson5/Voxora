//! Auth routes: SIA login and token refresh.

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::{sia, tokens};
use crate::error::ApiError;
use crate::models::pod_user;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/refresh", post(refresh))
}

// ---------------------------------------------------------------------------
// POST /api/v1/auth/login
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub sia: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: String,
    pub ws_ticket: String,
    pub ws_url: String,
    pub user: UserInfo,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    // Validate the SIA JWT.
    let claims = sia::validate_sia(
        &body.sia,
        &state.jwks,
        state.kv.as_ref(),
        &state.config.pod_id,
        &state.config.hub_url,
    )
    .await?;

    // Convert flags vec to bitfield for storage.
    let hub_flags = flags_to_bitfield(&claims.flags);

    // Upsert the local pod_user record.
    let user = pod_user::upsert_from_sia(
        &state.db,
        &claims.sub,
        &claims.username,
        &claims.display_name,
        claims.avatar_url.as_deref(),
        hub_flags,
    )
    .await?;

    // Generate tokens.
    let pat = tokens::generate_pat();
    let refresh = tokens::generate_refresh_token();
    let ws_ticket = tokens::generate_ws_ticket();

    let kv = state.kv.as_ref();

    tokens::store_pat(kv, &pat, &tokens::PatData { user_id: user.id.clone() }).await?;
    tokens::store_refresh_token(
        kv,
        &refresh,
        &tokens::RefreshData { user_id: user.id.clone() },
    )
    .await?;
    tokens::store_ws_ticket(
        kv,
        &ws_ticket,
        &tokens::WsTicketData { user_id: user.id.clone() },
    )
    .await?;

    let ws_url = format!("ws://localhost:{}/gateway", state.config.port);

    Ok(Json(LoginResponse {
        access_token: pat,
        token_type: "Bearer".to_string(),
        expires_in: tokens::PAT_TTL_SECS,
        refresh_token: refresh,
        ws_ticket,
        ws_url,
        user: UserInfo {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            avatar_url: user.avatar_url,
        },
    }))
}

/// Convert flag name strings to a bitfield.
fn flags_to_bitfield(flags: &[String]) -> i64 {
    let mut bits: i64 = 0;
    for flag in flags {
        match flag.as_str() {
            "staff" => bits |= 1,
            "verified" => bits |= 2,
            _ => {}
        }
    }
    bits
}

// ---------------------------------------------------------------------------
// POST /api/v1/auth/refresh
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: String,
}

async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, ApiError> {
    let kv = state.kv.as_ref();

    // Consume the old refresh token (single-use rotation).
    let data = tokens::consume_refresh_token(kv, &body.refresh_token)
        .await?
        .ok_or_else(|| ApiError::unauthorized("Invalid or expired refresh token"))?;

    // Issue new PAT + refresh token.
    let new_pat = tokens::generate_pat();
    let new_refresh = tokens::generate_refresh_token();

    tokens::store_pat(kv, &new_pat, &tokens::PatData { user_id: data.user_id.clone() }).await?;
    tokens::store_refresh_token(
        kv,
        &new_refresh,
        &tokens::RefreshData { user_id: data.user_id },
    )
    .await?;

    Ok(Json(RefreshResponse {
        access_token: new_pat,
        token_type: "Bearer".to_string(),
        expires_in: tokens::PAT_TTL_SECS,
        refresh_token: new_refresh,
    }))
}
