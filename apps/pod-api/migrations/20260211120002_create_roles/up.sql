CREATE TABLE roles (
    id              TEXT PRIMARY KEY,
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    color           INTEGER,
    position        INTEGER NOT NULL DEFAULT 0,
    permissions     BIGINT NOT NULL DEFAULT 0,
    mentionable     BOOLEAN NOT NULL DEFAULT FALSE,
    is_default      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_roles_community ON roles(community_id);
