use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::pool::DbPool;
use crate::db::schema::audit_log;
use crate::error::ApiError;

#[derive(Debug, Insertable)]
#[diesel(table_name = audit_log)]
pub struct NewAuditLog<'a> {
    pub id: &'a str,
    pub community_id: &'a str,
    pub actor_id: &'a str,
    pub action: &'a str,
    pub target_type: Option<&'a str>,
    pub target_id: Option<&'a str>,
    pub changes: Option<serde_json::Value>,
    pub reason: Option<&'a str>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = audit_log)]
pub struct AuditLogEntry {
    pub id: String,
    pub community_id: String,
    pub actor_id: String,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub changes: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Reusable helper to insert an audit log entry.
pub async fn log(
    pool: &DbPool,
    community_id: &str,
    actor_id: &str,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    changes: Option<serde_json::Value>,
    reason: Option<&str>,
) -> Result<(), ApiError> {
    let mut conn = pool.get().await?;
    let log_id = voxora_common::id::prefixed_ulid("aud");

    diesel_async::RunQueryDsl::execute(
        diesel::insert_into(audit_log::table).values(NewAuditLog {
            id: &log_id,
            community_id,
            actor_id,
            action,
            target_type,
            target_id,
            changes,
            reason,
            created_at: Utc::now(),
        }),
        &mut conn,
    )
    .await?;

    Ok(())
}
