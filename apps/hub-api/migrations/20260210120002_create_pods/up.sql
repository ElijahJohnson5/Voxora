CREATE TABLE pods (
    id              TEXT PRIMARY KEY,
    owner_id        TEXT NOT NULL REFERENCES users(id),
    name            TEXT NOT NULL,
    description     TEXT,
    icon_url        TEXT,
    url             TEXT NOT NULL,
    region          TEXT,
    client_id       TEXT NOT NULL UNIQUE,
    client_secret   TEXT NOT NULL,
    public          BOOLEAN NOT NULL DEFAULT TRUE,
    capabilities    TEXT[] NOT NULL DEFAULT '{"text"}',
    max_members     INTEGER NOT NULL DEFAULT 10000,
    version         TEXT,
    status          TEXT NOT NULL DEFAULT 'active',
    member_count    INTEGER NOT NULL DEFAULT 0,
    online_count    INTEGER NOT NULL DEFAULT 0,
    community_count INTEGER NOT NULL DEFAULT 0,
    last_heartbeat  TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pods_owner ON pods(owner_id);
CREATE INDEX idx_pods_status ON pods(status);
