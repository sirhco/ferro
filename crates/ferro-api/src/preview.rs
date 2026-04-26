//! Server-side draft preview. Renders the latest stored content for a
//! `(type_slug, slug)` pair as HTML — including drafts. Intended to be
//! embedded in the admin via `<iframe src="/preview/{type}/{slug}">`.
//!
//! Fields are rendered by kind:
//! * `RichText { format: Blocks }`  → [`ferro_editor::render_blocks_html`]
//! * `RichText { format: Markdown }` → [`ferro_editor::markdown::render_markdown`]
//! * other fields render as a labelled key/value row.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use ferro_core::{Content, ContentType, FieldDef, FieldKind, FieldValue, RichFormat, Site};
use serde_json::Value;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/preview/{type_slug}/{slug}", get(render_preview))
}

async fn render_preview(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path((type_slug, slug)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let (_site, ty, content) = resolve(&state, &type_slug, &slug).await?;
    let html = render_html(&ty, &content);
    Ok((
        [
            (CACHE_CONTROL, "no-store"),
            (CONTENT_TYPE, "text/html; charset=utf-8"),
        ],
        html,
    ))
}

async fn resolve(
    state: &AppState,
    type_slug: &str,
    slug: &str,
) -> ApiResult<(Site, ContentType, Content)> {
    let sites = state.repo.sites().list().await?;
    let site = sites.into_iter().next().ok_or(ApiError::NotFound)?;
    let ty = state
        .repo
        .types()
        .by_slug(site.id, type_slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let content = state
        .repo
        .content()
        .by_slug(site.id, ty.id, slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((site, ty, content))
}

fn render_html(ty: &ContentType, content: &Content) -> String {
    let title_field = ty.title_field.as_deref().unwrap_or("title");
    let title = match content.data.get(title_field) {
        Some(FieldValue::String(s)) => s.clone(),
        _ => content.slug.clone(),
    };
    let status = format!("{:?}", content.status).to_lowercase();

    let mut body = String::new();
    body.push_str(&format!(
        r#"<header class="preview-header"><span class="preview-pill preview-pill-{status}">{status}</span><h1>{}</h1><p class="preview-meta">{} · {}</p></header>"#,
        escape_html(&title),
        escape_html(&ty.name),
        escape_html(&content.slug),
    ));

    body.push_str(r#"<main class="preview-main">"#);
    for field in &ty.fields {
        if field.hidden {
            continue;
        }
        let val = content.data.get(&field.slug).cloned().map(field_to_json);
        body.push_str(&render_field(field, val.as_ref()));
    }
    body.push_str("</main>");

    wrap_document(&title, &body)
}

fn render_field(def: &FieldDef, val: Option<&Value>) -> String {
    let header = format!(
        r#"<section class="preview-field" data-field="{}"><h2>{}</h2>"#,
        escape_html(&def.slug),
        escape_html(&def.name),
    );
    let body = match (&def.kind, val) {
        (FieldKind::RichText { format: RichFormat::Blocks }, Some(v)) => {
            match serde_json::from_value::<ferro_editor::Document>(v.clone()) {
                Ok(doc) => ferro_editor::render_blocks_html(&doc, "/media"),
                Err(_) => format!("<pre>{}</pre>", escape_html(&v.to_string())),
            }
        }
        (FieldKind::RichText { format: RichFormat::Markdown }, Some(Value::String(s))) => {
            ferro_editor::markdown::render_markdown(s)
        }
        (FieldKind::RichText { .. }, Some(v)) => {
            format!("<pre>{}</pre>", escape_html(&v.to_string()))
        }
        (FieldKind::Media { .. }, Some(Value::String(id))) if !id.is_empty() => format!(
            r#"<figure><img src="/media/{}" alt="" /></figure>"#,
            escape_html(id)
        ),
        (FieldKind::Boolean, Some(Value::Bool(b))) => {
            format!(r#"<p>{}</p>"#, if *b { "✓" } else { "✗" })
        }
        (_, Some(Value::String(s))) if !s.is_empty() => {
            format!(r#"<p>{}</p>"#, escape_html(s))
        }
        (_, Some(Value::Number(n))) => format!(r#"<p>{n}</p>"#),
        (_, Some(v)) if !v.is_null() => {
            format!(
                r#"<pre>{}</pre>"#,
                escape_html(&serde_json::to_string_pretty(v).unwrap_or_default())
            )
        }
        _ => r#"<p class="preview-empty">—</p>"#.to_string(),
    };
    format!("{header}{body}</section>")
}

fn wrap_document(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Preview · {title}</title>
<style>
:root {{ color-scheme: light dark; --fg: #0f172a; --muted: #64748b; --bg: #f8fafc; --card: #ffffff; --pub: #16a34a; --draft: #f59e0b; --arch: #94a3b8; }}
@media (prefers-color-scheme: dark) {{ :root {{ --fg: #e2e8f0; --muted: #94a3b8; --bg: #0f172a; --card: #1e293b; }} }}
* {{ box-sizing: border-box; }}
html, body {{ margin: 0; padding: 0; background: var(--bg); color: var(--fg); font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif; line-height: 1.6; }}
.preview-header {{ padding: 2rem clamp(1rem, 5vw, 4rem) 1rem; border-bottom: 1px solid color-mix(in oklab, var(--fg) 10%, transparent); }}
.preview-header h1 {{ margin: .25rem 0 .5rem; font-size: clamp(1.6rem, 4vw, 2.4rem); }}
.preview-meta {{ color: var(--muted); margin: 0; font-size: .9rem; }}
.preview-pill {{ display: inline-block; padding: .15rem .5rem; border-radius: 999px; font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: white; }}
.preview-pill-published {{ background: var(--pub); }}
.preview-pill-draft {{ background: var(--draft); }}
.preview-pill-archived {{ background: var(--arch); }}
.preview-main {{ max-width: 70ch; margin: 0 auto; padding: 1rem clamp(1rem, 5vw, 4rem) 4rem; }}
.preview-field {{ margin-bottom: 2rem; }}
.preview-field h2 {{ margin: 0 0 .5rem; font-size: .8rem; font-weight: 600; text-transform: uppercase; letter-spacing: .08em; color: var(--muted); }}
.preview-field p {{ margin: .25rem 0; }}
.preview-empty {{ color: var(--muted); font-style: italic; }}
.preview-field img {{ max-width: 100%; height: auto; border-radius: .5rem; }}
.preview-field pre {{ background: var(--card); padding: 1rem; border-radius: .5rem; overflow-x: auto; font-size: .85rem; }}
.preview-field blockquote {{ margin: 1rem 0; padding: .5rem 1rem; border-left: 3px solid color-mix(in oklab, var(--fg) 20%, transparent); color: var(--muted); }}
.preview-field hr {{ border: none; border-top: 1px solid color-mix(in oklab, var(--fg) 12%, transparent); margin: 2rem 0; }}
</style>
</head>
<body>
{body}
</body>
</html>"#,
        title = escape_html(title),
        body = body,
    )
}

fn field_to_json(v: FieldValue) -> Value {
    serde_json::to_value(&v).unwrap_or(Value::Null)
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}
