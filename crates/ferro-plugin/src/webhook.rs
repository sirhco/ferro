//! HTTP webhook hook handler.
//!
//! Each `WebhookHook` POSTs a [`HookEvent`] JSON payload to a configured URL
//! whenever a matching event fires. Deliveries carry an `X-Ferro-Signature`
//! header — `hex(hmac_sha256(secret, body))` — so receivers can verify
//! authenticity. Filtering by event kind keeps high-volume endpoints from
//! drowning low-priority subscribers.

use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::PluginResult;
use crate::hook::{HookEvent, HookHandler};

/// Per-webhook configuration. Multiple webhooks can share the same URL with
/// different filters; the registry applies them independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Identifier used in logs.
    #[serde(default)]
    pub name: Option<String>,
    /// Target URL. Must be HTTP/HTTPS. Validated lazily on first call.
    pub url: String,
    /// Optional shared secret. When set, the request includes
    /// `X-Ferro-Signature: <hex hmac-sha256>`.
    #[serde(default)]
    pub secret: Option<String>,
    /// Subset of event kinds (`content.created`, `type.migrated`, …) to
    /// deliver. Empty list means deliver everything.
    #[serde(default)]
    pub events: Vec<String>,
    /// Request timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    10
}

#[derive(Debug)]
pub struct WebhookHook {
    pub config: WebhookConfig,
    pub client: reqwest::Client,
    pub allowed: HashSet<String>,
}

impl WebhookHook {
    /// Build a `WebhookHook` ready to register with [`crate::HookRegistry`].
    pub fn new(config: WebhookConfig) -> PluginResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| crate::PluginError::Other(e.to_string()))?;
        let allowed = config.events.iter().cloned().collect();
        Ok(Self { config, client, allowed })
    }

    fn matches(&self, kind: &str) -> bool {
        self.allowed.is_empty() || self.allowed.contains(kind)
    }
}

#[async_trait]
impl HookHandler for WebhookHook {
    async fn handle(&self, event: &HookEvent) -> PluginResult<()> {
        let kind = event_kind(event);
        if !self.matches(kind) {
            return Ok(());
        }
        let payload = serde_json::to_vec(event)
            .map_err(|e| crate::PluginError::Other(format!("encode: {e}")))?;

        let mut req = self.client.post(&self.config.url).body(payload.clone());
        req = req.header("Content-Type", "application/json");
        req = req.header("X-Ferro-Event", kind);
        if let Some(secret) = &self.config.secret {
            let sig = sign(secret.as_bytes(), &payload);
            req = req.header("X-Ferro-Signature", sig);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| crate::PluginError::Other(format!("send: {e}")))?;
        if !resp.status().is_success() {
            return Err(crate::PluginError::Other(format!(
                "webhook {} returned {}",
                self.config.url,
                resp.status()
            )));
        }
        Ok(())
    }

    fn name(&self) -> &str {
        self.config.name.as_deref().unwrap_or("webhook")
    }
}

/// `hex(hmac_sha256(secret, body))`. Stable, lowercase hex.
#[must_use]
pub fn sign(secret: &[u8], body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("hmac key length always valid");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

fn event_kind(evt: &HookEvent) -> &'static str {
    match evt {
        HookEvent::ContentCreated { .. } => "content.created",
        HookEvent::ContentUpdated { .. } => "content.updated",
        HookEvent::ContentPublished { .. } => "content.published",
        HookEvent::ContentDeleted { .. } => "content.deleted",
        HookEvent::TypeMigrated { .. } => "type.migrated",
        _ => "event",
    }
}
