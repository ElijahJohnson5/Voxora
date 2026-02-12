use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use pod_api::auth::jwks::JwksClient;
use pod_api::config::Config;
use pod_api::db::kv::{KeyValueStore, MemoryStore};
use pod_api::AppState;
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

    // Connect to PostgreSQL.
    let db = pod_api::db::pool::connect(&config.database_url).await;

    // In-memory KV store for Phase 1. Replace with RedisStore when Redis is added.
    let kv: Arc<dyn KeyValueStore> = Arc::new(MemoryStore::new());

    // JWKS client for validating Hub SIA tokens.
    let jwks = JwksClient::new(&config.hub_url);

    tracing::info!(pod_id = %config.pod_id, hub_url = %config.hub_url, "pod-api configured");

    let state = AppState {
        db,
        kv,
        jwks,
        config: Arc::new(config),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(pod_api::routes::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(%addr, "pod-api listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind");
    axum::serve(listener, app).await.expect("server error");
}
