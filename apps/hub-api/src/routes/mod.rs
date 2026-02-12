pub mod health;
pub mod oidc;
pub mod pods;
pub mod sia;
pub mod users;

use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        // OIDC/OAuth routes live outside /api/v1 (standards-based paths).
        .merge(oidc::router())
        .nest(
            "/api/v1",
            users::router().merge(sia::router()).merge(pods::router()),
        )
}
