use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::schema::pod_bans;

#[derive(Debug, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = pod_bans)]
pub struct PodBan {
    pub user_id: String,
    pub reason: Option<String>,
    pub banned_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = pod_bans)]
pub struct NewPodBan<'a> {
    pub user_id: &'a str,
    pub reason: Option<&'a str>,
    pub banned_by: &'a str,
}
