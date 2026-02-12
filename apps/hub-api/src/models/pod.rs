use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::schema::pods;

/// Full pod row from the database.
#[derive(Debug, Queryable, Selectable, Serialize)]
#[diesel(table_name = pods)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Pod {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub url: String,
    pub region: Option<String>,
    pub client_id: String,
    #[serde(skip)]
    pub client_secret: String,
    pub public: bool,
    pub capabilities: Vec<String>,
    pub max_members: i32,
    pub version: Option<String>,
    pub status: String,
    pub member_count: i32,
    pub online_count: i32,
    pub community_count: i32,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Insertable struct for creating a new pod.
#[derive(Debug, Insertable)]
#[diesel(table_name = pods)]
pub struct NewPod {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub url: String,
    pub region: Option<String>,
    pub client_id: String,
    pub client_secret: String,
    pub public: bool,
    pub capabilities: Vec<String>,
    pub max_members: i32,
    pub version: Option<String>,
}

/// Public-facing pod response (no sensitive fields).
#[derive(Debug, Serialize)]
pub struct PodResponse {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub url: String,
    pub region: Option<String>,
    pub public: bool,
    pub capabilities: Vec<String>,
    pub max_members: i32,
    pub version: Option<String>,
    pub status: String,
    pub member_count: i32,
    pub online_count: i32,
    pub community_count: i32,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Pod> for PodResponse {
    fn from(p: Pod) -> Self {
        Self {
            id: p.id,
            owner_id: p.owner_id,
            name: p.name,
            description: p.description,
            icon_url: p.icon_url,
            url: p.url,
            region: p.region,
            public: p.public,
            capabilities: p.capabilities,
            max_members: p.max_members,
            version: p.version,
            status: p.status,
            member_count: p.member_count,
            online_count: p.online_count,
            community_count: p.community_count,
            last_heartbeat: p.last_heartbeat,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

/// Registration response — includes the client credentials (returned only once).
#[derive(Debug, Serialize)]
pub struct PodRegistrationResponse {
    pub pod_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub registered_at: DateTime<Utc>,
    pub status: String,
}
