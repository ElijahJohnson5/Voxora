/// Pod API configuration, loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string.
    pub database_url: String,
    /// The Hub API origin (e.g. `http://localhost:4001`).
    pub hub_url: String,
    /// This Pod's registered ID on the Hub.
    pub pod_id: String,
    /// Client ID issued by the Hub during Pod registration.
    pub pod_client_id: String,
    /// Client secret issued by the Hub during Pod registration.
    pub pod_client_secret: String,
    /// Port the HTTP server binds to.
    pub port: u16,
    /// Optional pod owner user ID. When set, this user gets implicit POD_ADMINISTRATOR.
    pub pod_owner_id: Option<String>,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Panics with a descriptive message if a required variable is missing.
    pub fn from_env() -> Self {
        Self {
            database_url: required_var("DATABASE_URL"),
            hub_url: required_var("HUB_URL"),
            pod_id: required_var("POD_ID"),
            pod_client_id: required_var("POD_CLIENT_ID"),
            pod_client_secret: required_var("POD_CLIENT_SECRET"),
            port: std::env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4002),
            pod_owner_id: std::env::var("POD_OWNER_ID").ok().filter(|s| !s.is_empty()),
        }
    }
}

fn required_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} env var is required"))
}
