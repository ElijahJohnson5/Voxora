CREATE TABLE reactions (
    message_id      BIGINT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    emoji           TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, user_id, emoji)
);
