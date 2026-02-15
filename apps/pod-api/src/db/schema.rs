// @generated automatically by Diesel CLI.

diesel::table! {
    pod_users (id) {
        id -> Text,
        username -> Text,
        display_name -> Text,
        avatar_url -> Nullable<Text>,
        hub_flags -> Int8,
        status -> Text,
        first_seen_at -> Timestamptz,
        last_seen_at -> Timestamptz,
    }
}

diesel::table! {
    communities (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        icon_url -> Nullable<Text>,
        owner_id -> Text,
        default_channel -> Nullable<Text>,
        member_count -> Int4,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    roles (id) {
        id -> Text,
        community_id -> Text,
        name -> Text,
        color -> Nullable<Int4>,
        position -> Int4,
        permissions -> Int8,
        mentionable -> Bool,
        is_default -> Bool,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    channels (id) {
        id -> Text,
        community_id -> Text,
        parent_id -> Nullable<Text>,
        name -> Text,
        topic -> Nullable<Text>,
        #[sql_name = "type"]
        type_ -> Int2,
        position -> Int4,
        slowmode_seconds -> Int4,
        nsfw -> Bool,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        message_count -> Int4,
    }
}

diesel::table! {
    channel_overrides (channel_id, target_type, target_id) {
        channel_id -> Text,
        target_type -> Int2,
        target_id -> Text,
        allow -> Int8,
        deny -> Int8,
    }
}

diesel::table! {
    community_members (community_id, user_id) {
        community_id -> Text,
        user_id -> Text,
        nickname -> Nullable<Text>,
        roles -> Array<Text>,
        joined_at -> Timestamptz,
    }
}

diesel::table! {
    messages (id) {
        id -> Int8,
        channel_id -> Text,
        author_id -> Text,
        content -> Nullable<Text>,
        #[sql_name = "type"]
        type_ -> Int2,
        flags -> Int4,
        reply_to -> Nullable<Int8>,
        edited_at -> Nullable<Timestamptz>,
        pinned -> Bool,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    reactions (message_id, user_id, emoji) {
        message_id -> Int8,
        user_id -> Text,
        emoji -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    invites (code) {
        code -> Text,
        community_id -> Text,
        channel_id -> Nullable<Text>,
        inviter_id -> Text,
        max_uses -> Nullable<Int4>,
        use_count -> Int4,
        max_age_seconds -> Nullable<Int4>,
        created_at -> Timestamptz,
        expires_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    bans (community_id, user_id) {
        community_id -> Text,
        user_id -> Text,
        reason -> Nullable<Text>,
        banned_by -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    read_states (user_id, channel_id) {
        user_id -> Text,
        channel_id -> Text,
        community_id -> Text,
        last_read_id -> Int8,
        mention_count -> Int4,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    audit_log (id) {
        id -> Text,
        community_id -> Text,
        actor_id -> Text,
        action -> Text,
        target_type -> Nullable<Text>,
        target_id -> Nullable<Text>,
        changes -> Nullable<Jsonb>,
        reason -> Nullable<Text>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    pod_roles (id) {
        id -> Text,
        name -> Text,
        position -> Int4,
        permissions -> Int8,
        is_default -> Bool,
        color -> Nullable<Int4>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    pod_member_roles (user_id, role_id) {
        user_id -> Text,
        role_id -> Text,
    }
}

diesel::table! {
    pod_bans (user_id) {
        user_id -> Text,
        reason -> Nullable<Text>,
        banned_by -> Text,
        created_at -> Timestamptz,
    }
}

diesel::joinable!(communities -> pod_users (owner_id));
diesel::joinable!(roles -> communities (community_id));
diesel::joinable!(channels -> communities (community_id));
diesel::joinable!(channel_overrides -> channels (channel_id));
diesel::joinable!(community_members -> communities (community_id));
diesel::joinable!(community_members -> pod_users (user_id));
diesel::joinable!(messages -> channels (channel_id));
diesel::joinable!(messages -> pod_users (author_id));
diesel::joinable!(reactions -> pod_users (user_id));
diesel::joinable!(invites -> communities (community_id));
diesel::joinable!(invites -> channels (channel_id));
diesel::joinable!(bans -> communities (community_id));
diesel::joinable!(read_states -> channels (channel_id));
diesel::joinable!(read_states -> communities (community_id));
diesel::joinable!(read_states -> pod_users (user_id));
diesel::joinable!(pod_member_roles -> pod_roles (role_id));
diesel::joinable!(pod_member_roles -> pod_users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    pod_users,
    communities,
    roles,
    channels,
    channel_overrides,
    community_members,
    messages,
    reactions,
    invites,
    bans,
    read_states,
    audit_log,
    pod_roles,
    pod_member_roles,
    pod_bans,
);
