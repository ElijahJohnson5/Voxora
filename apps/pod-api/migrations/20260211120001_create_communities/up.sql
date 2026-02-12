CREATE TABLE communities (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT,
    icon_url        TEXT,
    owner_id        TEXT NOT NULL REFERENCES pod_users(id),
    default_channel TEXT,
    member_count    INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
