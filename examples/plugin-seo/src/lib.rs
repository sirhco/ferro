//! SEO sidecar plugin.
//!
//! On `content.published`, derive Open Graph meta + JSON-LD structured data
//! from the content payload and write `/data/<type>/<slug>.json` inside the
//! plugin sandbox dir. The host preopens `<plugin_dir>/data` as `/data`, so
//! the file lands at `examples/starter-blog/plugins/seo/data/<type>/<slug>.json`.

wit_bindgen::generate!({
    world: "plugin",
    path: "../../crates/ferro-plugin/wit",
});

use exports::ferro::cms::guest::Guest;
use ferro::cms::host::{log, LogLevel};
use ferro::cms::types::{Content, HookEvent};

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde_json::{json, Value};

struct Component;

impl Guest for Component {
    fn init() -> Result<(), String> {
        log(LogLevel::Info, "plugin-seo", "loaded");
        Ok(())
    }

    fn on_event(evt: HookEvent) -> Result<(), String> {
        let HookEvent::ContentPublished(c) = evt else {
            return Ok(());
        };
        match write_sidecar(&c) {
            Ok(path) => log(
                LogLevel::Info,
                "plugin-seo",
                &format!("wrote {}", path.display()),
            ),
            Err(e) => log(
                LogLevel::Warn,
                "plugin-seo",
                &format!("failed to write sidecar for {}: {e}", c.slug),
            ),
        }
        Ok(())
    }
}

fn write_sidecar(c: &Content) -> Result<PathBuf, String> {
    let data: Value =
        serde_json::from_str(&c.data_json).map_err(|e| format!("invalid json: {e}"))?;
    let title = data
        .get("title")
        .or_else(|| data.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or(&c.slug)
        .to_string();
    let description = data
        .get("excerpt")
        .or_else(|| data.get("seo_description"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let image = data.get("cover_image_id").and_then(|v| v.as_str());

    let schema_type = match c.type_slug.as_str() {
        "post" => "Article",
        "product" => "Product",
        "event" => "Event",
        _ => "WebPage",
    };

    let mut json_ld = json!({
        "@context": "https://schema.org",
        "@type": schema_type,
        "name": title,
        "url": format!("/{}/{}", c.type_slug, c.slug),
    });
    if !description.is_empty() {
        json_ld["description"] = json!(description);
    }
    if let Some(id) = image {
        json_ld["image"] = json!(format!("/media/{id}"));
    }
    if c.type_slug == "product" {
        if let Some(price) = data.get("price_cents").and_then(|v| v.as_i64()) {
            let currency = data.get("currency").and_then(|v| v.as_str()).unwrap_or("USD");
            json_ld["offers"] = json!({
                "@type": "Offer",
                "price": format!("{:.2}", price as f64 / 100.0),
                "priceCurrency": currency,
            });
        }
    }
    if c.type_slug == "event" {
        if let Some(starts) = data.get("starts_at").and_then(|v| v.as_str()) {
            json_ld["startDate"] = json!(starts);
        }
        if let Some(ends) = data.get("ends_at").and_then(|v| v.as_str()) {
            json_ld["endDate"] = json!(ends);
        }
        if let Some(venue) = data.get("venue").and_then(|v| v.as_str()) {
            json_ld["location"] = json!({ "@type": "Place", "name": venue });
        }
    }

    let payload = json!({
        "open_graph": {
            "og:title": title,
            "og:description": description,
            "og:type": if c.type_slug == "post" { "article" } else { "website" },
            "og:url": format!("/{}/{}", c.type_slug, c.slug),
        },
        "json_ld": json_ld,
    });

    let dir = PathBuf::from(format!("/data/{}", c.type_slug));
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir failed: {e}"))?;
    let path = dir.join(format!("{}.json", c.slug));
    let mut f = fs::File::create(&path).map_err(|e| format!("create failed: {e}"))?;
    let bytes =
        serde_json::to_vec_pretty(&payload).map_err(|e| format!("encode failed: {e}"))?;
    f.write_all(&bytes).map_err(|e| format!("write failed: {e}"))?;
    Ok(path)
}

export!(Component);
