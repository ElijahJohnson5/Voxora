use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::schema::communities;
use crate::models::channel::Channel;
use crate::models::role::Role;

#[derive(Debug, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = communities)]
pub struct Community {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub owner_id: String,
    pub default_channel: Option<String>,
    pub member_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = communities)]
pub struct NewCommunity<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub icon_url: Option<&'a str>,
    pub owner_id: &'a str,
    pub default_channel: Option<&'a str>,
    pub member_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = communities)]
pub struct UpdateCommunity {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CommunityResponse {
    #[serde(flatten)]
    pub community: Community,
    pub channels: Vec<Channel>,
    pub roles: Vec<Role>,
}
