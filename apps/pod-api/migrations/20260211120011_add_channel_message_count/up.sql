ALTER TABLE channels ADD COLUMN message_count INTEGER NOT NULL DEFAULT 0;

-- Backfill existing counts
UPDATE channels SET message_count = (
    SELECT COUNT(*) FROM messages WHERE messages.channel_id = channels.id
);
