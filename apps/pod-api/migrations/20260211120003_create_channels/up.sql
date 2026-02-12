CREATE TABLE channels (
    id              TEXT PRIMARY KEY,
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    parent_id       TEXT REFERENCES channels(id),
    name            TEXT NOT NULL,
    topic           TEXT,
    type            SMALLINT NOT NULL DEFAULT 0,
    position        INTEGER NOT NULL DEFAULT 0,
    slowmode_seconds INTEGER NOT NULL DEFAULT 0,
    nsfw            BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_channels_community ON channels(community_id);
