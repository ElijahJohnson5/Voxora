CREATE TABLE pod_bans (
    user_id     TEXT PRIMARY KEY REFERENCES pod_users(id),
    reason      TEXT,
    banned_by   TEXT NOT NULL REFERENCES pod_users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
