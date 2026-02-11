use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use hub_api::auth::keys::SigningKeys;
use hub_api::config::Config;
use hub_api::AppState;
use std::path::Path;

#[tokio::main]
async fn main() {
    // Load .env file (silently skip if missing — env vars may be set externally)
    if dotenvy::dotenv().is_err() {
        let env_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
        let _ = dotenvy::from_path(env_path);
    }

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();
    let port = config.port;

    // Connect to PostgreSQL
    let db = hub_api::db::pool::connect(&config.database_url).await;

    // Connect to Redis
    let redis_client = redis::Client::open(config.redis_url.as_str()).expect("invalid REDIS_URL");
    let redis = redis::aio::ConnectionManager::new(redis_client)
        .await
        .expect("failed to connect to Redis");
    tracing::info!("redis connected");

    // Derive Ed25519 signing keys from seed
    let keys = Arc::new(SigningKeys::from_seed(&config.signing_key_seed));
    tracing::info!(kid = %keys.kid, "signing keys loaded");

    let state = AppState {
        db,
        redis,
        keys,
        config: Arc::new(config),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(hub_api::routes::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(%addr, "hub-api listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind");
    axum::serve(listener, app).await.expect("server error");
}
