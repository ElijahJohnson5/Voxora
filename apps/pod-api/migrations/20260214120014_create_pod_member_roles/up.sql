CREATE TABLE pod_member_roles (
    user_id TEXT NOT NULL REFERENCES pod_users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES pod_roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);
CREATE INDEX idx_pod_member_roles_user ON pod_member_roles(user_id);
