use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha1::Sha1;
use utoipa::ToSchema;

use crate::auth::pod::PodClient;
use crate::error::{ApiError, ApiErrorBody};
use crate::AppState;

/// TTL for TURN credentials in seconds (12 hours).
const TURN_TTL: u64 = 43200;

pub fn router() -> Router<AppState> {
    Router::new().route("/turn/credentials", post(turn_credentials))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TurnCredentialsResponse {
    pub ice_servers: Vec<IceServer>,
}

/// `POST /api/v1/turn/credentials` — Generate time-limited TURN credentials.
///
/// Authenticated via a Pod's `client_secret` as Bearer token. Returns ICE server
/// configuration with coturn REST API credentials (HMAC-SHA1).
#[utoipa::path(
    post,
    path = "/api/v1/turn/credentials",
    tag = "TURN",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "ICE server credentials", body = TurnCredentialsResponse),
        (status = 401, description = "Invalid credentials", body = ApiErrorBody),
    ),
)]
pub async fn turn_credentials(
    State(state): State<AppState>,
    pod_client: PodClient,
) -> Result<Json<TurnCredentialsResponse>, ApiError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs();
    let expiry = now + TURN_TTL;
    let username = format!("{}:{}", expiry, pod_client.pod.id);

    let mut mac = Hmac::<Sha1>::new_from_slice(state.config.turn_shared_secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(username.as_bytes());
    let credential = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let ice_servers = vec![
        IceServer {
            urls: state.config.stun_urls.clone(),
            username: None,
            credential: None,
            ttl: None,
        },
        IceServer {
            urls: state.config.turn_urls.clone(),
            username: Some(username),
            credential: Some(credential),
            ttl: Some(TURN_TTL),
        },
    ];

    tracing::debug!(pod_id = %pod_client.pod.id, "TURN credentials issued");

    Ok(Json(TurnCredentialsResponse { ice_servers }))
}
