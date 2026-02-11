use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, Header};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::auth::keys::SigningKeys;
use crate::error::ApiError;

// ---------------------------------------------------------------------------
// Opaque token helpers
// ---------------------------------------------------------------------------

/// Generate an opaque random token with the given prefix and byte length.
pub fn generate_opaque_token(prefix: &str, bytes: usize) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill(&mut buf[..]);
    format!("{}_{}", prefix, URL_SAFE_NO_PAD.encode(&buf))
}

/// Generate a Hub Access Token (opaque, `hat_` prefix).  15-minute TTL.
pub fn generate_access_token() -> String {
    generate_opaque_token("hat", 32)
}

/// Generate a Hub Refresh Token (opaque, `hrt_` prefix).  30-day sliding TTL.
pub fn generate_refresh_token() -> String {
    generate_opaque_token("hrt", 32)
}

/// Access-token TTL in seconds.
pub const ACCESS_TOKEN_TTL_SECS: i64 = 900; // 15 minutes

/// Refresh-token TTL in days.
pub const REFRESH_TOKEN_TTL_DAYS: i64 = 30;

// ---------------------------------------------------------------------------
// ID token (JWT signed with EdDSA)
// ---------------------------------------------------------------------------

/// Claims embedded in the ID token JWT.
#[derive(Debug, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// Issuer — the Hub domain.
    pub iss: String,
    /// Subject — the user's prefixed ULID.
    pub sub: String,
    /// Audience — the client_id.
    pub aud: String,
    /// Expiration (unix timestamp).
    pub exp: i64,
    /// Issued-at (unix timestamp).
    pub iat: i64,
    /// Nonce echoed back from the authorization request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,

    // Profile claims (included when `profile` scope is requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,

    // Email claims (included when `email` scope is requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_verified: Option<bool>,
}

/// Mint a signed ID token JWT.
pub fn mint_id_token(
    keys: &SigningKeys,
    issuer: &str,
    client_id: &str,
    user_id: &str,
    nonce: Option<&str>,
    scopes: &[String],
    // User profile fields.
    username: &str,
    display_name: &str,
    avatar_url: Option<&str>,
    email: Option<&str>,
    email_verified: bool,
) -> Result<String, ApiError> {
    let now = Utc::now();

    let mut claims = IdTokenClaims {
        iss: issuer.to_string(),
        sub: user_id.to_string(),
        aud: client_id.to_string(),
        exp: (now + Duration::seconds(ACCESS_TOKEN_TTL_SECS)).timestamp(),
        iat: now.timestamp(),
        nonce: nonce.map(|n| n.to_string()),
        preferred_username: None,
        name: None,
        picture: None,
        email: None,
        email_verified: None,
    };

    if scopes.iter().any(|s| s == "profile") {
        claims.preferred_username = Some(username.to_string());
        claims.name = Some(display_name.to_string());
        claims.picture = avatar_url.map(|u| u.to_string());
    }

    if scopes.iter().any(|s| s == "email") {
        claims.email = email.map(|e| e.to_string());
        claims.email_verified = Some(email_verified);
    }

    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(keys.kid.clone());

    jsonwebtoken::encode(&header, &claims, &keys.encoding).map_err(|e| {
        tracing::error!(?e, "failed to sign ID token");
        ApiError::internal("Token signing failed")
    })
}

// ---------------------------------------------------------------------------
// Redis helpers for access tokens and auth codes
// ---------------------------------------------------------------------------

/// Stored alongside an opaque access token in Redis.
#[derive(Debug, Serialize, Deserialize)]
pub struct AccessTokenData {
    pub user_id: String,
    pub scopes: Vec<String>,
}

/// Stored alongside an authorization code in Redis.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthCodeData {
    pub user_id: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub scopes: Vec<String>,
    pub nonce: Option<String>,
}

/// Authorization code TTL in seconds.
pub const AUTH_CODE_TTL_SECS: u64 = 60;

/// Store an access token in Redis.
pub async fn store_access_token(
    redis: &mut redis::aio::ConnectionManager,
    token: &str,
    data: &AccessTokenData,
) -> Result<(), ApiError> {
    use redis::AsyncCommands;
    let key = format!("hub:at:{}", token);
    let value = serde_json::to_string(data).map_err(|_| ApiError::internal("serialization"))?;
    redis
        .set_ex::<_, _, ()>(&key, &value, ACCESS_TOKEN_TTL_SECS as u64)
        .await
        .map_err(|e| {
            tracing::error!(?e, "redis set failed");
            ApiError::internal("Failed to store token")
        })
}

/// Look up an access token in Redis.
pub async fn lookup_access_token(
    redis: &mut redis::aio::ConnectionManager,
    token: &str,
) -> Result<Option<AccessTokenData>, ApiError> {
    use redis::AsyncCommands;
    let key = format!("hub:at:{}", token);
    let val: Option<String> = redis.get(&key).await.map_err(|e| {
        tracing::error!(?e, "redis get failed");
        ApiError::internal("Token lookup failed")
    })?;
    match val {
        Some(v) => {
            let data: AccessTokenData =
                serde_json::from_str(&v).map_err(|_| ApiError::internal("corrupt token data"))?;
            Ok(Some(data))
        }
        None => Ok(None),
    }
}

/// Delete an access token from Redis.
pub async fn delete_access_token(
    redis: &mut redis::aio::ConnectionManager,
    token: &str,
) -> Result<(), ApiError> {
    use redis::AsyncCommands;
    let key = format!("hub:at:{}", token);
    redis.del::<_, ()>(&key).await.map_err(|e| {
        tracing::error!(?e, "redis del failed");
        ApiError::internal("Token revocation failed")
    })
}

/// Store an authorization code in Redis with 60s TTL.
pub async fn store_auth_code(
    redis: &mut redis::aio::ConnectionManager,
    code: &str,
    data: &AuthCodeData,
) -> Result<(), ApiError> {
    use redis::AsyncCommands;
    let key = format!("hub:code:{}", code);
    let value = serde_json::to_string(data).map_err(|_| ApiError::internal("serialization"))?;
    redis
        .set_ex::<_, _, ()>(&key, &value, AUTH_CODE_TTL_SECS)
        .await
        .map_err(|e| {
            tracing::error!(?e, "redis set failed");
            ApiError::internal("Failed to store auth code")
        })
}

/// Consume an authorization code from Redis (single-use).
pub async fn consume_auth_code(
    redis: &mut redis::aio::ConnectionManager,
    code: &str,
) -> Result<Option<AuthCodeData>, ApiError> {
    use redis::AsyncCommands;
    let key = format!("hub:code:{}", code);
    let val: Option<String> = redis.get(&key).await.map_err(|e| {
        tracing::error!(?e, "redis get failed");
        ApiError::internal("Code lookup failed")
    })?;
    if val.is_some() {
        // Delete immediately — single use.
        redis.del::<_, ()>(&key).await.ok();
    }
    match val {
        Some(v) => {
            let data: AuthCodeData =
                serde_json::from_str(&v).map_err(|_| ApiError::internal("corrupt code data"))?;
            Ok(Some(data))
        }
        None => Ok(None),
    }
}
