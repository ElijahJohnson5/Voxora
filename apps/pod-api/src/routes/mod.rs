pub mod auth;
pub mod channels;
pub mod communities;
pub mod health;
pub mod messages;

use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().merge(health::router()).nest(
        "/api/v1",
        auth::router()
            .merge(communities::router())
            .merge(channels::router())
            .merge(messages::router()),
    )
}
