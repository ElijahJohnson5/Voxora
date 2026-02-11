pub mod health;

use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().merge(health::router())
}
