#![cfg(feature = "ssr")]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SeoMeta {
    #[serde(default)]
    pub open_graph: serde_json::Map<String, Value>,
    #[serde(default)]
    pub json_ld: Value,
}

#[derive(Debug, Clone)]
pub struct SeoLoader {
    root: PathBuf,
}

impl SeoLoader {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub async fn load(&self, type_slug: &str, slug: &str) -> Option<SeoMeta> {
        let path = self.root.join(type_slug).join(format!("{slug}.json"));
        let bytes = tokio::fs::read(&path).await.ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

pub fn render_head(meta: &SeoMeta) -> String {
    let mut out = String::new();
    for (k, v) in &meta.open_graph {
        if let Some(s) = v.as_str() {
            out.push_str(&format!(
                r#"<meta property="{}" content="{}" />"#,
                escape_attr(k),
                escape_attr(s)
            ));
        }
    }
    if !meta.json_ld.is_null() {
        let ld = serde_json::to_string(&meta.json_ld).unwrap_or_else(|_| "{}".into());
        let safe = ld.replace("</script", "<\\/script");
        out.push_str(&format!(r#"<script type="application/ld+json">{safe}</script>"#));
    }
    out
}

fn escape_attr(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn renders_meta_tags_and_jsonld() {
        let meta = SeoMeta {
            open_graph: serde_json::Map::from_iter([
                ("og:title".into(), json!("Hello")),
                ("og:type".into(), json!("article")),
            ]),
            json_ld: json!({ "@type": "Article", "name": "Hello" }),
        };
        let html = render_head(&meta);
        assert!(html.contains(r#"<meta property="og:title" content="Hello" />"#));
        assert!(html.contains(r#"<script type="application/ld+json">"#));
    }

    #[test]
    fn breaks_out_of_script_tag_safely() {
        let meta = SeoMeta {
            open_graph: serde_json::Map::new(),
            json_ld: json!({ "name": "</script><script>alert(1)</script>" }),
        };
        let html = render_head(&meta);
        assert!(!html.contains("</script><script>"));
        assert!(html.contains("<\\/script"));
    }
}
