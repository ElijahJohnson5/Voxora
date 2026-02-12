pub mod config;
pub mod db;
pub mod error;
pub mod routes;

use std::sync::Arc;

use config::Config;
use db::pool::DbPool;

/// Shared application state available to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub config: Arc<Config>,
}
