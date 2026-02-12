pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod gateway;
pub mod models;
pub mod permissions;
pub mod routes;

use std::sync::Arc;

use auth::jwks::JwksClient;
use config::Config;
use db::kv::KeyValueStore;
use db::pool::DbPool;
use gateway::fanout::GatewayBroadcast;
use voxora_common::SnowflakeGenerator;

/// Shared application state available to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub kv: Arc<dyn KeyValueStore>,
    pub jwks: JwksClient,
    pub config: Arc<Config>,
    pub snowflake: Arc<SnowflakeGenerator>,
    pub broadcast: Arc<GatewayBroadcast>,
}
