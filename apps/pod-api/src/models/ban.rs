use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::schema::bans;

#[derive(Debug, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = bans)]
pub struct Ban {
    pub community_id: String,
    pub user_id: String,
    pub reason: Option<String>,
    pub banned_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = bans)]
pub struct NewBan<'a> {
    pub community_id: &'a str,
    pub user_id: &'a str,
    pub reason: Option<&'a str>,
    pub banned_by: &'a str,
}
