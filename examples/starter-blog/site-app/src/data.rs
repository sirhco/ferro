#![cfg(feature = "ssr")]
//! SSR-only API client. The site fetches all content at server render time;
//! the browser never imports reqwest.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ApiClient {
    base: String,
    http: reqwest::Client,
}

impl ApiClient {
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            http: reqwest::Client::builder()
                .user_agent("starter-site/0.0.1")
                .build()
                .expect("reqwest client"),
        }
    }

    pub async fn list_published(&self, type_slug: &str) -> Result<Vec<ContentEntry>, String> {
        let url = format!("{}/api/v1/content/{type_slug}?status=published", self.base);
        let res = self.http.get(&url).send().await.map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Ok(Vec::new());
        }
        let body: ListResponse = res.json().await.map_err(|e| e.to_string())?;
        Ok(body.items)
    }

    pub async fn get(&self, type_slug: &str, slug: &str) -> Result<Option<ContentEntry>, String> {
        let url = format!("{}/api/v1/content/{type_slug}/{slug}", self.base);
        let res = self.http.get(&url).send().await.map_err(|e| e.to_string())?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !res.status().is_success() {
            return Ok(None);
        }
        let entry: ContentEntry = res.json().await.map_err(|e| e.to_string())?;
        Ok(Some(entry))
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ContentEntry {
    #[serde(default)]
    pub id: String,
    pub slug: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub data: serde_json::Map<String, Value>,
    #[serde(default)]
    pub published_at: Option<String>,
}

impl ContentEntry {
    pub fn title(&self) -> String {
        self.data
            .get("title")
            .or_else(|| self.data.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or(&self.slug)
            .to_string()
    }

    pub fn excerpt(&self) -> Option<String> {
        self.data
            .get("excerpt")
            .or_else(|| self.data.get("seo_description"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    pub fn published_date(&self) -> String {
        self.published_at
            .as_deref()
            .map(|s| s.split('T').next().unwrap_or(s).to_string())
            .unwrap_or_default()
    }
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    #[serde(default)]
    items: Vec<ContentEntry>,
}
