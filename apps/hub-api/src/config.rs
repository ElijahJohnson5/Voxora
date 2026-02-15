/// Hub API configuration, loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string.
    pub database_url: String,
    /// Redis connection string.
    pub redis_url: String,
    /// The public-facing origin of the Hub (e.g. `http://localhost:4001`).
    pub hub_domain: String,
    /// Seed used to derive the Ed25519 signing key (dev only — use KMS in prod).
    pub signing_key_seed: String,
    /// Port the HTTP server binds to.
    pub port: u16,
    /// Shared secret for coturn REST API credential generation.
    pub turn_shared_secret: String,
    /// STUN server URLs.
    pub stun_urls: Vec<String>,
    /// TURN server URLs.
    pub turn_urls: Vec<String>,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Panics with a descriptive message if a required variable is missing.
    pub fn from_env() -> Self {
        Self {
            database_url: required_var("DATABASE_URL"),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379/0".to_string()),
            hub_domain: required_var("HUB_DOMAIN"),
            signing_key_seed: required_var("SIGNING_KEY_SEED"),
            port: std::env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4001),
            turn_shared_secret: required_var("TURN_SHARED_SECRET"),
            stun_urls: csv_var("STUN_URLS", vec!["stun:localhost:3478".to_string()]),
            turn_urls: csv_var(
                "TURN_URLS",
                vec![
                    "turn:localhost:3478?transport=udp".to_string(),
                    "turn:localhost:3478?transport=tcp".to_string(),
                ],
            ),
        }
    }
}

fn required_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} env var is required"))
}

fn csv_var(name: &str, default: Vec<String>) -> Vec<String> {
    match std::env::var(name) {
        Ok(val) if !val.is_empty() => val.split(',').map(|s| s.trim().to_string()).collect(),
        _ => default,
    }
}
