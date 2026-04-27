//! Schema evolution migrator.
//!
//! Applies [`FieldChange`]s to every content row of a given `ContentTypeId`
//! so `content.data` stays aligned with the current `ContentType.fields`.
//!
//! Designed for admin-triggered migrations: callers diff the new and old
//! `ContentType`, persist the new type, then invoke [`apply_changes`] to
//! rewrite existing rows.

use ferro_core::{ContentQuery, ContentTypeId, FieldChange, FieldValue, SiteId};

use crate::{error::StorageResult, repo::Repository};

/// Apply a list of field changes to every content row with the given
/// `(site, type_id)`. Returns the number of rows touched.
///
/// Each change semantically maps to:
/// - `Added(slug)`   — inserts `slug -> FieldValue::Null` if missing.
/// - `Removed(slug)` — drops the key from `data`.
/// - `Renamed{from,to}` — moves the value from `from` to `to`, preserving
///   whatever type it was. If `to` already holds a value it is left as-is
///   (rename loses to explicit new value — caller's problem if this is wrong).
/// - `KindChanged{slug}` — leaves the value untouched. Validator will flag on
///   next user edit; no auto-coercion in v0.4.
pub async fn apply_changes(
    repo: &dyn Repository,
    site: SiteId,
    type_id: ContentTypeId,
    changes: &[FieldChange],
) -> StorageResult<u64> {
    if changes.is_empty() {
        return Ok(0);
    }
    let page = repo
        .content()
        .list(ContentQuery {
            site_id: Some(site),
            type_id: Some(type_id),
            per_page: Some(u32::MAX),
            ..Default::default()
        })
        .await?;

    let mut touched = 0u64;
    for mut content in page.items {
        let mut dirty = false;
        for change in changes {
            if apply_one(&mut content.data, change) {
                dirty = true;
            }
        }
        if dirty {
            content.updated_at = time::OffsetDateTime::now_utc();
            repo.content().upsert(content).await?;
            touched += 1;
        }
    }
    Ok(touched)
}

fn apply_one(
    data: &mut std::collections::BTreeMap<String, FieldValue>,
    change: &FieldChange,
) -> bool {
    match change {
        FieldChange::Added(slug) => {
            if !data.contains_key(slug) {
                data.insert(slug.clone(), FieldValue::Null);
                true
            } else {
                false
            }
        }
        FieldChange::Removed(slug) => data.remove(slug).is_some(),
        FieldChange::Renamed { from, to } => {
            if let Some(v) = data.remove(from) {
                data.entry(to.clone()).or_insert(v);
                true
            } else {
                false
            }
        }
        FieldChange::KindChanged { .. } => false,
    }
}
