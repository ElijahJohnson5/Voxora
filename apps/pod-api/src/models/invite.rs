use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::schema::invites;

#[derive(Debug, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = invites)]
pub struct Invite {
    pub code: String,
    pub community_id: String,
    pub channel_id: Option<String>,
    pub inviter_id: String,
    pub max_uses: Option<i32>,
    pub use_count: i32,
    pub max_age_seconds: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = invites)]
pub struct NewInvite<'a> {
    pub code: &'a str,
    pub community_id: &'a str,
    pub channel_id: Option<&'a str>,
    pub inviter_id: &'a str,
    pub max_uses: Option<i32>,
    pub use_count: i32,
    pub max_age_seconds: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}
