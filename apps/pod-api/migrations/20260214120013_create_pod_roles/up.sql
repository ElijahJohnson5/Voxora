CREATE TABLE pod_roles (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    position        INTEGER NOT NULL DEFAULT 0,
    permissions     BIGINT NOT NULL DEFAULT 0,
    is_default      BOOLEAN NOT NULL DEFAULT FALSE,
    color           INTEGER,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
