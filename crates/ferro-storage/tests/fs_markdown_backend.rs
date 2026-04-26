//! fs-markdown backend round-trip.

#![cfg(feature = "fs-markdown")]

use std::collections::BTreeMap;

use ferro_core::{
    Content, ContentId, ContentQuery, ContentType, ContentTypeId, ContentVersion,
    ContentVersionId, FieldDef, FieldId, FieldKind, FieldValue, Locale, NewContent, Site, SiteId,
    SiteSettings, Status, User, UserId,
};
use ferro_storage::StorageConfig;
use time::OffsetDateTime;

async fn repo() -> (tempfile::TempDir, Box<dyn ferro_storage::Repository>) {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = StorageConfig::FsMarkdown { path: tmp.path().to_path_buf() };
    let repo = ferro_storage::connect(&cfg).await.unwrap();
    repo.migrate().await.unwrap();
    (tmp, repo)
}

#[tokio::test]
async fn site_type_content_round_trip() {
    let (_tmp, repo) = repo().await;

    let now = OffsetDateTime::now_utc();
    let site = Site {
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
    };
    repo.sites().upsert(site.clone()).await.unwrap();

    let ty = ContentType {
        id: ContentTypeId::new(),
        site_id: site.id,
        slug: "post".into(),
        name: "Post".into(),
        description: None,
        fields: vec![
            FieldDef {
                id: FieldId::new(),
                slug: "title".into(),
                name: "Title".into(),
                help: None,
                kind: FieldKind::Text { multiline: false, max: None },
                required: true,
                localized: false,
                unique: false,
                hidden: false,
            },
            FieldDef {
                id: FieldId::new(),
                slug: "body".into(),
                name: "Body".into(),
                help: None,
                kind: FieldKind::RichText { format: ferro_core::RichFormat::Markdown },
                required: false,
                localized: false,
                unique: false,
                hidden: false,
            },
        ],
        singleton: false,
        title_field: Some("title".into()),
        slug_field: None,
        created_at: now,
        updated_at: now,
    };
    repo.types().upsert(ty.clone()).await.unwrap();

    // Create content with a `body` markdown field.
    let mut data = BTreeMap::new();
    data.insert("title".into(), FieldValue::String("Hello".into()));
    data.insert(
        "body".into(),
        FieldValue::String("# Heading\n\nSome body text.".into()),
    );
    let new = NewContent {
        type_id: ty.id,
        slug: "hello".into(),
        locale: Locale::default(),
        data,
        author_id: None,
    };
    let created = repo.content().create(site.id, new).await.unwrap();
    assert_eq!(created.status, Status::Draft);

    // Verify on-disk path uses site/type slugs.
    let p = _tmp_path(&_tmp).join("default/post/hello.en.md");
    assert!(p.exists(), "expected {p:?} to exist");
    let raw = std::fs::read_to_string(&p).unwrap();
    assert!(raw.starts_with("---\n"));
    assert!(raw.contains("# Heading"), "body should appear after front-matter: {raw}");

    // by_slug round-trip
    let fetched = repo
        .content()
        .by_slug(site.id, ty.id, "hello")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.id, created.id);
    match fetched.data.get("title") {
        Some(FieldValue::String(s)) if s == "Hello" => {}
        other => panic!("title roundtrip: {other:?}"),
    }
    match fetched.data.get("body") {
        Some(FieldValue::String(s)) if s.contains("# Heading") => {}
        other => panic!("body roundtrip: {other:?}"),
    }

    // Publish
    let published = repo.content().publish(created.id).await.unwrap();
    assert_eq!(published.status, Status::Published);

    // List with search filter
    let page = repo
        .content()
        .list(ContentQuery {
            site_id: Some(site.id),
            type_id: Some(ty.id),
            search: Some("Heading".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);

    // Delete
    repo.content().delete(created.id).await.unwrap();
    assert!(repo.content().get(created.id).await.unwrap().is_none());
}

#[tokio::test]
async fn user_role_round_trip() {
    let (_tmp, repo) = repo().await;
    let role = ferro_core::Role {
        id: ferro_core::RoleId::new(),
        name: "viewer".into(),
        description: None,
        permissions: Vec::new(),
    };
    repo.users().upsert_role(role.clone()).await.unwrap();
    assert_eq!(repo.users().list_roles().await.unwrap().len(), 1);

    let user = User {
        id: UserId::new(),
        email: "v@example.com".into(),
        handle: "v".into(),
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
    let by_email = repo.users().by_email("v@example.com").await.unwrap().unwrap();
    assert_eq!(by_email.id, user.id);
    assert_eq!(by_email.password_hash.as_deref(), Some("hash"));
}

#[tokio::test]
async fn missing_get_returns_none() {
    let (_tmp, repo) = repo().await;
    assert!(repo.content().get(ContentId::new()).await.unwrap().is_none());
}

#[tokio::test]
async fn version_snapshots_round_trip() {
    let (_tmp, repo) = repo().await;
    let now = OffsetDateTime::now_utc();
    let content_id = ContentId::new();
    let site_id = SiteId::new();
    let type_id = ContentTypeId::new();

    // Create two snapshots a tick apart so most-recent-first is unambiguous.
    let v1 = ContentVersion {
        id: ContentVersionId::new(),
        content_id,
        site_id,
        type_id,
        slug: "alpha".into(),
        locale: Locale::default(),
        status: Status::Draft,
        data: BTreeMap::from([("title".into(), FieldValue::String("v1".into()))]),
        author_id: None,
        captured_at: now,
        parent_version: None,
    };
    let v2 = ContentVersion {
        id: ContentVersionId::new(),
        content_id,
        site_id,
        type_id,
        slug: "alpha".into(),
        locale: Locale::default(),
        status: Status::Draft,
        data: BTreeMap::from([("title".into(), FieldValue::String("v2".into()))]),
        author_id: None,
        captured_at: now + time::Duration::seconds(1),
        parent_version: Some(v1.id),
    };
    repo.versions().create(v1.clone()).await.unwrap();
    repo.versions().create(v2.clone()).await.unwrap();

    let listed = repo.versions().list(content_id).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, v2.id, "most-recent first");
    assert_eq!(listed[1].id, v1.id);

    let fetched = repo.versions().get(v1.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, v1.id);

    // Sanity: unrelated id misses.
    assert!(repo.versions().get(ContentVersionId::new()).await.unwrap().is_none());

    // Other content id sees nothing.
    assert!(repo.versions().list(ContentId::new()).await.unwrap().is_empty());

    // Force `Content` import to be used (silences unused warning if linter
    // ever flips strict mode here).
    let _: Option<Content> = None;
}

fn _tmp_path(tmp: &tempfile::TempDir) -> std::path::PathBuf {
    tmp.path().to_path_buf()
}
