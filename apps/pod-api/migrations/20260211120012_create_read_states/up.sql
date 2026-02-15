CREATE TABLE read_states (
    user_id       TEXT NOT NULL REFERENCES pod_users(id),
    channel_id    TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    community_id  TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    last_read_id  BIGINT NOT NULL DEFAULT 0,
    mention_count INTEGER NOT NULL DEFAULT 0,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, channel_id)
);

CREATE INDEX idx_read_states_user ON read_states (user_id);
