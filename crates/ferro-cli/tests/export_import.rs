//! End-to-end export → import round-trip against the fs-json backend.

use std::collections::BTreeMap;

use ferro_cli::{export, import};
use ferro_core::{
    Content, ContentType, FieldDef, FieldId, FieldKind, FieldValue, Locale, Permission, Role,
    RoleId, Scope, Site, SiteSettings, Status, User, UserId,
};
use time::OffsetDateTime;

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
async fn export_then_import_round_trip() {
    let src_tmp = tempfile::tempdir().unwrap();
    let dst_tmp = tempfile::tempdir().unwrap();

    let src_cfg = src_tmp.path().join("ferro.toml");
    let dst_cfg = dst_tmp.path().join("ferro.toml");
    tokio::fs::write(
        &src_cfg,
        ferro_toml(&src_tmp.path().join("data"), &src_tmp.path().join("media")),
    )
    .await
    .unwrap();
    tokio::fs::write(
        &dst_cfg,
        ferro_toml(&dst_tmp.path().join("data"), &dst_tmp.path().join("media")),
    )
    .await
    .unwrap();

    // Seed the source repo directly via ferro-storage.
    let src_cfg_loaded = ferro_cli::config::FerroConfig::load(&src_cfg).await.unwrap();
    let src_repo = ferro_storage::connect(&src_cfg_loaded.storage).await.unwrap();
    src_repo.migrate().await.unwrap();

    let now = OffsetDateTime::now_utc();
    let site = Site {
        id: ferro_core::SiteId::new(),
        slug: "default".into(),
        name: "Default".into(),
        description: Some("test fixture".into()),
        primary_url: None,
        locales: vec![Locale::default()],
        default_locale: Locale::default(),
        settings: SiteSettings::default(),
        created_at: now,
        updated_at: now,
    };
    src_repo.sites().upsert(site.clone()).await.unwrap();

    let ty = ContentType {
        id: ferro_core::ContentTypeId::new(),
        site_id: site.id,
        slug: "post".into(),
        name: "Post".into(),
        description: None,
        fields: vec![FieldDef {
            id: FieldId::new(),
            slug: "title".into(),
            name: "Title".into(),
            help: None,
            kind: FieldKind::Text { multiline: false, max: Some(200) },
            required: true,
            localized: false,
            unique: false,
            hidden: false,
        }],
        singleton: false,
        title_field: Some("title".into()),
        slug_field: None,
        created_at: now,
        updated_at: now,
    };
    src_repo.types().upsert(ty.clone()).await.unwrap();

    let role = Role {
        id: RoleId::new(),
        name: "editor".into(),
        description: None,
        permissions: vec![Permission::Write(Scope::Type { id: ty.id })],
    };
    src_repo.users().upsert_role(role.clone()).await.unwrap();

    let user = User {
        id: UserId::new(),
        email: "admin@example.com".into(),
        handle: "admin".into(),
        display_name: Some("Admin".into()),
        password_hash: None,
        roles: vec![role.id],
        active: true,
        created_at: now,
        last_login: None,
        password_changed_at: None,
        totp_secret: None,
    };
    src_repo.users().upsert(user.clone()).await.unwrap();

    let content = Content {
        id: ferro_core::ContentId::new(),
        site_id: site.id,
        type_id: ty.id,
        slug: "hello".into(),
        locale: Locale::default(),
        status: Status::Published,
        data: {
            let mut m = BTreeMap::new();
            m.insert("title".into(), FieldValue::String("Hello".into()));
            m
        },
        author_id: Some(user.id),
        created_at: now,
        updated_at: now,
        published_at: Some(now),
    };
    src_repo.content().upsert(content.clone()).await.unwrap();

    // Export
    let bundle_path = src_tmp.path().join("bundle.json");
    export::run(export::Args { out: bundle_path.clone(), include_media: false }, src_cfg.clone())
        .await
        .unwrap();
    assert!(bundle_path.exists());

    // Import into destination
    import::run(
        import::Args { bundle: bundle_path.clone(), mode: import::Mode::Merge },
        dst_cfg.clone(),
    )
    .await
    .unwrap();

    // Verify destination
    let dst_cfg_loaded = ferro_cli::config::FerroConfig::load(&dst_cfg).await.unwrap();
    let dst_repo = ferro_storage::connect(&dst_cfg_loaded.storage).await.unwrap();

    let sites = dst_repo.sites().list().await.unwrap();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].id, site.id);

    let types = dst_repo.types().list(site.id).await.unwrap();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].slug, "post");

    let roles = dst_repo.users().list_roles().await.unwrap();
    assert!(roles.iter().any(|r| r.id == role.id));

    let users = dst_repo.users().list().await.unwrap();
    assert!(users.iter().any(|u| u.email == "admin@example.com"));

    let c = dst_repo
        .content()
        .by_slug(site.id, ty.id, "hello")
        .await
        .unwrap()
        .expect("content present");
    assert_eq!(c.id, content.id);
    assert_eq!(c.status, Status::Published);
    match c.data.get("title") {
        Some(FieldValue::String(s)) if s == "Hello" => {}
        other => panic!("expected title=Hello, got {other:?}"),
    }
}

#[tokio::test]
async fn import_replace_mode_wipes_existing() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("ferro.toml");
    tokio::fs::write(&cfg_path, ferro_toml(&tmp.path().join("data"), &tmp.path().join("media")))
        .await
        .unwrap();

    let cfg = ferro_cli::config::FerroConfig::load(&cfg_path).await.unwrap();
    let repo = ferro_storage::connect(&cfg.storage).await.unwrap();
    repo.migrate().await.unwrap();

    let now = OffsetDateTime::now_utc();
    let pre_site = Site {
        id: ferro_core::SiteId::new(),
        slug: "pre".into(),
        name: "Pre-existing".into(),
        description: None,
        primary_url: None,
        locales: vec![Locale::default()],
        default_locale: Locale::default(),
        settings: SiteSettings::default(),
        created_at: now,
        updated_at: now,
    };
    repo.sites().upsert(pre_site.clone()).await.unwrap();
    assert_eq!(repo.sites().list().await.unwrap().len(), 1);

    // Empty bundle → replace should wipe.
    let empty_bundle = serde_json::json!({
        "version": 1,
        "sites": [],
        "types": [],
        "content": [],
        "users": [],
        "roles": [],
        "media": []
    });
    let bundle_path = tmp.path().join("empty.json");
    tokio::fs::write(&bundle_path, serde_json::to_vec_pretty(&empty_bundle).unwrap())
        .await
        .unwrap();

    import::run(import::Args { bundle: bundle_path, mode: import::Mode::Replace }, cfg_path)
        .await
        .unwrap();

    assert_eq!(repo.sites().list().await.unwrap().len(), 0);
}
