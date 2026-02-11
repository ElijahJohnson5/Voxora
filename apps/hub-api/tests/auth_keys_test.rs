//! Unit tests for `auth::keys` — Ed25519 key derivation, signing, and JWKS helpers.

use hub_api::auth::keys::SigningKeys;
use jsonwebtoken::{Algorithm, Validation};

#[test]
fn from_seed_is_deterministic() {
    let a = SigningKeys::from_seed("test-seed");
    let b = SigningKeys::from_seed("test-seed");
    assert_eq!(a.kid, b.kid);
    assert_eq!(a.public_key_b64, b.public_key_b64);
}

#[test]
fn different_seeds_produce_different_keys() {
    let a = SigningKeys::from_seed("seed-alpha");
    let b = SigningKeys::from_seed("seed-beta");
    assert_ne!(a.kid, b.kid);
    assert_ne!(a.public_key_b64, b.public_key_b64);
}

#[test]
fn kid_has_hub_prefix() {
    let keys = SigningKeys::from_seed("test-seed");
    assert!(keys.kid.starts_with("hub-"), "kid should start with 'hub-'");
    // hub- + 8 hex chars = 12 chars total
    assert_eq!(keys.kid.len(), 12);
}

#[test]
fn public_key_b64_is_valid_base64url() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let keys = SigningKeys::from_seed("test-seed");
    let decoded = URL_SAFE_NO_PAD.decode(&keys.public_key_b64);
    assert!(decoded.is_ok(), "public_key_b64 should be valid base64url");
    assert_eq!(decoded.unwrap().len(), 32, "Ed25519 public key is 32 bytes");
}

#[test]
fn sign_and_verify_roundtrip() {
    let keys = SigningKeys::from_seed("roundtrip-seed");

    // Use mint_id_token which is proven to work end-to-end
    let jwt = hub_api::auth::tokens::mint_id_token(
        &keys,
        "http://test.local",
        "test-aud",
        "usr_123",
        Some("nonce1"),
        &["openid".to_string(), "profile".to_string()],
        "testuser",
        "Test User",
        None,
        None,
        false,
    )
    .expect("minting must succeed");

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_audience(&["test-aud"]);
    validation.set_issuer(&["http://test.local"]);

    let decoded = jsonwebtoken::decode::<hub_api::auth::tokens::IdTokenClaims>(
        &jwt,
        &keys.decoding,
        &validation,
    )
    .expect("decoding must succeed");

    assert_eq!(decoded.claims.sub, "usr_123");
    assert_eq!(
        decoded.claims.preferred_username.as_deref(),
        Some("testuser")
    );
    assert_eq!(decoded.header.kid.as_deref(), Some(keys.kid.as_str()));
    assert_eq!(decoded.header.alg, Algorithm::EdDSA);
}

#[test]
fn cannot_verify_with_different_key() {
    let keys_a = SigningKeys::from_seed("key-a");
    let keys_b = SigningKeys::from_seed("key-b");

    let jwt = hub_api::auth::tokens::mint_id_token(
        &keys_a,
        "http://test.local",
        "test-aud",
        "usr_456",
        None,
        &["openid".to_string()],
        "testuser",
        "Test",
        None,
        None,
        false,
    )
    .unwrap();

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_audience(&["test-aud"]);
    validation.set_issuer(&["http://test.local"]);

    let result = jsonwebtoken::decode::<hub_api::auth::tokens::IdTokenClaims>(
        &jwt,
        &keys_b.decoding,
        &validation,
    );
    assert!(result.is_err(), "verifying with wrong key should fail");
}
