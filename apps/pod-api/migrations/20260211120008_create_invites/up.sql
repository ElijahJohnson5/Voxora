CREATE TABLE invites (
    code            TEXT PRIMARY KEY,
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    channel_id      TEXT REFERENCES channels(id),
    inviter_id      TEXT NOT NULL REFERENCES pod_users(id),
    max_uses        INTEGER,
    use_count       INTEGER NOT NULL DEFAULT 0,
    max_age_seconds INTEGER,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ
);
