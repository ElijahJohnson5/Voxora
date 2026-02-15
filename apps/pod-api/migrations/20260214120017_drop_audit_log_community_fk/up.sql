-- Drop FK on audit_log.community_id so pod-level audit entries can use "pod" as sentinel.
ALTER TABLE audit_log DROP CONSTRAINT audit_log_community_id_fkey;
