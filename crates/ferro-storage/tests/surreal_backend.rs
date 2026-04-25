//! SurrealDB backend round-trip tests against an in-memory engine.

#![cfg(feature = "surreal")]

use std::collections::BTreeMap;

use ferro_core::{
    Content, ContentId, ContentQuery, ContentType, ContentTypeId, FieldDef, FieldId, FieldKind,
    FieldValue, Locale, NewContent, Role, RoleId, Site, SiteId, SiteSettings, Status, User, UserId,
};
use ferro_storage::{StorageConfig, StorageError};
use time::OffsetDateTime;

fn site_fixture() -> Site {
    let now = OffsetDateTime::now_utc();
    Site {
        id: SiteId::new(),
        slug: "default".into(),
        name: "Default".into(),
        description: None,
        primary_url: None,
        locales: vec![Locale::default()],
        default_locale: Locale::default(),
        settings: SiteSettings::default(),
        created_at: now,
        updated_at: now,
    }
}

fn type_fixture(site: SiteId) -> ContentType {
    let now = OffsetDateTime::now_utc();
    ContentType {
        id: ContentTypeId::new(),
        site_id: site,
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
    }
}

async fn mem_repo() -> (tempfile::TempDir, Box<dyn ferro_storage::Repository>) {
    // Each test gets its own RocksDB directory so parallel runs don't fight
    // for the file lock. SurrealDB's `mem://` engine is single-process global,
    // so it can't be sharded across `#[tokio::test]` cases.
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = StorageConfig::SurrealEmbedded {
        path: tmp.path().to_path_buf(),
        namespace: "ferro_test".into(),
        database: "main".into(),
    };
    let repo = ferro_storage::connect(&cfg).await.expect("connect surreal");
    repo.migrate().await.expect("migrate");
    (tmp, repo)
}

#[tokio::test]
async fn site_round_trip() {
    let (_tmp, repo) = mem_repo().await;
    let site = site_fixture();
    let saved = repo.sites().upsert(site.clone()).await.unwrap();
    assert_eq!(saved.id, site.id);

    let by_id = repo.sites().get(site.id).await.unwrap().unwrap();
    assert_eq!(by_id.slug, "default");
    let by_slug = repo.sites().by_slug("default").await.unwrap().unwrap();
    assert_eq!(by_slug.id, site.id);

    let listed = repo.sites().list().await.unwrap();
    assert_eq!(listed.len(), 1);

    repo.sites().delete(site.id).await.unwrap();
    assert!(repo.sites().get(site.id).await.unwrap().is_none());
}

#[tokio::test]
async fn type_and_content_round_trip() {
    let (_tmp, repo) = mem_repo().await;
    let site = site_fixture();
    repo.sites().upsert(site.clone()).await.unwrap();

    let ty = type_fixture(site.id);
    repo.types().upsert(ty.clone()).await.unwrap();

    let by_slug = repo.types().by_slug(site.id, "post").await.unwrap().unwrap();
    assert_eq!(by_slug.id, ty.id);

    let mut data = BTreeMap::new();
    data.insert("title".into(), FieldValue::String("Hello".into()));
    let new = NewContent {
        type_id: ty.id,
        slug: "hello".into(),
        locale: Locale::default(),
        data: data.clone(),
        author_id: None,
    };
    let created = repo.content().create(site.id, new).await.unwrap();
    assert_eq!(created.status, Status::Draft);

    let fetched = repo
        .content()
        .by_slug(site.id, ty.id, "hello")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.id, created.id);

    // Publish
    let published = repo.content().publish(created.id).await.unwrap();
    assert_eq!(published.status, Status::Published);
    assert!(published.published_at.is_some());

    // List with filter
    let page = repo
        .content()
        .list(ContentQuery {
            site_id: Some(site.id),
            type_id: Some(ty.id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total, 1);

    // Update via patch
    let mut new_data = BTreeMap::new();
    new_data.insert("title".into(), FieldValue::String("Hello v2".into()));
    let updated = repo
        .content()
        .update(
            created.id,
            ferro_core::ContentPatch {
                slug: None,
                status: None,
                data: Some(new_data),
            },
        )
        .await
        .unwrap();
    match updated.data.get("title") {
        Some(FieldValue::String(s)) if s == "Hello v2" => {}
        other => panic!("unexpected title: {other:?}"),
    }

    repo.content().delete(created.id).await.unwrap();
    assert!(repo.content().get(created.id).await.unwrap().is_none());
}

#[tokio::test]
async fn user_role_round_trip() {
    let (_tmp, repo) = mem_repo().await;

    let role = Role {
        id: RoleId::new(),
        name: "editor".into(),
        description: None,
        permissions: Vec::new(),
    };
    repo.users().upsert_role(role.clone()).await.unwrap();
    assert!(repo.users().get_role(role.id).await.unwrap().is_some());

    let user = User {
        id: UserId::new(),
        email: "Admin@Example.com".into(),
        handle: "admin".into(),
        display_name: None,
        password_hash: Some("hash".into()),
        roles: vec![role.id],
        active: true,
        created_at: OffsetDateTime::now_utc(),
        last_login: None,
        password_changed_at: None,
        totp_secret: None,
    };
    repo.users().upsert(user.clone()).await.unwrap();

    let by_email = repo.users().by_email("admin@example.com").await.unwrap().unwrap();
    assert_eq!(by_email.id, user.id);
    assert_eq!(by_email.password_hash.as_deref(), Some("hash"));
}

#[tokio::test]
async fn missing_record_get_returns_none() {
    let (_tmp, repo) = mem_repo().await;
    let out = repo.content().get(ContentId::new()).await.unwrap();
    assert!(out.is_none());
}

#[tokio::test]
async fn update_missing_returns_not_found() {
    let (_tmp, repo) = mem_repo().await;
    let out = repo
        .content()
        .update(ContentId::new(), ferro_core::ContentPatch::default())
        .await;
    assert!(matches!(out, Err(StorageError::NotFound)));
}
