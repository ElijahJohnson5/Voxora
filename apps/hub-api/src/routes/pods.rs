use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthUser;
use crate::auth::tokens;
use crate::db::schema::pods;
use crate::error::{ApiError, FieldError};
use crate::models::pod::{NewPod, Pod, PodRegistrationResponse, PodResponse};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/pods/register", post(register_pod))
        .route("/pods", get(list_pods))
        .route("/pods/{pod_id}", get(get_pod))
        .route("/pods/{pod_id}/heartbeat", post(heartbeat))
}

// =========================================================================
// POST /api/v1/pods/register — Register a new Pod
// =========================================================================

#[derive(Debug, Deserialize)]
pub struct RegisterPodRequest {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default = "default_public")]
    pub public: bool,
    #[serde(default = "default_capabilities")]
    pub capabilities: Vec<String>,
    #[serde(default = "default_max_members")]
    pub max_members: i32,
    #[serde(default)]
    pub version: Option<String>,
}

fn default_public() -> bool {
    true
}

fn default_capabilities() -> Vec<String> {
    vec!["text".to_string()]
}

fn default_max_members() -> i32 {
    10_000
}

/// `POST /api/v1/pods/register` — Register a new Pod.
async fn register_pod(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RegisterPodRequest>,
) -> Result<(StatusCode, Json<PodRegistrationResponse>), ApiError> {
    // --- Validation ---
    let mut errors: Vec<FieldError> = Vec::new();

    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > 100 {
        errors.push(FieldError {
            field: "name".into(),
            message: "Name must be 1–100 characters".into(),
        });
    }

    let url = body.url.trim().to_string();
    if url.is_empty() {
        errors.push(FieldError {
            field: "url".into(),
            message: "URL is required".into(),
        });
    } else if !url.starts_with("http://") && !url.starts_with("https://") {
        errors.push(FieldError {
            field: "url".into(),
            message: "URL must start with http:// or https://".into(),
        });
    }

    if body.max_members < 1 {
        errors.push(FieldError {
            field: "max_members".into(),
            message: "max_members must be at least 1".into(),
        });
    }

    if let Some(ref desc) = body.description {
        if desc.len() > 1000 {
            errors.push(FieldError {
                field: "description".into(),
                message: "Description must be at most 1000 characters".into(),
            });
        }
    }

    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }

    // --- Generate credentials ---
    let pod_id = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::POD);
    let client_id = tokens::generate_opaque_token("pod_client", 24);
    let client_secret = tokens::generate_opaque_token("vxs", 32);

    let new_pod = NewPod {
        id: pod_id.clone(),
        owner_id: auth.user_id.clone(),
        name,
        description: body.description,
        icon_url: body.icon_url,
        url,
        region: body.region,
        client_id: client_id.clone(),
        client_secret: client_secret.clone(),
        public: body.public,
        capabilities: body.capabilities,
        max_members: body.max_members,
        version: body.version,
    };

    // --- Insert ---
    let mut conn = state.db.get().await?;

    let pod: Pod = diesel::insert_into(pods::table)
        .values(&new_pod)
        .returning(pods::all_columns)
        .get_result(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                ref info,
            ) => {
                let constraint = info.constraint_name().unwrap_or("");
                if constraint.contains("client_id") {
                    ApiError::conflict("A pod with that client_id already exists")
                } else {
                    ApiError::conflict("A pod with that information already exists")
                }
            }
            other => ApiError::from(other),
        })?;

    tracing::info!(
        pod_id = %pod.id,
        owner_id = %auth.user_id,
        name = %pod.name,
        "pod registered"
    );

    Ok((
        StatusCode::CREATED,
        Json(PodRegistrationResponse {
            pod_id: pod.id,
            client_id,
            client_secret,
            registered_at: pod.created_at,
            status: pod.status,
        }),
    ))
}

// =========================================================================
// POST /api/v1/pods/{pod_id}/heartbeat — Pod heartbeat
// =========================================================================

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    #[serde(default)]
    pub member_count: Option<i32>,
    #[serde(default)]
    pub online_count: Option<i32>,
    #[serde(default)]
    pub community_count: Option<i32>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HeartbeatResponse {
    pub ok: bool,
    pub recorded_at: String,
}

/// `POST /api/v1/pods/{pod_id}/heartbeat` — Record a Pod heartbeat.
///
/// Authenticated via the Pod's `client_id`/`client_secret` pair sent as a
/// Bearer token (the `client_secret` value). For Phase 1 we look the pod up
/// by its id and verify the secret from the `Authorization` header.
async fn heartbeat(
    State(state): State<AppState>,
    Path(pod_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, ApiError> {
    // Extract the Bearer token (client_secret).
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("Missing Authorization header"))?;

    let provided_secret = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::unauthorized("Invalid Authorization header format"))?;

    let mut conn = state.db.get().await?;

    // Look up the pod and verify the secret.
    let pod: Pod = pods::table
        .find(&pod_id)
        .select(Pod::as_select())
        .first(&mut conn)
        .await
        .optional()
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("Pod not found"))?;

    if pod.client_secret != provided_secret {
        return Err(ApiError::unauthorized("Invalid client credentials"));
    }

    // Update heartbeat fields.
    let now = Utc::now();

    diesel::update(pods::table.find(&pod_id))
        .set((
            pods::last_heartbeat.eq(now),
            pods::updated_at.eq(now),
            pods::member_count.eq(body.member_count.unwrap_or(pod.member_count)),
            pods::online_count.eq(body.online_count.unwrap_or(pod.online_count)),
            pods::community_count.eq(body.community_count.unwrap_or(pod.community_count)),
            pods::version.eq(body.version.as_deref().or(pod.version.as_deref())),
        ))
        .execute(&mut conn)
        .await
        .map_err(ApiError::from)?;

    tracing::debug!(pod_id = %pod_id, "heartbeat recorded");

    Ok(Json(HeartbeatResponse {
        ok: true,
        recorded_at: now.to_rfc3339(),
    }))
}

// =========================================================================
// GET /api/v1/pods — List pods
// =========================================================================

#[derive(Debug, Deserialize)]
pub struct ListPodsQuery {
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_sort() -> String {
    "popular".to_string()
}

fn default_limit() -> i64 {
    25
}

#[derive(Debug, Serialize)]
pub struct ListPodsResponse {
    pub data: Vec<PodResponse>,
    pub has_more: bool,
}

/// `GET /api/v1/pods` — List active pods.
async fn list_pods(
    State(state): State<AppState>,
    Query(params): Query<ListPodsQuery>,
) -> Result<Json<ListPodsResponse>, ApiError> {
    let limit = params.limit.clamp(1, 100);
    let mut conn = state.db.get().await?;

    // Only return active, public pods.
    let mut query = pods::table
        .filter(pods::status.eq("active"))
        .filter(pods::public.eq(true))
        .into_boxed();

    // Cursor-based pagination.
    if let Some(ref after) = params.after {
        query = query.filter(pods::id.gt(after.clone()));
    }
    if let Some(ref before) = params.before {
        query = query.filter(pods::id.lt(before.clone()));
    }

    // Sort.
    query = match params.sort.as_str() {
        "newest" => query.order(pods::created_at.desc()),
        // "popular" — sort by member_count descending, then id for stability.
        _ => query.order((pods::member_count.desc(), pods::id.asc())),
    };

    // Fetch one extra to determine `has_more`.
    let rows: Vec<Pod> = query
        .limit(limit + 1)
        .select(Pod::as_select())
        .load(&mut conn)
        .await
        .map_err(ApiError::from)?;

    let has_more = rows.len() as i64 > limit;
    let data: Vec<PodResponse> = rows
        .into_iter()
        .take(limit as usize)
        .map(PodResponse::from)
        .collect();

    Ok(Json(ListPodsResponse { data, has_more }))
}

// =========================================================================
// GET /api/v1/pods/{pod_id} — Pod details
// =========================================================================

/// `GET /api/v1/pods/{pod_id}` — Get a single pod's details.
async fn get_pod(
    State(state): State<AppState>,
    Path(pod_id): Path<String>,
) -> Result<Json<PodResponse>, ApiError> {
    let mut conn = state.db.get().await?;

    let pod: Pod = pods::table
        .find(&pod_id)
        .select(Pod::as_select())
        .first(&mut conn)
        .await
        .optional()
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("Pod not found"))?;

    Ok(Json(PodResponse::from(pod)))
}
