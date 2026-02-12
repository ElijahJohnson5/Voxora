CREATE TABLE community_members (
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    nickname        TEXT,
    roles           TEXT[] NOT NULL DEFAULT '{}',
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (community_id, user_id)
);
CREATE INDEX idx_members_user ON community_members(user_id);
