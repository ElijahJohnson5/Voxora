use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::schema::channel_overrides;

#[derive(Debug, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = channel_overrides)]
pub struct ChannelOverride {
    pub channel_id: String,
    pub target_type: i16,
    pub target_id: String,
    pub allow: i64,
    pub deny: i64,
}

#[derive(Debug, Insertable, AsChangeset)]
#[diesel(table_name = channel_overrides)]
pub struct NewChannelOverride<'a> {
    pub channel_id: &'a str,
    pub target_type: i16,
    pub target_id: &'a str,
    pub allow: i64,
    pub deny: i64,
}
