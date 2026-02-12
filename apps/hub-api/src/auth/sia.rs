use chrono::Utc;
use jsonwebtoken::{Algorithm, Header};
use serde::{Deserialize, Serialize};

use crate::auth::keys::SigningKeys;
use crate::error::ApiError;

/// SIA (Signed Identity Assertion) lifetime in seconds (5 minutes).
pub const SIA_TTL_SECS: i64 = 300;

/// Claims embedded in a SIA JWT.
#[derive(Debug, Serialize, Deserialize)]
pub struct SiaClaims {
    /// Issuer — the Hub domain.
    pub iss: String,
    /// Subject — the user's prefixed ULID.
    pub sub: String,
    /// Audience — the target Pod ID.
    pub aud: String,
    /// Issued-at (unix timestamp).
    pub iat: i64,
    /// Expiration (unix timestamp).
    pub exp: i64,
    /// Unique token identifier (`sia_` prefixed ULID).
    pub jti: String,

    // Identity claims carried to the Pod.
    pub username: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub email_verified: bool,
    pub flags: Vec<String>,
    pub hub_version: u32,
}

/// Mint a signed SIA JWT for a user targeting a specific Pod.
pub fn mint_sia(
    keys: &SigningKeys,
    issuer: &str,
    user_id: &str,
    pod_id: &str,
    username: &str,
    display_name: &str,
    avatar_url: Option<&str>,
    email: Option<&str>,
    email_verified: bool,
    flags: i64,
) -> Result<(String, chrono::DateTime<Utc>), ApiError> {
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(SIA_TTL_SECS);

    let jti = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::SIA);

    // Convert integer flags to string list. For Phase 1, flags are unused
    // so this will be empty, but the structure supports future flag names.
    let flag_names = flags_to_names(flags);

    let claims = SiaClaims {
        iss: issuer.to_string(),
        sub: user_id.to_string(),
        aud: pod_id.to_string(),
        iat: now.timestamp(),
        exp: expires_at.timestamp(),
        jti,
        username: username.to_string(),
        display_name: display_name.to_string(),
        avatar_url: avatar_url.map(|u| u.to_string()),
        email: email.map(|e| e.to_string()),
        email_verified,
        flags: flag_names,
        hub_version: 1,
    };

    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(keys.kid.clone());
    header.typ = Some("voxora-sia+jwt".to_string());

    let token = jsonwebtoken::encode(&header, &claims, &keys.encoding).map_err(|e| {
        tracing::error!(?e, "failed to sign SIA");
        ApiError::internal("SIA signing failed")
    })?;

    Ok((token, expires_at))
}

/// Convert the integer flags bitfield to a list of flag names.
/// For Phase 1 no flags are defined; this returns an empty vec for flags == 0.
fn flags_to_names(flags: i64) -> Vec<String> {
    let mut names = Vec::new();
    if flags & 1 != 0 {
        names.push("staff".to_string());
    }
    if flags & 2 != 0 {
        names.push("verified".to_string());
    }
    names
}
