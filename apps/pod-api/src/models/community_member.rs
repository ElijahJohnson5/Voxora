use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::schema::community_members;

#[derive(Debug, Queryable, Selectable, Serialize)]
#[diesel(table_name = community_members)]
pub struct CommunityMember {
    pub community_id: String,
    pub user_id: String,
    pub nickname: Option<String>,
    pub roles: Vec<String>,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = community_members)]
pub struct NewCommunityMember<'a> {
    pub community_id: &'a str,
    pub user_id: &'a str,
    pub nickname: Option<&'a str>,
    pub roles: Vec<String>,
    pub joined_at: DateTime<Utc>,
}
