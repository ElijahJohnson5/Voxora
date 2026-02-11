// @generated automatically by Diesel CLI.

diesel::table! {
    users (id) {
        id -> Text,
        username -> Text,
        username_lower -> Text,
        display_name -> Text,
        email -> Nullable<Text>,
        email_verified -> Bool,
        password_hash -> Nullable<Text>,
        avatar_url -> Nullable<Text>,
        flags -> Int8,
        status -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    sessions (id) {
        id -> Text,
        user_id -> Text,
        refresh_token -> Text,
        ip_address -> Nullable<Inet>,
        user_agent -> Nullable<Text>,
        last_active_at -> Timestamptz,
        expires_at -> Timestamptz,
        revoked -> Bool,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    pods (id) {
        id -> Text,
        owner_id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        icon_url -> Nullable<Text>,
        url -> Text,
        region -> Nullable<Text>,
        client_id -> Text,
        client_secret -> Text,
        public -> Bool,
        capabilities -> Array<Text>,
        max_members -> Int4,
        version -> Nullable<Text>,
        status -> Text,
        member_count -> Int4,
        online_count -> Int4,
        community_count -> Int4,
        last_heartbeat -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::joinable!(sessions -> users (user_id));
diesel::joinable!(pods -> users (owner_id));

diesel::allow_tables_to_appear_in_same_query!(
    users,
    sessions,
    pods,
);
