//! Unit + integration tests for `auth::tokens` — opaque token generation,
//! ID token minting, and KV-backed storage.

mod common;

use hub_api::auth::keys::SigningKeys;
use hub_api::auth::tokens::*;

// ---------------------------------------------------------------------------
// Pure / unit tests (no KV needed)
// ---------------------------------------------------------------------------

#[test]
fn access_token_has_hat_prefix() {
    let t = generate_access_token();
    assert!(t.starts_with("hat_"), "access token must start with 'hat_'");
    assert!(t.len() > 10, "token must have reasonable length");
}

#[test]
fn refresh_token_has_hrt_prefix() {
    let t = generate_refresh_token();
    assert!(
        t.starts_with("hrt_"),
        "refresh token must start with 'hrt_'"
    );
}

#[test]
fn opaque_tokens_are_unique() {
    let a = generate_access_token();
    let b = generate_access_token();
    assert_ne!(a, b, "successive tokens must differ");
}

#[test]
fn opaque_token_custom_prefix() {
    let t = generate_opaque_token("hac", 16);
    assert!(t.starts_with("hac_"));
}

#[test]
fn mint_id_token_minimal_scopes() {
    let keys = SigningKeys::from_seed("test-mint");

    let jwt = mint_id_token(
        &keys,
        "http://localhost:4001",
        "voxora-web",
        "usr_abc",
        None,
        &["openid".to_string()],
        "alice",
        "Alice",
        None,
        Some("alice@example.com"),
        true,
    )
    .expect("minting must succeed");

    // Decode and verify claims
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    validation.set_audience(&["voxora-web"]);
    validation.set_issuer(&["http://localhost:4001"]);

    let data = jsonwebtoken::decode::<IdTokenClaims>(&jwt, &keys.decoding, &validation)
        .expect("decode must succeed");

    assert_eq!(data.claims.sub, "usr_abc");
    assert_eq!(data.claims.aud, "voxora-web");
    assert_eq!(data.claims.iss, "http://localhost:4001");
    // With only "openid" scope, profile/email claims should be absent
    assert!(data.claims.preferred_username.is_none());
    assert!(data.claims.email.is_none());
    assert!(data.claims.nonce.is_none());
}

#[test]
fn mint_id_token_profile_scope() {
    let keys = SigningKeys::from_seed("test-profile");

    let jwt = mint_id_token(
        &keys,
        "http://localhost:4001",
        "voxora-web",
        "usr_abc",
        Some("n123"),
        &["openid".to_string(), "profile".to_string()],
        "bob",
        "Bob Builder",
        Some("https://example.com/avatar.png"),
        None,
        false,
    )
    .unwrap();

    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    validation.set_audience(&["voxora-web"]);
    validation.set_issuer(&["http://localhost:4001"]);

    let data = jsonwebtoken::decode::<IdTokenClaims>(&jwt, &keys.decoding, &validation).unwrap();

    assert_eq!(data.claims.preferred_username.as_deref(), Some("bob"));
    assert_eq!(data.claims.name.as_deref(), Some("Bob Builder"));
    assert_eq!(
        data.claims.picture.as_deref(),
        Some("https://example.com/avatar.png")
    );
    assert_eq!(data.claims.nonce.as_deref(), Some("n123"));
    // email scope not requested
    assert!(data.claims.email.is_none());
}

#[test]
fn mint_id_token_email_scope() {
    let keys = SigningKeys::from_seed("test-email");

    let jwt = mint_id_token(
        &keys,
        "http://localhost:4001",
        "voxora-web",
        "usr_abc",
        None,
        &["openid".to_string(), "email".to_string()],
        "carol",
        "Carol",
        None,
        Some("carol@example.com"),
        true,
    )
    .unwrap();

    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    validation.set_audience(&["voxora-web"]);
    validation.set_issuer(&["http://localhost:4001"]);

    let data = jsonwebtoken::decode::<IdTokenClaims>(&jwt, &keys.decoding, &validation).unwrap();

    assert_eq!(data.claims.email.as_deref(), Some("carol@example.com"));
    assert_eq!(data.claims.email_verified, Some(true));
    // profile scope not requested
    assert!(data.claims.preferred_username.is_none());
}

#[test]
fn mint_id_token_all_scopes() {
    let keys = SigningKeys::from_seed("test-all");

    let jwt = mint_id_token(
        &keys,
        "http://localhost:4001",
        "voxora-web",
        "usr_abc",
        Some("nonce_val"),
        &[
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
        ],
        "dave",
        "Dave D",
        None,
        Some("dave@example.com"),
        false,
    )
    .unwrap();

    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    validation.set_audience(&["voxora-web"]);
    validation.set_issuer(&["http://localhost:4001"]);

    let data = jsonwebtoken::decode::<IdTokenClaims>(&jwt, &keys.decoding, &validation).unwrap();

    assert_eq!(data.claims.preferred_username.as_deref(), Some("dave"));
    assert_eq!(data.claims.name.as_deref(), Some("Dave D"));
    assert_eq!(data.claims.email.as_deref(), Some("dave@example.com"));
    assert_eq!(data.claims.email_verified, Some(false));
    assert_eq!(data.claims.nonce.as_deref(), Some("nonce_val"));
}

#[test]
fn id_token_header_has_kid() {
    let keys = SigningKeys::from_seed("test-kid");

    let jwt = mint_id_token(
        &keys,
        "http://localhost:4001",
        "voxora-web",
        "usr_abc",
        None,
        &["openid".to_string()],
        "eve",
        "Eve",
        None,
        None,
        false,
    )
    .unwrap();

    let header = jsonwebtoken::decode_header(&jwt).unwrap();
    assert_eq!(header.alg, jsonwebtoken::Algorithm::EdDSA);
    assert_eq!(header.kid.as_deref(), Some(keys.kid.as_str()));
}

// ---------------------------------------------------------------------------
// KV integration tests (use in-memory store)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn store_and_lookup_access_token() {
    let state = common::test_state().await;
    let kv = state.kv.as_ref();

    let token = generate_access_token();
    let data = AccessTokenData {
        user_id: "usr_test1".to_string(),
        scopes: vec!["openid".to_string(), "profile".to_string()],
    };

    store_access_token(kv, &token, &data).await.unwrap();
    let found = lookup_access_token(kv, &token).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.user_id, "usr_test1");
    assert_eq!(found.scopes, vec!["openid", "profile"]);

    // Clean up
    delete_access_token(kv, &token).await.unwrap();
}

#[tokio::test]
async fn lookup_missing_access_token_returns_none() {
    let state = common::test_state().await;
    let kv = state.kv.as_ref();

    let result = lookup_access_token(kv, "hat_nonexistent")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn delete_access_token_removes_it() {
    let state = common::test_state().await;
    let kv = state.kv.as_ref();

    let token = generate_access_token();
    let data = AccessTokenData {
        user_id: "usr_del".to_string(),
        scopes: vec!["openid".to_string()],
    };

    store_access_token(kv, &token, &data).await.unwrap();
    delete_access_token(kv, &token).await.unwrap();

    let result = lookup_access_token(kv, &token).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn store_and_consume_auth_code() {
    let state = common::test_state().await;
    let kv = state.kv.as_ref();

    let code = generate_opaque_token("hac", 32);
    let data = AuthCodeData {
        user_id: "usr_code1".to_string(),
        client_id: "voxora-web".to_string(),
        redirect_uri: "http://localhost:5173/callback".to_string(),
        code_challenge: "test_challenge".to_string(),
        scopes: vec!["openid".to_string()],
        nonce: Some("n1".to_string()),
    };

    store_auth_code(kv, &code, &data).await.unwrap();

    // First consume should succeed
    let found = consume_auth_code(kv, &code).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.user_id, "usr_code1");
    assert_eq!(found.nonce.as_deref(), Some("n1"));

    // Second consume should return None (single-use)
    let again = consume_auth_code(kv, &code).await.unwrap();
    assert!(again.is_none(), "auth code must be single-use");
}
