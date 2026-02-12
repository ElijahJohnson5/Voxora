use std::sync::Arc;

use axum::Router;
use ed25519_dalek::{SigningKey, VerifyingKey};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use pod_api::auth::jwks::JwksClient;
use pod_api::config::Config;
use pod_api::db::kv::{KeyValueStore, MemoryStore};
use pod_api::AppState;
use voxora_common::SnowflakeGenerator;

/// Test signing keys (mirrors hub-api's `SigningKeys` derivation from a seed).
pub struct TestSigningKeys {
    pub kid: String,
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
}

impl TestSigningKeys {
    /// Derive keys from the same seed as the hub-api dev environment.
    pub fn from_seed(seed: &str) -> Self {
        let hash = Sha256::digest(seed.as_bytes());
        let mut secret_bytes = [0u8; 32];
        secret_bytes.copy_from_slice(&hash);

        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key: VerifyingKey = (&signing_key).into();

        let secret = signing_key.to_bytes();
        let public_bytes = verifying_key.to_bytes();

        let pkcs8_der = wrap_ed25519_private_pkcs8(&secret);
        let encoding = EncodingKey::from_ed_der(&pkcs8_der);
        let decoding = DecodingKey::from_ed_der(&public_bytes);

        let kid_hash = Sha256::digest(public_bytes);
        let kid = format!(
            "hub-{}",
            kid_hash
                .iter()
                .flat_map(|b| [format!("{:02x}", b)])
                .collect::<String>()[..8]
                .to_string()
        );

        Self {
            kid,
            encoding,
            decoding,
        }
    }
}

fn wrap_ed25519_private_pkcs8(secret: &[u8; 32]) -> Vec<u8> {
    let mut der = Vec::with_capacity(48);
    der.extend_from_slice(&[0x30, 0x2e]);
    der.extend_from_slice(&[0x02, 0x01, 0x00]);
    der.extend_from_slice(&[0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70]);
    der.extend_from_slice(&[0x04, 0x22, 0x04, 0x20]);
    der.extend_from_slice(secret);
    der
}

/// SIA claims for minting test tokens (mirrors hub-api's SiaClaims).
#[derive(Debug, Serialize, Deserialize)]
pub struct TestSiaClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
    pub jti: String,
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

/// Mint a test SIA JWT.
pub fn mint_test_sia(
    keys: &TestSigningKeys,
    issuer: &str,
    user_id: &str,
    pod_id: &str,
    username: &str,
    display_name: &str,
) -> String {
    let now = chrono::Utc::now();
    let claims = TestSiaClaims {
        iss: issuer.to_string(),
        sub: user_id.to_string(),
        aud: pod_id.to_string(),
        iat: now.timestamp(),
        exp: (now + chrono::Duration::seconds(300)).timestamp(),
        jti: voxora_common::id::prefixed_ulid(voxora_common::id::prefix::SIA),
        username: username.to_string(),
        display_name: display_name.to_string(),
        avatar_url: None,
        email: None,
        email_verified: false,
        flags: vec![],
        hub_version: 1,
    };

    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(keys.kid.clone());
    header.typ = Some("voxora-sia+jwt".to_string());

    jsonwebtoken::encode(&header, &claims, &keys.encoding).expect("mint test SIA")
}

/// Mint an expired SIA for testing.
pub fn mint_expired_sia(
    keys: &TestSigningKeys,
    issuer: &str,
    user_id: &str,
    pod_id: &str,
) -> String {
    let now = chrono::Utc::now();
    let claims = TestSiaClaims {
        iss: issuer.to_string(),
        sub: user_id.to_string(),
        aud: pod_id.to_string(),
        iat: (now - chrono::Duration::seconds(600)).timestamp(),
        exp: (now - chrono::Duration::seconds(300)).timestamp(),
        jti: voxora_common::id::prefixed_ulid(voxora_common::id::prefix::SIA),
        username: "expired_user".to_string(),
        display_name: "Expired User".to_string(),
        avatar_url: None,
        email: None,
        email_verified: false,
        flags: vec![],
        hub_version: 1,
    };

    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(keys.kid.clone());
    header.typ = Some("voxora-sia+jwt".to_string());

    jsonwebtoken::encode(&header, &claims, &keys.encoding).expect("mint expired SIA")
}

/// Build a test AppState with in-memory KV and a static JWKS key.
pub async fn test_state() -> (AppState, TestSigningKeys) {
    let env_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    let _ = dotenvy::from_path(env_path);

    let mut config = Config::from_env();
    config.database_url = with_test_db_suffix(&config.database_url);

    let db = pod_api::db::pool::connect(&config.database_url).await;
    let kv: Arc<dyn KeyValueStore> = Arc::new(MemoryStore::new());

    // Use the same seed as the hub-api dev environment.
    let signing_keys = TestSigningKeys::from_seed("dev-seed-do-not-use-in-production");

    // Pre-load the JWKS client with the test key so it doesn't hit the network.
    let jwks = JwksClient::with_static_key(&signing_keys.kid, signing_keys.decoding.clone());

    let snowflake = Arc::new(SnowflakeGenerator::new(0));

    let state = AppState {
        db,
        kv,
        jwks,
        config: Arc::new(config),
        snowflake,
    };

    (state, signing_keys)
}

fn with_test_db_suffix(database_url: &str) -> String {
    let mut parts = database_url.splitn(2, '?');
    let base = parts.next().unwrap_or(database_url);
    let query = parts.next();

    let mut base_parts = base.rsplitn(2, '/');
    let db_name = base_parts.next().unwrap_or("");
    let prefix = base_parts.next().unwrap_or("");

    if db_name.is_empty() || db_name.ends_with("_test") {
        return database_url.to_string();
    }

    let mut updated = format!("{}/{}", prefix, format!("{db_name}_test"));
    if let Some(query) = query {
        updated.push('?');
        updated.push_str(query);
    }
    updated
}

/// Build the full application router wired to the test state.
pub async fn test_app() -> (Router, AppState, TestSigningKeys) {
    let (state, keys) = test_state().await;
    let app = pod_api::routes::router().with_state(state.clone());
    (app, state, keys)
}

/// Clean up a test pod_user.
pub async fn cleanup_test_user(db: &pod_api::db::pool::DbPool, user_id: &str) {
    use diesel::prelude::*;
    use diesel_async::RunQueryDsl;

    let mut conn = db.get().await.expect("pool");
    diesel::delete(
        pod_api::db::schema::pod_users::table
            .filter(pod_api::db::schema::pod_users::id.eq(user_id)),
    )
    .execute(&mut conn)
    .await
    .ok();
}

/// Clean up a test community (CASCADE handles roles, channels, members).
pub async fn cleanup_community(db: &pod_api::db::pool::DbPool, community_id: &str) {
    use diesel::prelude::*;
    use diesel_async::RunQueryDsl;

    let mut conn = db.get().await.expect("pool");
    diesel::delete(
        pod_api::db::schema::communities::table
            .filter(pod_api::db::schema::communities::id.eq(community_id)),
    )
    .execute(&mut conn)
    .await
    .ok();
}

/// Login a test user and return their access token (PAT).
pub async fn login_test_user(
    server: &axum_test::TestServer,
    keys: &TestSigningKeys,
    config: &pod_api::config::Config,
    user_id: &str,
    username: &str,
) -> String {
    let sia = mint_test_sia(keys, &config.hub_url, user_id, &config.pod_id, username, username);
    let resp = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia }))
        .await;
    resp.assert_status_ok();
    resp.json::<serde_json::Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}
