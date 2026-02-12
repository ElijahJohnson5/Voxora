CREATE TABLE audit_log (
    id              TEXT PRIMARY KEY,
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    actor_id        TEXT NOT NULL REFERENCES pod_users(id),
    action          TEXT NOT NULL,
    target_type     TEXT,
    target_id       TEXT,
    changes         JSONB,
    reason          TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_audit_community ON audit_log(community_id, created_at DESC);
