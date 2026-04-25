//! End-to-end test for the `ferro admin` subcommand. Exercises bootstrap path:
//! seed admin role, create user, grant role, login.

use ferro_cli::{admin, config::FerroConfig};
use ferro_core::Permission;

fn ferro_toml(data_dir: &std::path::Path, media_dir: &std::path::Path) -> String {
    format!(
        r#"[server]
bind = "127.0.0.1:0"
public_url = "http://localhost:0"
admin_enabled = false

[storage]
kind = "fs-json"
path = "{}"

[media]
kind = "local"
path = "{}"
base_url = "http://localhost/media"

[auth]
session_secret = "CHANGE_ME_test_0000000000000000"
jwt_issuer = "ferro-test"
allow_public_signup = false

[plugins]
dir = "{}"
max_memory_mb = 128
fuel_per_request = 10000000
"#,
        data_dir.display(),
        media_dir.display(),
        media_dir.display(),
    )
}

#[tokio::test]
async fn admin_cli_bootstraps_first_admin() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("ferro.toml");
    tokio::fs::write(
        &cfg_path,
        ferro_toml(&tmp.path().join("data"), &tmp.path().join("media")),
    )
    .await
    .unwrap();

    // Single-shot bootstrap: --with-admin auto-seeds the role and grants it.
    admin::run(
        admin::Cmd::CreateUser(admin::CreateUserArgs {
            email: "root@example.com".into(),
            handle: "root".into(),
            password: "correct-horse-battery-staple".into(),
            display_name: Some("Root".into()),
            role: Vec::new(),
            inactive: false,
            with_admin: true,
        }),
        cfg_path.clone(),
    )
    .await
    .unwrap();

    // Verify directly via repo.
    let cfg = FerroConfig::load(&cfg_path).await.unwrap();
    let repo = ferro_storage::connect(&cfg.storage).await.unwrap();
    let user = repo
        .users()
        .by_email("root@example.com")
        .await
        .unwrap()
        .expect("user persisted");
    assert!(user.active);
    assert_eq!(user.roles.len(), 1, "one role attached");
    assert!(user.password_hash.is_some(), "hash persisted");

    // Role exists with Permission::Admin.
    let role_id = user.roles[0];
    let role = repo.users().get_role(role_id).await.unwrap().unwrap();
    assert_eq!(role.name, "admin");
    assert!(matches!(role.permissions[0], Permission::Admin));

    // Idempotency: running --with-admin again on a different user reuses the role.
    admin::run(
        admin::Cmd::CreateUser(admin::CreateUserArgs {
            email: "second@example.com".into(),
            handle: "second".into(),
            password: "another-strong-password".into(),
            display_name: None,
            role: Vec::new(),
            inactive: false,
            with_admin: true,
        }),
        cfg_path.clone(),
    )
    .await
    .unwrap();

    let roles_after = repo.users().list_roles().await.unwrap();
    assert_eq!(roles_after.len(), 1, "admin role not duplicated");

    // List sanity-check.
    admin::run(admin::Cmd::ListUsers, cfg_path.clone()).await.unwrap();
    admin::run(admin::Cmd::ListRoles, cfg_path).await.unwrap();
}

#[tokio::test]
async fn admin_cli_grants_existing_role() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("ferro.toml");
    tokio::fs::write(
        &cfg_path,
        ferro_toml(&tmp.path().join("data"), &tmp.path().join("media")),
    )
    .await
    .unwrap();

    // Create a role + user, then grant by name.
    admin::run(
        admin::Cmd::CreateRole(admin::CreateRoleArgs {
            name: "reader".into(),
            description: None,
            preset: admin::Preset::Reader,
        }),
        cfg_path.clone(),
    )
    .await
    .unwrap();

    admin::run(
        admin::Cmd::CreateUser(admin::CreateUserArgs {
            email: "viewer@example.com".into(),
            handle: "viewer".into(),
            password: "viewer-password".into(),
            display_name: None,
            role: Vec::new(),
            inactive: false,
            with_admin: false,
        }),
        cfg_path.clone(),
    )
    .await
    .unwrap();

    admin::run(
        admin::Cmd::GrantRole(admin::GrantRoleArgs {
            user: "viewer@example.com".into(),
            role: "reader".into(),
        }),
        cfg_path.clone(),
    )
    .await
    .unwrap();

    let cfg = FerroConfig::load(&cfg_path).await.unwrap();
    let repo = ferro_storage::connect(&cfg.storage).await.unwrap();
    let user = repo
        .users()
        .by_email("viewer@example.com")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.roles.len(), 1);

    // Grant again — should still be 1 (idempotent).
    admin::run(
        admin::Cmd::GrantRole(admin::GrantRoleArgs {
            user: "viewer@example.com".into(),
            role: "reader".into(),
        }),
        cfg_path,
    )
    .await
    .unwrap();
    let user2 = repo
        .users()
        .by_email("viewer@example.com")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user2.roles.len(), 1, "duplicate role should be deduped");
}
