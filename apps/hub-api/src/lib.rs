pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;

use std::sync::Arc;

use auth::keys::SigningKeys;
use config::Config;
use db::kv::KeyValueStore;
use db::pool::DbPool;

/// Shared application state available to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub kv: Arc<dyn KeyValueStore>,
    pub keys: Arc<SigningKeys>,
    pub config: Arc<Config>,
}
