CREATE TABLE user_pod_bookmarks (
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pod_id      TEXT NOT NULL REFERENCES pods(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, pod_id)
);

CREATE INDEX idx_bookmarks_user ON user_pod_bookmarks(user_id);
CREATE INDEX idx_bookmarks_pod ON user_pod_bookmarks(pod_id);
