//! PAT (Pod Access Token) extraction middleware.

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::auth::tokens;
use crate::AppState;

/// Authenticated user extracted from the `Authorization: Bearer <pat>` header.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
}

/// Rejection returned when the bearer token is missing or invalid.
pub struct AuthError {
    message: &'static str,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "error": {
                "code": "UNAUTHORIZED",
                "message": self.message
            }
        });
        (StatusCode::UNAUTHORIZED, Json(body)).into_response()
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError {
                message: "Missing Authorization header",
            })?;

        let token = header.strip_prefix("Bearer ").ok_or(AuthError {
            message: "Invalid Authorization header format",
        })?;

        let data = tokens::lookup_pat(state.kv.as_ref(), token)
            .await
            .map_err(|_| AuthError {
                message: "Token lookup failed",
            })?
            .ok_or(AuthError {
                message: "Invalid or expired token",
            })?;

        Ok(AuthUser {
            user_id: data.user_id,
        })
    }
}
