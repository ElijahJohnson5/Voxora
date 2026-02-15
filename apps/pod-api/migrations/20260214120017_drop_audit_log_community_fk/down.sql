ALTER TABLE audit_log ADD CONSTRAINT audit_log_community_id_fkey
    FOREIGN KEY (community_id) REFERENCES communities(id) ON DELETE CASCADE;
