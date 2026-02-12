CREATE TABLE messages (
    id              BIGINT PRIMARY KEY,
    channel_id      TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_id       TEXT NOT NULL REFERENCES pod_users(id),
    content         TEXT,
    type            SMALLINT NOT NULL DEFAULT 0,
    flags           INTEGER NOT NULL DEFAULT 0,
    reply_to        BIGINT,
    edited_at       TIMESTAMPTZ,
    pinned          BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_messages_channel ON messages(channel_id, id DESC);
CREATE INDEX idx_messages_author ON messages(author_id);
