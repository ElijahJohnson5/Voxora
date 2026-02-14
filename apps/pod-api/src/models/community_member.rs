use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::schema::community_members;

/// Raw row from the `community_members` table (used for inserts / updates).
#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = community_members)]
pub struct CommunityMemberRow {
    pub community_id: String,
    pub user_id: String,
    pub nickname: Option<String>,
    pub roles: Vec<String>,
    pub joined_at: DateTime<Utc>,
}

/// Enriched member returned by the API (member + user info).
#[derive(Debug, Serialize, ToSchema)]
pub struct CommunityMember {
    pub community_id: String,
    pub user_id: String,
    pub nickname: Option<String>,
    pub roles: Vec<String>,
    pub joined_at: DateTime<Utc>,
    pub display_name: String,
    pub username: String,
    pub avatar_url: Option<String>,
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
