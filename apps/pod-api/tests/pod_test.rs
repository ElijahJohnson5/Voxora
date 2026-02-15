mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ===========================================================================
// Pod Roles
// ===========================================================================

// ---------------------------------------------------------------------------
// GET /api/v1/pod/roles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_pod_roles_returns_default_everyone() {
    let (app, _state, _keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/v1/pod/roles").await;
    resp.assert_status_ok();

    let roles: Vec<serde_json::Value> = resp.json();
    assert!(!roles.is_empty());

    // Find the @everyone default role.
    let everyone = roles.iter().find(|r| r["is_default"] == true).unwrap();
    assert_eq!(everyone["name"], "@everyone");
    assert_eq!(everyone["permissions"], 9); // POD_CREATE_COMMUNITY | POD_MANAGE_INVITES
}

// ---------------------------------------------------------------------------
// POST /api/v1/pod/roles — requires POD_MANAGE_ROLES
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_pod_role_requires_pod_manage_roles() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let token = common::login_test_user(&server, &keys, &state.config, &user_id, "pr_noperm").await;

    // Regular user (no POD_MANAGE_ROLES) -> 403.
    let resp = server
        .post("/api/v1/pod/roles")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Test Role" }))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_test_user(&state.db, &user_id).await;
}

#[tokio::test]
async fn create_pod_role_succeeds_for_pod_owner() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    // Use the pod_owner_id from config (set it for this test).
    let owner_id = voxora_common::id::prefixed_ulid("usr");

    // Manually set pod_owner_id for this test's state.
    let mut config = (*state.config).clone();
    config.pod_owner_id = Some(owner_id.clone());
    let state2 = pod_api::AppState {
        config: std::sync::Arc::new(config),
        ..state.clone()
    };
    let app2 = pod_api::routes::router().with_state(state2);
    let server2 = TestServer::new(app2).unwrap();

    let token = common::login_test_user(&server, &keys, &state.config, &owner_id, "pr_owner").await;

    let resp = server2
        .post("/api/v1/pod/roles")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Moderator", "permissions": 4 }))
        .await;

    resp.assert_status(StatusCode::CREATED);
    let role: serde_json::Value = resp.json();
    assert_eq!(role["name"], "Moderator");
    assert_eq!(role["permissions"], 4);
    assert_eq!(role["is_default"], false);

    let role_id = role["id"].as_str().unwrap().to_string();

    common::cleanup_pod_role(&state.db, &role_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/pod/roles/:role_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_pod_role_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let mut config = (*state.config).clone();
    config.pod_owner_id = Some(owner_id.clone());
    let state2 = pod_api::AppState {
        config: std::sync::Arc::new(config),
        ..state.clone()
    };
    let app2 = pod_api::routes::router().with_state(state2);
    let server2 = TestServer::new(app2).unwrap();

    let token = common::login_test_user(&server, &keys, &state.config, &owner_id, "pr_upd").await;

    // Create a role first.
    let create_resp = server2
        .post("/api/v1/pod/roles")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Original" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let role_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Update the role.
    let resp = server2
        .patch(&format!("/api/v1/pod/roles/{role_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Updated", "permissions": 7 }))
        .await;

    resp.assert_status_ok();
    let updated: serde_json::Value = resp.json();
    assert_eq!(updated["name"], "Updated");
    assert_eq!(updated["permissions"], 7);

    common::cleanup_pod_role(&state.db, &role_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

#[tokio::test]
async fn cannot_change_everyone_pod_role_name() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let mut config = (*state.config).clone();
    config.pod_owner_id = Some(owner_id.clone());
    let state2 = pod_api::AppState {
        config: std::sync::Arc::new(config),
        ..state.clone()
    };
    let app2 = pod_api::routes::router().with_state(state2);
    let server2 = TestServer::new(app2).unwrap();

    let token = common::login_test_user(&server, &keys, &state.config, &owner_id, "pr_evname").await;

    let resp = server2
        .patch("/api/v1/pod/roles/pod_role_everyone")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Not Everyone" }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/pod/roles/:role_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_pod_role_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let mut config = (*state.config).clone();
    config.pod_owner_id = Some(owner_id.clone());
    let state2 = pod_api::AppState {
        config: std::sync::Arc::new(config),
        ..state.clone()
    };
    let app2 = pod_api::routes::router().with_state(state2);
    let server2 = TestServer::new(app2).unwrap();

    let token = common::login_test_user(&server, &keys, &state.config, &owner_id, "pr_del").await;

    // Create a role.
    let create_resp = server2
        .post("/api/v1/pod/roles")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Temp Role" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let role_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete it.
    let resp = server2
        .delete(&format!("/api/v1/pod/roles/{role_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone.
    let list_resp = server2.get("/api/v1/pod/roles").await;
    let roles: Vec<serde_json::Value> = list_resp.json();
    assert!(!roles.iter().any(|r| r["id"] == role_id.as_str()));

    common::cleanup_test_user(&state.db, &owner_id).await;
}

#[tokio::test]
async fn cannot_delete_everyone_pod_role() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let mut config = (*state.config).clone();
    config.pod_owner_id = Some(owner_id.clone());
    let state2 = pod_api::AppState {
        config: std::sync::Arc::new(config),
        ..state.clone()
    };
    let app2 = pod_api::routes::router().with_state(state2);
    let server2 = TestServer::new(app2).unwrap();

    let token = common::login_test_user(&server, &keys, &state.config, &owner_id, "pr_evdel").await;

    let resp = server2
        .delete("/api/v1/pod/roles/pod_role_everyone")
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ===========================================================================
// Pod Member Role Assignment
// ===========================================================================

#[tokio::test]
async fn assign_and_unassign_pod_role() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let member_id = voxora_common::id::prefixed_ulid("usr");

    let mut config = (*state.config).clone();
    config.pod_owner_id = Some(owner_id.clone());
    let state2 = pod_api::AppState {
        config: std::sync::Arc::new(config),
        ..state.clone()
    };
    let app2 = pod_api::routes::router().with_state(state2);
    let server2 = TestServer::new(app2).unwrap();

    let owner_token = common::login_test_user(&server, &keys, &state.config, &owner_id, "pmr_own").await;
    let _member_token = common::login_test_user(&server, &keys, &state.config, &member_id, "pmr_mem").await;

    // Create a pod role.
    let role_resp = server2
        .post("/api/v1/pod/roles")
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "name": "Admin", "permissions": 4 }))
        .await;
    role_resp.assert_status(StatusCode::CREATED);
    let role_id = role_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Assign role to member.
    let assign_resp = server2
        .put(&format!("/api/v1/pod/members/{member_id}/roles/{role_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    assign_resp.assert_status(StatusCode::NO_CONTENT);

    // Assign again (idempotent).
    let assign_resp2 = server2
        .put(&format!("/api/v1/pod/members/{member_id}/roles/{role_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    assign_resp2.assert_status(StatusCode::NO_CONTENT);

    // Unassign.
    let unassign_resp = server2
        .delete(&format!("/api/v1/pod/members/{member_id}/roles/{role_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    unassign_resp.assert_status(StatusCode::NO_CONTENT);

    // Unassign again -> 404.
    let unassign_resp2 = server2
        .delete(&format!("/api/v1/pod/members/{member_id}/roles/{role_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    unassign_resp2.assert_status(StatusCode::NOT_FOUND);

    common::cleanup_pod_role(&state.db, &role_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}

// ===========================================================================
// Pod Bans
// ===========================================================================

#[tokio::test]
async fn pod_ban_succeeds_and_blocks_login() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let victim_id = voxora_common::id::prefixed_ulid("usr");

    let mut config = (*state.config).clone();
    config.pod_owner_id = Some(owner_id.clone());
    let state2 = pod_api::AppState {
        config: std::sync::Arc::new(config.clone()),
        ..state.clone()
    };
    let app2 = pod_api::routes::router().with_state(state2);
    let server2 = TestServer::new(app2).unwrap();

    let owner_token = common::login_test_user(&server, &keys, &state.config, &owner_id, "pb_own").await;
    let _victim_token = common::login_test_user(&server, &keys, &state.config, &victim_id, "pb_vic").await;

    // Ban user.
    let ban_resp = server2
        .put(&format!("/api/v1/pod/bans/{victim_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "reason": "Spamming" }))
        .await;
    ban_resp.assert_status_ok();
    let ban: serde_json::Value = ban_resp.json();
    assert_eq!(ban["user_id"], victim_id.as_str());
    assert_eq!(ban["reason"], "Spamming");

    // Banned user trying to login -> 403.
    let sia = common::mint_test_sia(
        &keys,
        &config.hub_url,
        &victim_id,
        &config.pod_id,
        "pb_vic",
        "pb_vic",
    );
    let login_resp = server2
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia }))
        .await;
    login_resp.assert_status(StatusCode::FORBIDDEN);

    // List bans.
    let list_resp = server2
        .get("/api/v1/pod/bans")
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    list_resp.assert_status_ok();
    let bans: Vec<serde_json::Value> = list_resp.json();
    assert!(bans.iter().any(|b| b["user_id"] == victim_id.as_str()));

    // Unban.
    let unban_resp = server2
        .delete(&format!("/api/v1/pod/bans/{victim_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await;
    unban_resp.assert_status(StatusCode::NO_CONTENT);

    // Login should work again.
    let sia2 = common::mint_test_sia(
        &keys,
        &config.hub_url,
        &victim_id,
        &config.pod_id,
        "pb_vic",
        "pb_vic",
    );
    let login_resp2 = server2
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "sia": sia2 }))
        .await;
    login_resp2.assert_status_ok();

    common::cleanup_pod_ban(&state.db, &victim_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &victim_id).await;
}

#[tokio::test]
async fn cannot_pod_ban_self() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let mut config = (*state.config).clone();
    config.pod_owner_id = Some(owner_id.clone());
    let state2 = pod_api::AppState {
        config: std::sync::Arc::new(config),
        ..state.clone()
    };
    let app2 = pod_api::routes::router().with_state(state2);
    let server2 = TestServer::new(app2).unwrap();

    let owner_token = common::login_test_user(&server, &keys, &state.config, &owner_id, "pb_self").await;

    let resp = server2
        .put(&format!("/api/v1/pod/bans/{owner_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_test_user(&state.db, &owner_id).await;
}

#[tokio::test]
async fn pod_ban_requires_permission() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let user_id = voxora_common::id::prefixed_ulid("usr");
    let target_id = voxora_common::id::prefixed_ulid("usr");
    let token = common::login_test_user(&server, &keys, &state.config, &user_id, "pb_noperm").await;
    let _target_token = common::login_test_user(&server, &keys, &state.config, &target_id, "pb_target").await;

    // Regular user (no POD_BAN_MEMBERS) -> 403.
    let resp = server
        .put(&format!("/api/v1/pod/bans/{target_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({}))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_test_user(&state.db, &user_id).await;
    common::cleanup_test_user(&state.db, &target_id).await;
}

#[tokio::test]
async fn pod_ban_removes_community_memberships() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let victim_id = voxora_common::id::prefixed_ulid("usr");

    let mut config = (*state.config).clone();
    config.pod_owner_id = Some(owner_id.clone());
    let state2 = pod_api::AppState {
        config: std::sync::Arc::new(config),
        ..state.clone()
    };
    let app2 = pod_api::routes::router().with_state(state2);
    let server2 = TestServer::new(app2).unwrap();

    let owner_token = common::login_test_user(&server, &keys, &state.config, &owner_id, "pb_cm_own").await;

    // Create a community.
    let (community_id, _) = common::setup_community(&server2, &keys, &state.config, &owner_id, "pb_cm_own").await;

    // Add victim as member.
    let _victim_token = common::join_via_invite(
        &server2,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &victim_id,
        "pb_cm_vic",
    )
    .await;

    // Verify victim is a member (member_count = 2).
    let community_resp = server2.get(&format!("/api/v1/communities/{community_id}")).await;
    let community: serde_json::Value = community_resp.json();
    assert_eq!(community["member_count"], 2);

    // Pod ban victim.
    let ban_resp = server2
        .put(&format!("/api/v1/pod/bans/{victim_id}"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({}))
        .await;
    ban_resp.assert_status_ok();

    // Verify victim is no longer a member (member_count = 1).
    let community_resp2 = server2.get(&format!("/api/v1/communities/{community_id}")).await;
    let community2: serde_json::Value = community_resp2.json();
    assert_eq!(community2["member_count"], 1);

    common::cleanup_pod_ban(&state.db, &victim_id).await;
    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &victim_id).await;
}
