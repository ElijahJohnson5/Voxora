//! JWKS client for fetching and caching the Hub's Ed25519 public keys.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::ApiError;

/// A cached set of decoding keys fetched from the Hub JWKS endpoint.
#[derive(Clone)]
pub struct JwksClient {
    hub_url: String,
    http: reqwest::Client,
    cache: Arc<RwLock<JwksCache>>,
}

struct JwksCache {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Option<std::time::Instant>,
}

/// How long to cache JWKS before re-fetching (1 hour).
const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkEntry>,
}

#[derive(Debug, Deserialize)]
struct JwkEntry {
    kid: Option<String>,
    kty: String,
    crv: Option<String>,
    x: Option<String>,
}

impl JwksClient {
    pub fn new(hub_url: &str) -> Self {
        Self {
            hub_url: hub_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(JwksCache {
                keys: HashMap::new(),
                fetched_at: None,
            })),
        }
    }

    /// For tests: create a client pre-loaded with a known key.
    pub fn with_static_key(kid: &str, decoding_key: DecodingKey) -> Self {
        let mut keys = HashMap::new();
        keys.insert(kid.to_string(), decoding_key);
        Self {
            hub_url: String::new(),
            http: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(JwksCache {
                keys,
                // Set fetched_at far in the future so it never expires in tests.
                fetched_at: Some(std::time::Instant::now() + std::time::Duration::from_secs(86400)),
            })),
        }
    }

    /// Get the decoding key for a given `kid`. Fetches/re-fetches JWKS as needed.
    pub async fn get_key(&self, kid: &str) -> Result<DecodingKey, ApiError> {
        // Try cache first.
        {
            let cache = self.cache.read().await;
            if let Some(key) = cache.keys.get(kid) {
                if self.cache_is_fresh(&cache) {
                    return Ok(key.clone());
                }
            }
        }

        // Cache miss or stale — re-fetch.
        self.refresh().await?;

        // Try again after refresh.
        let cache = self.cache.read().await;
        cache
            .keys
            .get(kid)
            .cloned()
            .ok_or_else(|| ApiError::unauthorized("Unknown signing key"))
    }

    fn cache_is_fresh(&self, cache: &JwksCache) -> bool {
        match cache.fetched_at {
            Some(t) => t.elapsed() < CACHE_TTL,
            None => false,
        }
    }

    async fn refresh(&self) -> Result<(), ApiError> {
        let url = format!("{}/oidc/.well-known/jwks.json", self.hub_url);
        tracing::info!(%url, "fetching Hub JWKS");

        let resp: JwksResponse = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(?e, "JWKS fetch failed");
                ApiError::internal("Failed to fetch Hub JWKS")
            })?
            .json()
            .await
            .map_err(|e| {
                tracing::error!(?e, "JWKS parse failed");
                ApiError::internal("Failed to parse Hub JWKS")
            })?;

        let mut keys = HashMap::new();
        for entry in resp.keys {
            if entry.kty != "OKP" {
                continue;
            }
            if entry.crv.as_deref() != Some("Ed25519") {
                continue;
            }
            let (Some(kid), Some(x)) = (entry.kid, entry.x) else {
                continue;
            };

            let public_bytes = URL_SAFE_NO_PAD.decode(&x).map_err(|e| {
                tracing::error!(?e, %kid, "bad JWKS x value");
                ApiError::internal("Invalid JWKS key encoding")
            })?;

            let decoding = DecodingKey::from_ed_der(&public_bytes);
            keys.insert(kid, decoding);
        }

        let mut cache = self.cache.write().await;
        cache.keys = keys;
        cache.fetched_at = Some(std::time::Instant::now());

        Ok(())
    }
}
