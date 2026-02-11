pub mod health;
pub mod users;

use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .nest("/api/v1", users::router())
}
