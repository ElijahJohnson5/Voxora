use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::schema::pod_roles;

#[derive(Debug, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = pod_roles)]
pub struct PodRole {
    pub id: String,
    pub name: String,
    pub position: i32,
    pub permissions: i64,
    pub is_default: bool,
    pub color: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = pod_roles)]
pub struct NewPodRole<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub position: i32,
    pub permissions: i64,
    pub is_default: bool,
    pub color: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = pod_roles)]
pub struct UpdatePodRole {
    pub name: Option<String>,
    pub color: Option<Option<i32>>,
    pub position: Option<i32>,
    pub permissions: Option<i64>,
}
