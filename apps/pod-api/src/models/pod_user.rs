use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::schema::pod_users;

/// A user record local to this Pod, created from a Hub SIA.
#[derive(Debug, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = pod_users)]
pub struct PodUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub hub_flags: i64,
    pub status: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// Insertable form for creating a new pod_user.
#[derive(Debug, Insertable)]
#[diesel(table_name = pod_users)]
pub struct NewPodUser<'a> {
    pub id: &'a str,
    pub username: &'a str,
    pub display_name: &'a str,
    pub avatar_url: Option<&'a str>,
    pub hub_flags: i64,
    pub status: &'a str,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// Upsert a user from SIA claims. Creates or updates the local record.
pub async fn upsert_from_sia(
    pool: &crate::db::pool::DbPool,
    user_id: &str,
    username: &str,
    display_name: &str,
    avatar_url: Option<&str>,
    hub_flags: i64,
) -> Result<PodUser, crate::error::ApiError> {
    let now = Utc::now();
    let mut conn = pool.get().await?;

    let query = diesel::insert_into(pod_users::table)
        .values(NewPodUser {
            id: user_id,
            username,
            display_name,
            avatar_url,
            hub_flags,
            status: "online",
            first_seen_at: now,
            last_seen_at: now,
        })
        .on_conflict(pod_users::id)
        .do_update()
        .set((
            pod_users::username.eq(username),
            pod_users::display_name.eq(display_name),
            pod_users::avatar_url.eq(avatar_url),
            pod_users::hub_flags.eq(hub_flags),
            pod_users::last_seen_at.eq(now),
        ))
        .returning(PodUser::as_returning());

    let user: PodUser = diesel_async::RunQueryDsl::get_result(query, &mut conn).await?;

    Ok(user)
}
