pub mod auth;
pub mod channels;
pub mod communities;
pub mod health;

use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(auth::router())
        .merge(communities::router())
        .merge(channels::router())
}
