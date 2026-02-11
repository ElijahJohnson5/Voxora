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
        }
    }
}

fn required_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} env var is required"))
}
