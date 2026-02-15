//! Audit log query endpoint.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::middleware::AuthUser;
use crate::db::schema::audit_log;
use crate::error::{ApiError, ApiErrorBody};
use crate::models::audit_log::AuditLogEntry;
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/communities/{community_id}/audit-log",
        get(list_audit_log),
    )
}

#[derive(Debug, Deserialize)]
pub struct AuditLogParams {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub before: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditLogResponse {
    pub data: Vec<AuditLogEntry>,
    pub has_more: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/audit-log",
    tag = "Audit Log",
    security(("bearer" = [])),
    params(
        ("community_id" = String, Path, description = "Community ID"),
        ("user_id" = Option<String>, Query, description = "Filter by actor user ID"),
        ("action" = Option<String>, Query, description = "Filter by action type"),
        ("before" = Option<String>, Query, description = "Cursor: audit log entry ID"),
        ("limit" = Option<i64>, Query, description = "Number of entries (1-100, default 50)"),
    ),
    responses(
        (status = 200, description = "Audit log entries", body = AuditLogResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
    ),
)]
pub async fn list_audit_log(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(community_id): Path<String>,
    Query(params): Query<AuditLogParams>,
) -> Result<Json<AuditLogResponse>, ApiError> {
    // Check VIEW_AUDIT_LOG permission (owner bypasses via check_permission).
    permissions::check_permission(
        &state.db,
        &community_id,
        &user_id,
        permissions::VIEW_AUDIT_LOG,
    )
    .await?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100);

    let mut conn = state.db.get().await?;

    let mut query = audit_log::table
        .filter(audit_log::community_id.eq(&community_id))
        .order(audit_log::id.desc())
        .limit(limit + 1)
        .select(AuditLogEntry::as_select())
        .into_boxed();

    if let Some(ref action) = params.action {
        query = query.filter(audit_log::action.eq(action));
    }

    if let Some(ref actor_id) = params.user_id {
        query = query.filter(audit_log::actor_id.eq(actor_id));
    }

    if let Some(ref before) = params.before {
        query = query.filter(audit_log::id.lt(before));
    }

    let rows: Vec<AuditLogEntry> =
        diesel_async::RunQueryDsl::load(query, &mut conn).await?;

    let has_more = rows.len() as i64 > limit;
    let data: Vec<AuditLogEntry> = rows.into_iter().take(limit as usize).collect();

    Ok(Json(AuditLogResponse { data, has_more }))
}
