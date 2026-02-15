use chrono::{DateTime, Utc};
use diesel::prelude::*;

use crate::db::schema::audit_log;

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
