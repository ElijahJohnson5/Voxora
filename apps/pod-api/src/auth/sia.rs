//! SIA (Signed Identity Assertion) JWT validation on the Pod side.

use jsonwebtoken::{Algorithm, Validation};
use serde::{Deserialize, Serialize};

use crate::auth::jwks::JwksClient;
use crate::db::kv::KeyValueStore;
use crate::error::ApiError;

/// SIA claims carried from the Hub.
#[derive(Debug, Serialize, Deserialize)]
pub struct SiaClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
    pub jti: String,
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub hub_version: u32,
}

/// JTI replay-prevention TTL in seconds (5 minutes, matching SIA lifetime).
const JTI_TTL_SECS: u64 = 300;

/// Validate a SIA JWT and return its claims.
///
/// Checks:
///   1. Signature via Hub JWKS
///   2. `exp` (jsonwebtoken handles this)
///   3. `aud` matches this Pod's ID
///   4. `iss` matches the configured Hub URL
///   5. `jti` not already seen (replay prevention)
pub async fn validate_sia(
    token: &str,
    jwks: &JwksClient,
    kv: &dyn KeyValueStore,
    expected_pod_id: &str,
    expected_issuer: &str,
) -> Result<SiaClaims, ApiError> {
    // Decode the header to find `kid`.
    let header = jsonwebtoken::decode_header(token).map_err(|e| {
        tracing::debug!(?e, "SIA header decode failed");
        ApiError::unauthorized("Invalid SIA token")
    })?;

    let kid = header
        .kid
        .ok_or_else(|| ApiError::unauthorized("SIA token missing kid"))?;

    // Fetch the decoding key.
    let key = jwks.get_key(&kid).await?;

    // Build validation: require EdDSA, validate exp, set expected aud.
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_audience(&[expected_pod_id]);
    validation.set_issuer(&[expected_issuer]);

    let token_data = jsonwebtoken::decode::<SiaClaims>(token, &key, &validation).map_err(|e| {
        tracing::debug!(?e, "SIA validation failed");
        ApiError::unauthorized("Invalid or expired SIA token")
    })?;

    let claims = token_data.claims;

    // Replay prevention: reject if jti was already used.
    let jti_key = format!("pod:sia_jti:{}", claims.jti);
    if kv.get(&jti_key).await?.is_some() {
        return Err(ApiError::unauthorized("SIA token already used"));
    }
    // Mark jti as seen.
    kv.set_ex(&jti_key, "1", JTI_TTL_SECS).await?;

    Ok(claims)
}
