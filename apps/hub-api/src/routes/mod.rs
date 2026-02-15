pub mod health;
pub mod oidc;
pub mod pods;
pub mod sia;
pub mod users;

use axum::Router;
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};

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

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        // Health
        health::health,
        // OIDC
        oidc::openid_configuration,
        oidc::jwks,
        oidc::authorize,
        oidc::authorize_submit,
        oidc::token,
        oidc::userinfo,
        oidc::revoke,
        // Users
        users::create_user,
        users::get_me,
        users::update_me,
        users::get_user,
        users::get_my_pods,
        users::get_preferences,
        users::update_preferences,
        // SIA
        sia::issue_sia,
        // Pods
        pods::register_pod,
        pods::heartbeat,
        pods::list_pods,
        pods::get_pod,
    ),
    components(
        schemas(
            // Error types
            crate::error::ApiErrorBody,
            crate::error::ApiErrorDetail,
            crate::error::FieldError,
            // User models
            crate::models::user::UserResponse,
            crate::models::user::PublicUserResponse,
            // Pod models
            crate::models::pod::PodResponse,
            crate::models::pod::PodRegistrationResponse,
            // Route request/response types
            health::HealthResponse,
            users::CreateUserRequest,
            users::UpdateProfileRequest,
            users::MyPodEntry,
            users::MyPodsResponse,
            users::PreferencesResponse,
            users::UpdatePreferencesRequest,
            oidc::OpenIdConfiguration,
            oidc::JwksResponse,
            oidc::JwkKey,
            oidc::UserinfoResponse,
            oidc::TokenRequest,
            oidc::TokenResponse,
            oidc::RevokeRequest,
            sia::SiaRequest,
            sia::SiaResponse,
            pods::RegisterPodRequest,
            pods::HeartbeatRequest,
            pods::HeartbeatResponse,
            pods::ListPodsQuery,
            pods::ListPodsResponse,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Health", description = "Health check"),
        (name = "OIDC", description = "OpenID Connect endpoints"),
        (name = "Users", description = "User management"),
        (name = "SIA", description = "Signed Identity Assertions"),
        (name = "Pods", description = "Pod registration and discovery"),
    )
)]
pub struct ApiDoc;
