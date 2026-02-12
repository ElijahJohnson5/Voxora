CREATE TABLE pod_users (
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    avatar_url      TEXT,
    hub_flags       BIGINT NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'active',
    first_seen_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
