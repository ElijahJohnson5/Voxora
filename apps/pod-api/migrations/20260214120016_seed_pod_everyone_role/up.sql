-- Seed the default @everyone pod role with POD_CREATE_COMMUNITY (1) | POD_MANAGE_INVITES (8) = 9
INSERT INTO pod_roles (id, name, position, permissions, is_default)
VALUES ('pod_role_everyone', '@everyone', 0, 9, TRUE)
ON CONFLICT DO NOTHING;
