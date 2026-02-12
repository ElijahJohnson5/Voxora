CREATE TABLE channel_overrides (
    channel_id      TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    target_type     SMALLINT NOT NULL,
    target_id       TEXT NOT NULL,
    allow           BIGINT NOT NULL DEFAULT 0,
    deny            BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, target_type, target_id)
);
