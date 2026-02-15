CREATE TABLE user_preferences (
    user_id         TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    preferred_pods  TEXT[] NOT NULL DEFAULT '{}',
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
