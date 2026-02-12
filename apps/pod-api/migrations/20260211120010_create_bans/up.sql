CREATE TABLE bans (
    community_id TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    user_id      TEXT NOT NULL REFERENCES pod_users(id) ON DELETE CASCADE,
    reason       TEXT,
    banned_by    TEXT NOT NULL REFERENCES pod_users(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (community_id, user_id)
);
