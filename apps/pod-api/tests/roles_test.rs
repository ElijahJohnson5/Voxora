mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum_test::TestServer;

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:community_id/roles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_roles_is_public() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "role_list").await;

    // No auth header -- should still work.
    let resp = server
        .get(&format!("/api/v1/communities/{community_id}/roles"))
        .await;

    resp.assert_status_ok();
    let roles: Vec<serde_json::Value> = resp.json();
    assert!(!roles.is_empty());
    // Should include @everyone.
    assert!(roles.iter().any(|r| r["name"] == "@everyone"));

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ---------------------------------------------------------------------------
// POST /api/v1/communities/:community_id/roles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_role_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "role_create").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/roles"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Moderator",
            "color": 0xFF0000,
            "permissions": 8,
            "mentionable": true
        }))
        .await;

    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["name"], "Moderator");
    assert_eq!(body["color"], 0xFF0000);
    assert_eq!(body["mentionable"], true);
    assert_eq!(body["is_default"], false);
    assert_eq!(body["position"], 1); // @everyone is 0, new role is 1

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

#[tokio::test]
async fn create_role_requires_manage_roles() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, _owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "role_perm_owner").await;

    // Non-member tries to create role.
    let other_id = voxora_common::id::prefixed_ulid("usr");
    let other_token =
        common::login_test_user(&server, &keys, &state.config, &other_id, "role_outsider").await;

    let resp = server
        .post(&format!("/api/v1/communities/{community_id}/roles"))
        .add_header(AUTHORIZATION, format!("Bearer {other_token}"))
        .json(&serde_json::json!({ "name": "Hacker" }))
        .await;

    resp.assert_status(StatusCode::FORBIDDEN);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &other_id).await;
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/communities/:community_id/roles/:role_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_role_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "role_update").await;

    // Create a role first.
    let create_resp = server
        .post(&format!("/api/v1/communities/{community_id}/roles"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "OldName" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let role_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Update it.
    let resp = server
        .patch(&format!(
            "/api/v1/communities/{community_id}/roles/{role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "NewName", "mentionable": true }))
        .await;

    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["name"], "NewName");
    assert_eq!(body["mentionable"], true);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

#[tokio::test]
async fn update_everyone_name_is_prevented() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "role_everyone").await;

    // Get @everyone role ID.
    let roles_resp = server
        .get(&format!("/api/v1/communities/{community_id}/roles"))
        .await;
    let roles: Vec<serde_json::Value> = roles_resp.json();
    let everyone_id = roles
        .iter()
        .find(|r| r["is_default"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Try to rename @everyone.
    let resp = server
        .patch(&format!(
            "/api/v1/communities/{community_id}/roles/{everyone_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "NotEveryone" }))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:community_id/roles/:role_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_role_succeeds() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "role_delete").await;

    // Create a role.
    let create_resp = server
        .post(&format!("/api/v1/communities/{community_id}/roles"))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": "Temp Role" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let role_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete it.
    let resp = server
        .delete(&format!(
            "/api/v1/communities/{community_id}/roles/{role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone.
    let roles_resp = server
        .get(&format!("/api/v1/communities/{community_id}/roles"))
        .await;
    let roles: Vec<serde_json::Value> = roles_resp.json();
    assert!(!roles.iter().any(|r| r["id"] == role_id.as_str()));

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

#[tokio::test]
async fn delete_everyone_is_prevented() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "role_del_ev").await;

    // Get @everyone role ID.
    let roles_resp = server
        .get(&format!("/api/v1/communities/{community_id}/roles"))
        .await;
    let roles: Vec<serde_json::Value> = roles_resp.json();
    let everyone_id = roles
        .iter()
        .find(|r| r["is_default"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Try to delete @everyone.
    let resp = server
        .delete(&format!(
            "/api/v1/communities/{community_id}/roles/{everyone_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {token}"))
        .await;

    resp.assert_status(StatusCode::BAD_REQUEST);

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
}

#[tokio::test]
async fn delete_role_cleans_up_member_role_arrays() {
    let (app, state, keys) = common::test_app().await;
    let server = TestServer::new(app).unwrap();

    let owner_id = voxora_common::id::prefixed_ulid("usr");
    let (community_id, owner_token) =
        common::setup_community(&server, &keys, &state.config, &owner_id, "role_cleanup_owner").await;

    // Create a role.
    let create_resp = server
        .post(&format!("/api/v1/communities/{community_id}/roles"))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "name": "Cleanup Role" }))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let role_id = create_resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Add a member and assign the role.
    let member_id = voxora_common::id::prefixed_ulid("usr");
    let _member_token = common::join_via_invite(
        &server,
        &keys,
        &state.config,
        &community_id,
        &owner_token,
        &member_id,
        "role_cleanup_mem",
    )
    .await;

    // Assign role to member.
    server
        .patch(&format!(
            "/api/v1/communities/{community_id}/members/{member_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .json(&serde_json::json!({ "roles": [role_id] }))
        .await
        .assert_status_ok();

    // Verify member has the role.
    let member_resp = server
        .get(&format!(
            "/api/v1/communities/{community_id}/members/{member_id}"
        ))
        .await;
    let member: serde_json::Value = member_resp.json();
    assert!(member["roles"].as_array().unwrap().contains(&serde_json::json!(role_id)));

    // Delete the role.
    server
        .delete(&format!(
            "/api/v1/communities/{community_id}/roles/{role_id}"
        ))
        .add_header(AUTHORIZATION, format!("Bearer {owner_token}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    // Verify member no longer has the role.
    let member_resp = server
        .get(&format!(
            "/api/v1/communities/{community_id}/members/{member_id}"
        ))
        .await;
    let member: serde_json::Value = member_resp.json();
    assert!(!member["roles"].as_array().unwrap().contains(&serde_json::json!(role_id)));

    common::cleanup_community(&state.db, &community_id).await;
    common::cleanup_test_user(&state.db, &owner_id).await;
    common::cleanup_test_user(&state.db, &member_id).await;
}
