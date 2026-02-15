use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use pod_api::auth::jwks::JwksClient;
use pod_api::config::Config;
use pod_api::db::kv::{KeyValueStore, MemoryStore};
use pod_api::gateway::fanout::GatewayBroadcast;
use pod_api::gateway::presence::PresenceRegistry;
use pod_api::gateway::registry::SessionRegistry;
use pod_api::routes::ApiDoc;
use pod_api::AppState;
use std::path::Path;
use voxora_common::SnowflakeGenerator;

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

    let snowflake = Arc::new(SnowflakeGenerator::new(0));
    let broadcast = Arc::new(GatewayBroadcast::new());
    let sessions = Arc::new(SessionRegistry::new());
    let presence = Arc::new(PresenceRegistry::new());

    // Spawn background task to clean up expired gateway sessions (every 60s).
    let cleanup_sessions = sessions.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let removed = cleanup_sessions.cleanup_expired();
            if removed > 0 {
                tracing::debug!(removed, "cleaned up expired gateway sessions");
            }
        }
    });

    // Spawn background task to sweep expired presence entries (every 5s).
    {
        let sweep_presence = presence.clone();
        let sweep_broadcast = broadcast.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            let grace = Duration::from_secs(30);
            loop {
                interval.tick().await;
                let gone_offline = sweep_presence.sweep_offline(grace);
                for user in gone_offline {
                    for community_id in &user.communities {
                        sweep_broadcast.dispatch(
                            pod_api::gateway::fanout::BroadcastPayload {
                                community_id: community_id.clone(),
                                event_name: "PRESENCE_UPDATE".to_string(),
                                data: serde_json::json!({
                                    "user_id": user.user_id,
                                    "status": "offline",
                                }),
                            },
                        );
                    }
                }
            }
        });
    }

    let state = AppState {
        db,
        kv,
        jwks,
        config: Arc::new(config),
        snowflake,
        broadcast,
        sessions,
        presence,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(pod_api::routes::router())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
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
