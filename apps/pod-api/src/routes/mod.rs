pub mod auth;
pub mod bans;
pub mod channels;
pub mod communities;
pub mod health;
pub mod invites;
pub mod members;
pub mod messages;
pub mod roles;

use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().merge(health::router()).nest(
        "/api/v1",
        auth::router()
            .merge(communities::router())
            .merge(channels::router())
            .merge(messages::router())
            .merge(invites::router())
            .merge(members::router())
            .merge(roles::router())
            .merge(bans::router()),
    )
}
