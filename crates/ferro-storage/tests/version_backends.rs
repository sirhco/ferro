//! ContentVersionRepo round-trip on the surreal backend's in-process engine.

#![cfg(feature = "surreal")]

use std::collections::BTreeMap;

use ferro_core::{
    Content, ContentId, ContentTypeId, ContentVersion, FieldValue, Locale, SiteId, Status,
};
use ferro_storage::StorageConfig;
use time::OffsetDateTime;

async fn repo() -> (tempfile::TempDir, Box<dyn ferro_storage::Repository>) {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = StorageConfig::SurrealEmbedded {
        path: tmp.path().to_path_buf(),
        namespace: "ferro_test".into(),
        database: "main".into(),
    };
    let repo = ferro_storage::connect(&cfg).await.unwrap();
    repo.migrate().await.unwrap();
    (tmp, repo)
}

#[tokio::test]
async fn surreal_version_create_list_get_round_trip() {
    let (_tmp, repo) = repo().await;

    let now = OffsetDateTime::now_utc();
    let mut data = BTreeMap::new();
    data.insert("title".into(), FieldValue::String("snap".into()));
    let template = Content {
        id: ContentId::new(),
        site_id: SiteId::new(),
        type_id: ContentTypeId::new(),
        slug: "alpha".into(),
        locale: Locale::default(),
        status: Status::Draft,
        data: data.clone(),
        author_id: None,
        created_at: now,
        updated_at: now,
        published_at: None,
    };

    // Create three versions for the same content_id.
    let v1 = ContentVersion::from_content(&template, None, None);
    let v2 = ContentVersion::from_content(&template, None, Some(v1.id));
    let v3 = ContentVersion::from_content(&template, None, Some(v2.id));
    let v1 = repo.versions().create(v1).await.unwrap();
    let v2 = repo.versions().create(v2).await.unwrap();
    let v3 = repo.versions().create(v3).await.unwrap();

    let listed = repo.versions().list(template.id).await.unwrap();
    assert_eq!(listed.len(), 3);
    let ids: Vec<_> = listed.iter().map(|v| v.id).collect();
    assert!(ids.contains(&v1.id));
    assert!(ids.contains(&v2.id));
    assert!(ids.contains(&v3.id));

    let fetched = repo.versions().get(v2.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, v2.id);
    assert_eq!(fetched.parent_version, Some(v1.id));

    // Different content_id has empty list.
    let empty = repo.versions().list(ContentId::new()).await.unwrap();
    assert!(empty.is_empty());
}
